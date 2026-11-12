//! The scrobble rules as a pure state machine: player events in, "now
//! playing" and scrobble actions out. No I/O, the caller passes the time.
//!
//! What the player sends (see `PlayerService::play_all_in_queue`): every
//! track start is a `CurrentSongEvent` followed by `PlaybackStateEvent(PLAYING)`,
//! then one `SongTimeEvent` per second of decoded audio. Pause, stop and
//! source switches stop the playback thread (`STOPPED`); resuming starts a
//! new one, so the same `CurrentSongEvent` + `PLAYING` pair comes again, with
//! the first progress event at the paused position. Queries and likes also
//! re-broadcast the current song (without `PLAYING`), so only a song followed
//! by `PLAYING` counts as a start.
//!
//! Rules:
//! * Only library songs from the queue with artist and title
//!   ([`ScrobbleTrack::from_song`]); not radio, podcasts, or while this
//!   instance is a multiroom follower (the leader scrobbles).
//! * "Now playing" on every start or resume.
//! * One scrobble per play, once the listened time reaches half the track or
//!   4 minutes, whichever is less. Tracks of 30 seconds or less never count;
//!   with no known duration the 4 minute rule applies alone. The timestamp
//!   is when the play started.
//! * Listened time is the sum of 1-2 s steps between progress events while
//!   playing: a seek forward is a jump and does not count, a pause sends no
//!   progress.
//! * The same song starting again from the beginning (repeat one, play
//!   again after it ended, restart) is a new play; resuming it is not.

use api_models::playback_source::PlaybackSource;
use api_models::player::Song;
use api_models::state::{MultiroomRole, PlayerState, StateChangeEvent};

use crate::track::{Listen, ScrobbleTrack};

/// Tracks this long or shorter are never scrobbled (seconds).
pub const MIN_TRACK_SECS: u64 = 30;
/// Listening this long always scrobbles (seconds).
pub const MAX_THRESHOLD_SECS: u64 = 240;
/// Largest step between two progress events counted as listening.
const MAX_STEP_SECS: u64 = 2;
/// A same-song start whose first position is at most this, after the last
/// play got past [`RESTART_MIN_PREVIOUS_SECS`], is a new play.
const RESTART_MAX_POS_SECS: u64 = 2;
const RESTART_MIN_PREVIOUS_SECS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    NowPlaying(ScrobbleTrack),
    Scrobble(Listen),
}

#[derive(Debug)]
struct Play {
    file: String,
    /// `None` when this play is not reported (no artist/title, stream...).
    track: Option<ScrobbleTrack>,
    started_at: i64,
    listened_secs: u64,
    last_pos: Option<u64>,
    scrobbled: bool,
    /// Started again after a stop: the first progress event tells whether
    /// it resumed or started over.
    resumed: bool,
}

#[derive(Debug)]
pub struct Scrobbler {
    /// The player plays the queue (not a station or an episode).
    from_queue: bool,
    follower: bool,
    playing: bool,
    /// Last announced song, waiting for the `PLAYING` that starts it.
    pending: Option<Song>,
    play: Option<Play>,
}

impl Scrobbler {
    #[must_use]
    pub const fn new(source: &PlaybackSource) -> Self {
        Self {
            from_queue: source.is_queue(),
            follower: false,
            playing: false,
            pending: None,
            play: None,
        }
    }

    /// Events were lost: the next progress event only sets the position.
    pub const fn on_lagged(&mut self) {
        if let Some(play) = self.play.as_mut() {
            play.last_pos = None;
        }
    }

    /// Feeds one event; `now` is the current unix time in seconds.
    pub fn handle(&mut self, event: &StateChangeEvent, now: i64) -> Option<Action> {
        match event {
            StateChangeEvent::PlaybackSourceEvent(source) => {
                self.from_queue = source.is_queue();
                None
            }
            StateChangeEvent::MultiroomGroupEvent(group) => {
                self.follower = group.role == MultiroomRole::Follower;
                None
            }
            StateChangeEvent::CurrentSongEvent(song) => {
                self.pending = Some(song.clone());
                None
            }
            StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING) => self.on_playing(now),
            StateChangeEvent::PlaybackStateEvent(_) => {
                self.playing = false;
                None
            }
            StateChangeEvent::SongTimeEvent(progress) => {
                self.on_progress(progress.current_time.as_secs(), progress.total_time.as_secs(), now)
            }
            _ => None,
        }
    }

    const fn reportable(&self) -> bool {
        self.from_queue && !self.follower
    }

    fn on_playing(&mut self, now: i64) -> Option<Action> {
        self.playing = true;
        let song = self.pending.take()?;
        let track = if self.reportable() { ScrobbleTrack::from_song(&song) } else { None };
        match self.play.as_mut() {
            Some(play) if play.file == song.file => {
                play.resumed = true;
                if track.is_some() && play.track.is_none() {
                    play.track.clone_from(&track);
                }
            }
            _ => {
                self.play = Some(Play {
                    file: song.file,
                    track: track.clone(),
                    started_at: now,
                    listened_secs: 0,
                    last_pos: None,
                    scrobbled: false,
                    resumed: false,
                });
            }
        }
        track.map(Action::NowPlaying)
    }

    fn on_progress(&mut self, pos: u64, total: u64, now: i64) -> Option<Action> {
        if !self.playing {
            return None;
        }
        let reportable = self.reportable();
        let play = self.play.as_mut()?;
        if let Some(track) = play.track.as_mut()
            && track.duration_secs.is_none()
            && total > 0
        {
            track.duration_secs = Some(total);
        }
        if play.resumed {
            play.resumed = false;
            if pos <= RESTART_MAX_POS_SECS && play.last_pos.unwrap_or(0) >= RESTART_MIN_PREVIOUS_SECS {
                play.started_at = now - i64::try_from(pos).unwrap_or(0);
                play.listened_secs = 0;
                play.scrobbled = false;
                play.last_pos = Some(pos);
                return None;
            }
        }
        if let Some(last) = play.last_pos
            && pos > last
            && pos - last <= MAX_STEP_SECS
        {
            play.listened_secs += pos - last;
        }
        play.last_pos = Some(pos);

        if play.scrobbled || !reportable {
            return None;
        }
        let track = play.track.as_ref()?;
        let threshold = match track.duration_secs {
            Some(d) if d <= MIN_TRACK_SECS => return None,
            Some(d) => (d / 2).min(MAX_THRESHOLD_SECS),
            None => MAX_THRESHOLD_SECS,
        };
        if play.listened_secs < threshold {
            return None;
        }
        play.scrobbled = true;
        Some(Action::Scrobble(Listen {
            track: track.clone(),
            listened_at: play.started_at,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use api_models::radio::RadioStation;
    use api_models::state::{MultiroomGroupState, SongProgress};

    use super::*;

    const T0: i64 = 1_700_000_000;

    fn song(file: &str, secs: Option<u64>) -> Song {
        Song {
            artist: Some("Artist".into()),
            title: Some(format!("Title {file}")),
            album: Some("Album".into()),
            time: secs.map(Duration::from_secs),
            file: file.into(),
            ..Default::default()
        }
    }

    /// Drives a [`Scrobbler`] the way the player does, with a fake clock.
    struct Player {
        s: Scrobbler,
        now: i64,
        total: u64,
        actions: Vec<Action>,
    }

    impl Player {
        fn new() -> Self {
            Self {
                s: Scrobbler::new(&PlaybackSource::Queue),
                now: T0,
                total: 0,
                actions: vec![],
            }
        }
        fn send(&mut self, e: StateChangeEvent) {
            if let Some(a) = self.s.handle(&e, self.now) {
                self.actions.push(a);
            }
        }
        fn start(&mut self, song: &Song) {
            self.total = song.time.map_or(0, |d| d.as_secs());
            self.send(StateChangeEvent::CurrentSongEvent(song.clone()));
            self.send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING));
        }
        fn at(&mut self, pos: u64) {
            self.send(StateChangeEvent::SongTimeEvent(SongProgress {
                total_time: Duration::from_secs(self.total),
                current_time: Duration::from_secs(pos),
            }));
        }
        /// Plays positions `from..=to`, one second each.
        fn play(&mut self, from: u64, to: u64) {
            for pos in from..=to {
                self.at(pos);
                self.now += 1;
            }
        }
        fn stop(&mut self) {
            self.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED));
        }
        fn scrobbles(&self) -> Vec<&Listen> {
            self.actions
                .iter()
                .filter_map(|a| if let Action::Scrobble(l) = a { Some(l) } else { None })
                .collect()
        }
        fn now_playing(&self) -> usize {
            self.actions.iter().filter(|a| matches!(a, Action::NowPlaying(_))).count()
        }
    }

    #[test]
    fn now_playing_on_start_and_scrobble_at_half() {
        let mut p = Player::new();
        let s = song("a", Some(200));
        p.start(&s);
        assert_eq!(p.now_playing(), 1);
        p.play(0, 99);
        assert!(p.scrobbles().is_empty(), "99 s listened of 200");
        p.play(100, 101);
        let scrobbles = p.scrobbles();
        assert_eq!(scrobbles.len(), 1);
        assert_eq!(scrobbles[0].listened_at, T0);
        assert_eq!(scrobbles[0].track.title, "Title a");
        p.play(102, 200);
        assert_eq!(p.scrobbles().len(), 1, "one scrobble per play");
    }

    #[test]
    fn four_minutes_is_enough_for_long_tracks() {
        let mut p = Player::new();
        p.start(&song("long", Some(3600)));
        p.play(0, 239);
        assert!(p.scrobbles().is_empty());
        p.play(240, 240);
        assert_eq!(p.scrobbles().len(), 1);
    }

    #[test]
    fn short_tracks_never_scrobble() {
        let mut p = Player::new();
        p.start(&song("short", Some(30)));
        p.play(0, 30);
        assert!(p.scrobbles().is_empty());
        assert_eq!(p.now_playing(), 1);
    }

    #[test]
    fn unknown_duration_uses_four_minutes() {
        let mut p = Player::new();
        p.start(&song("nodur", None));
        p.play(0, 200);
        assert!(p.scrobbles().is_empty());
        p.play(201, 241);
        assert_eq!(p.scrobbles().len(), 1);
    }

    #[test]
    fn duration_from_progress_when_song_has_none() {
        let mut p = Player::new();
        p.start(&song("tagless", None));
        p.total = 100;
        p.play(0, 51);
        let scrobbles = p.scrobbles();
        assert_eq!(scrobbles.len(), 1);
        assert_eq!(scrobbles[0].track.duration_secs, Some(100));
    }

    #[test]
    fn pause_stops_counting_and_resume_continues_the_play() {
        let mut p = Player::new();
        p.start(&song("a", Some(200)));
        p.play(0, 60);
        p.stop();
        p.now += 3600; // paused for an hour, no progress meanwhile
        p.start(&song("a", Some(200)));
        assert_eq!(p.now_playing(), 2, "now playing again on resume");
        p.play(60, 99);
        assert!(p.scrobbles().is_empty(), "99 s listened, 100 needed");
        p.play(100, 100);
        let scrobbles = p.scrobbles();
        assert_eq!(scrobbles.len(), 1);
        assert_eq!(scrobbles[0].listened_at, T0, "timestamp of the original start");
    }

    #[test]
    fn seek_forward_does_not_count() {
        let mut p = Player::new();
        p.start(&song("a", Some(300)));
        p.play(0, 10);
        p.play(200, 250); // jumped from 10 to 200
        assert!(p.scrobbles().is_empty(), "only ~60 s heard");
        p.play(251, 345);
        assert_eq!(p.scrobbles().len(), 1, "150 s heard now");
    }

    #[test]
    fn seek_back_does_not_double_count_but_replayed_parts_count() {
        let mut p = Player::new();
        p.start(&song("a", Some(300)));
        p.play(0, 100);
        p.play(0, 49); // seeked back to 0: 49 more seconds heard
        assert!(p.scrobbles().is_empty(), "149 s");
        p.play(50, 52);
        assert_eq!(p.scrobbles().len(), 1);
    }

    #[test]
    fn skip_before_threshold_does_not_scrobble() {
        let mut p = Player::new();
        p.start(&song("a", Some(200)));
        p.play(0, 50);
        p.start(&song("b", Some(200)));
        p.play(0, 10);
        assert!(p.scrobbles().is_empty());
        assert_eq!(p.now_playing(), 2);
    }

    #[test]
    fn repeat_of_the_same_track_is_a_new_play() {
        let mut p = Player::new();
        let s = song("a", Some(100));
        p.start(&s);
        p.play(0, 100);
        // Repeat one: the next start of the same song comes while playing.
        let second_start = p.now;
        p.start(&s);
        p.play(0, 60);
        let scrobbles = p.scrobbles();
        assert_eq!(scrobbles.len(), 2);
        assert_eq!(scrobbles[1].listened_at, second_start);
    }

    #[test]
    fn restart_after_stop_is_a_new_play() {
        let mut p = Player::new();
        let s = song("a", Some(100));
        p.start(&s);
        p.play(0, 100);
        p.stop(); // end of queue
        p.now += 600;
        p.start(&s);
        p.play(0, 51);
        assert_eq!(p.scrobbles().len(), 2);
    }

    #[test]
    fn rebroadcast_of_the_current_song_changes_nothing() {
        let mut p = Player::new();
        let s = song("a", Some(200));
        p.start(&s);
        p.play(0, 50);
        // QueryCurrentSong / like: song without PLAYING.
        p.send(StateChangeEvent::CurrentSongEvent(s.clone()));
        p.play(51, 101);
        assert_eq!(p.scrobbles().len(), 1);
        assert_eq!(p.now_playing(), 1);
    }

    #[test]
    fn radio_and_podcasts_are_ignored() {
        let mut p = Player::new();
        p.send(StateChangeEvent::PlaybackSourceEvent(PlaybackSource::Radio(RadioStation::default())));
        let mut station = song("https://radio.example/stream", None);
        station.artist = Some("ICY Artist".into());
        p.start(&station);
        p.play(0, 400);
        p.send(StateChangeEvent::PlaybackSourceEvent(PlaybackSource::Podcast {
            podcast_id: "p".into(),
            episode_id: "e".into(),
        }));
        p.start(&song("local-file-but-podcast-source", Some(100)));
        p.play(0, 100);
        // A podcast episode added to the queue.
        p.send(StateChangeEvent::PlaybackSourceEvent(PlaybackSource::Queue));
        let mut episode = song("https://pod.example/ep.mp3", Some(100));
        episode.tags.insert(api_models::podcast::EPISODE_ID_TAG.into(), "e".into());
        p.start(&episode);
        p.play(0, 100);
        assert!(p.actions.is_empty(), "{:?}", p.actions);
    }

    #[test]
    fn songs_without_artist_are_ignored() {
        let mut p = Player::new();
        let mut s = song("a", Some(100));
        s.artist = None;
        p.start(&s);
        p.play(0, 100);
        assert!(p.actions.is_empty());
    }

    #[test]
    fn multiroom_follower_does_not_scrobble() {
        let mut p = Player::new();
        p.send(StateChangeEvent::MultiroomGroupEvent(MultiroomGroupState {
            role: MultiroomRole::Follower,
            ..Default::default()
        }));
        p.start(&song("a", Some(100)));
        p.play(0, 100);
        assert!(p.actions.is_empty());
        p.send(StateChangeEvent::MultiroomGroupEvent(MultiroomGroupState::default()));
        p.start(&song("b", Some(100)));
        p.play(0, 60);
        assert_eq!(p.scrobbles().len(), 1);
    }

    #[test]
    fn lagged_receiver_rebases_without_counting_the_gap() {
        let mut p = Player::new();
        p.start(&song("a", Some(200)));
        p.play(0, 10);
        p.s.on_lagged();
        p.play(12, 101); // 11 was lost: 10 -> 12 is not counted
        assert!(p.scrobbles().is_empty(), "99 s");
        p.play(102, 102);
        assert_eq!(p.scrobbles().len(), 1);
    }

    #[test]
    fn progress_while_stopped_is_ignored() {
        let mut p = Player::new();
        p.start(&song("a", Some(100)));
        p.play(0, 10);
        p.stop();
        p.play(11, 100);
        assert!(p.scrobbles().is_empty());
    }
}
