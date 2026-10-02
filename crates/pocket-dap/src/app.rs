use api_models::common::{
    ArtistPageMode, MetadataCommand, MetadataLibraryItem, PlayerCommand, PlaylistCommand, QueueCommand, SystemRequest, UserCommand, Volume,
};
use api_models::player::Song;
use api_models::playlist::PlaylistPage;
use api_models::state::{CurrentQueueQuery, PlayerState, StateChangeEvent};

use crate::input::Input;
use crate::theme::Behavior;

pub const MENU_ITEMS: [&str; 3] = ["Now Playing", "Library", "Queue"];
pub const LIBRARY_ITEMS: [&str; 5] = ["Artists", "Albums", "Playlists", "Tracks", "Search"];
pub const ITEM_ACTION_ITEMS: [&str; 3] = ["Play", "Play Next", "Add to queue"];
pub const QUEUE_ACTION_ITEMS: [&str; 3] = ["Play", "Play Next", "Remove"];
const SEARCH_LIMIT: usize = 80;
const ARTIST_PAGE: usize = 48;
const SEARCH_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789 -'";
/// A–Z for list letter jump / scrub (mPod-style).
const LETTER_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
/// About two seconds at the window UI's 30 fps. `tick` runs once per frame.
const VOLUME_OVERLAY_FRAMES: u16 = 60;
/// Wheel steps inside [`SCRUB_ARM_IDLE`] frames to enter letter scrub.
const SCRUB_ARM_STEPS: u8 = 4;
const SCRUB_ARM_IDLE: u16 = 12;
/// Idle frames after the last scrub step before returning to row scroll.
const SCRUB_EXIT_FRAMES: u16 = 18;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListBack {
    Artist,
    Search,
    AllAlbums,
    Playlists,
    AllTracks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Library,
    Search,
    NowPlaying,
    Artists,
    Albums {
        artist: String,
    },
    AllAlbums,
    Playlists,
    AllTracks,
    Tracks {
        artist: String,
        album_id: String,
        back: ListBack,
    },
    TrackActions {
        artist: String,
        album_id: String,
        file: String,
        back: ListBack,
    },
    AlbumActions {
        artist: String,
        album_id: String,
        back: ListBack,
    },
    Queue,
    QueueActions {
        file: String,
    },
}

pub struct App {
    pub screen: Screen,
    pub behavior: Behavior,
    pub cursor: usize,
    pub connected: bool,
    pub paused: bool,
    pub volume: Volume,
    volume_overlay: u16,
    pub song: Option<Song>,
    pub elapsed_secs: u32,
    pub total_secs: u32,
    artists: Vec<MetadataLibraryItem>,
    albums: Vec<MetadataLibraryItem>,
    tracks: Vec<MetadataLibraryItem>,
    playlists: Vec<(String, String)>,
    search_results: Vec<MetadataLibraryItem>,
    search_query: String,
    search_editing: bool,
    search_picker: usize,
    from_search: bool,
    queue: Option<PlaylistPage>,
    artist_offset: usize,
    artist_total: usize,
    artist_after_load: AfterLoad,
    song_offset: usize,
    song_total: usize,
    song_after_load: AfterLoad,
    shown_artist: String,
    shown_album: String,
    marquee_px: i32,
    marquee_text: String,
    letter_ui: LetterUi,
    letter_picker: usize,
    letter_pending: Option<char>,
    wheel_burst: u8,
    wheel_burst_idle: u16,
}

enum AfterLoad {
    Clamp,
    First,
    Global(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LetterUi {
    Off,
    /// Hold-Menu picker: wheel chooses a letter, Center jumps.
    Picker,
    /// Fast-wheel scrub: overlay letter, steps change it and jump.
    Scrub {
        idle: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StatusCaption {
    None,
    Fixed(String),
    Scroll(String),
}

fn fixed_name(name: Option<String>) -> StatusCaption {
    match name {
        Some(name) if !name.is_empty() => StatusCaption::Fixed(name),
        _ => StatusCaption::None,
    }
}

fn album_line(artist: &str, album: &str) -> String {
    if artist.is_empty() {
        album.to_string()
    } else {
        format!("{artist} - {album}")
    }
}

fn album_pair(item: &MetadataLibraryItem, fallback_artist: &str) -> (String, String) {
    match item {
        MetadataLibraryItem::Album { name, artist, .. } => {
            let artist = if artist.is_empty() {
                fallback_artist.to_string()
            } else {
                artist.clone()
            };
            (artist, name.clone())
        }
        _ => (fallback_artist.to_string(), item.get_title()),
    }
}

fn caption_for_album(item: &MetadataLibraryItem, fallback_artist: &str) -> StatusCaption {
    let (artist, album) = album_pair(item, fallback_artist);
    if album.is_empty() {
        fixed_name((!artist.is_empty()).then_some(artist))
    } else {
        StatusCaption::Scroll(album_line(&artist, &album))
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::NowPlaying,
            behavior: Behavior::PocketDap,
            cursor: 0,
            connected: false,
            paused: true,
            volume: Volume::default(),
            volume_overlay: 0,
            song: None,
            elapsed_secs: 0,
            total_secs: 0,
            artists: Vec::new(),
            albums: Vec::new(),
            tracks: Vec::new(),
            playlists: Vec::new(),
            search_results: Vec::new(),
            search_query: String::new(),
            search_editing: true,
            search_picker: 0,
            from_search: false,
            queue: None,
            artist_offset: 0,
            artist_total: 0,
            artist_after_load: AfterLoad::Clamp,
            song_offset: 0,
            song_total: 0,
            song_after_load: AfterLoad::Clamp,
            shown_artist: String::new(),
            shown_album: String::new(),
            marquee_px: 0,
            marquee_text: String::new(),
            letter_ui: LetterUi::Off,
            letter_picker: 0,
            letter_pending: None,
            wheel_burst: 0,
            wheel_burst_idle: 0,
        }
    }

    pub fn title(&self) -> &'static str {
        match self.screen {
            Screen::Menu => "MENU",
            Screen::Library => "LIBRARY",
            Screen::Search => "SEARCH",
            Screen::NowPlaying => "NOW PLAYING",
            Screen::Artists => "ARTISTS",
            Screen::Albums { .. } | Screen::AllAlbums => "ALBUMS",
            Screen::Playlists => "PLAYLISTS",
            Screen::Tracks { .. } | Screen::AllTracks => "TRACKS",
            Screen::TrackActions { .. } | Screen::AlbumActions { .. } => "ACTIONS",
            Screen::Queue => "QUEUE",
            Screen::QueueActions { .. } => "QUEUE ITEM",
        }
    }

    pub fn song_title(&self) -> String {
        self.song.as_ref().map(Song::get_title).unwrap_or_default()
    }

    pub fn song_artist(&self) -> String {
        self.song.as_ref().and_then(|s| s.artist.clone()).unwrap_or_default()
    }

    pub fn song_album(&self) -> String {
        self.song.as_ref().and_then(|s| s.album.clone()).unwrap_or_default()
    }

    pub fn progress_percent(&self) -> u8 {
        if self.total_secs == 0 {
            return 0;
        }
        ((self.elapsed_secs * 100) / self.total_secs).min(100) as u8
    }

    pub fn volume_percent(&self) -> u8 {
        let max = u16::from(self.volume.max.max(1));
        ((u16::from(self.volume.current) * 100) / max).min(100) as u8
    }

    pub fn volume_overlay_visible(&self) -> bool {
        self.volume_overlay > 0
    }

    /// Large A–Z overlay while picking or scrubbing on a long library list.
    pub fn letter_overlay(&self) -> Option<char> {
        match self.letter_ui {
            LetterUi::Off => None,
            LetterUi::Picker | LetterUi::Scrub { .. } => Some(self.letter_char()),
        }
    }

    pub fn letter_picking(&self) -> bool {
        matches!(self.letter_ui, LetterUi::Picker)
    }

    fn letter_char(&self) -> char {
        let alphabet = self.letter_alphabet();
        alphabet.get(self.letter_picker).copied().unwrap_or('a')
    }

    pub(crate) fn on_letter_list(&self) -> bool {
        matches!(
            self.screen,
            Screen::Artists | Screen::AllAlbums | Screen::Playlists | Screen::AllTracks
        )
    }

    /// Letters the picker/scrub may stop on for Artists, Albums, Playlists,
    /// and Tracks: only first letters that appear in the current (loaded)
    /// list. Falls back to A–Z when the list has no alphabetic titles.
    fn letter_alphabet(&self) -> Vec<char> {
        let present = Self::letters_from_titles(self.list_labels());
        if present.is_empty() {
            LETTER_CHARS.iter().map(|&b| b as char).collect()
        } else {
            present
        }
    }

    fn letters_from_titles(titles: impl IntoIterator<Item = String>) -> Vec<char> {
        let mut seen = [false; 26];
        for title in titles {
            if let Some(c) = title.chars().find(|c| c.is_alphabetic()) {
                let i = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a') as usize;
                if i < 26 {
                    seen[i] = true;
                }
            }
        }
        LETTER_CHARS
            .iter()
            .enumerate()
            .filter(|&(i, _)| seen[i])
            .map(|(_, &b)| b as char)
            .collect()
    }

    pub fn tick(&mut self) {
        self.volume_overlay = self.volume_overlay.saturating_sub(1);
        if self.wheel_burst > 0 {
            self.wheel_burst_idle = self.wheel_burst_idle.saturating_add(1);
            if self.wheel_burst_idle >= SCRUB_ARM_IDLE {
                self.wheel_burst = 0;
                self.wheel_burst_idle = 0;
            }
        }
        if let LetterUi::Scrub { idle } = self.letter_ui {
            let next = idle.saturating_add(1);
            if next >= SCRUB_EXIT_FRAMES {
                self.letter_ui = LetterUi::Off;
            } else {
                self.letter_ui = LetterUi::Scrub { idle: next };
            }
        }
        match self.status_caption() {
            StatusCaption::Scroll(text) => {
                if text != self.marquee_text {
                    self.marquee_text = text;
                    self.marquee_px = 0;
                } else {
                    self.marquee_px = self.marquee_px.saturating_add(1);
                }
            }
            _ => {
                self.marquee_text.clear();
                self.marquee_px = 0;
            }
        }
    }

    pub(crate) fn marquee_px(&self) -> i32 {
        self.marquee_px
    }

    /// Top-bar text. An album line scrolls. Every screen names what is open.
    pub(crate) fn status_caption(&self) -> StatusCaption {
        let specific = match &self.screen {
            Screen::NowPlaying => StatusCaption::Fixed("Now playing".to_string()),
            Screen::Artists => fixed_name(self.artists.get(self.cursor).map(MetadataLibraryItem::get_title)),
            Screen::Albums { artist } => self.album_caption(&self.albums, artist),
            Screen::AllAlbums => self.album_caption(&self.albums, ""),
            Screen::Playlists => fixed_name(self.playlists.get(self.cursor).map(|(_, name)| name.clone())),
            Screen::AllTracks => fixed_name(self.tracks.get(self.cursor).map(MetadataLibraryItem::get_title)),
            Screen::Queue => fixed_name(
                self.queue
                    .as_ref()
                    .and_then(|page| page.items.get(self.cursor))
                    .map(|song| song.get_title()),
            ),
            Screen::Search => match self.search_results.get(self.cursor) {
                Some(MetadataLibraryItem::Artist { name }) => StatusCaption::Fixed(name.clone()),
                Some(item @ MetadataLibraryItem::Album { .. }) => caption_for_album(item, ""),
                Some(MetadataLibraryItem::SongItem(song)) => StatusCaption::Fixed(song.get_title()),
                _ => StatusCaption::None,
            },
            Screen::Tracks { .. } | Screen::TrackActions { .. } | Screen::AlbumActions { .. } if !self.shown_album.is_empty() => {
                StatusCaption::Scroll(album_line(&self.shown_artist, &self.shown_album))
            }
            _ => StatusCaption::None,
        };
        match specific {
            StatusCaption::None => StatusCaption::Fixed(self.place_label().to_string()),
            other => other,
        }
    }

    fn place_label(&self) -> &'static str {
        match self.screen {
            Screen::NowPlaying => "Now playing",
            Screen::Menu => "Menu",
            Screen::Library => "Library",
            Screen::Search => "Search",
            Screen::Artists => "Artists",
            Screen::Albums { .. } | Screen::AllAlbums => "Albums",
            Screen::Playlists => "Playlists",
            Screen::Tracks { .. } | Screen::AllTracks => "Tracks",
            Screen::TrackActions { .. } | Screen::AlbumActions { .. } => "Actions",
            Screen::Queue => "Queue",
            Screen::QueueActions { .. } => "Queue item",
        }
    }

    fn album_caption(&self, items: &[MetadataLibraryItem], fallback_artist: &str) -> StatusCaption {
        let Some(item) = items.get(self.cursor) else {
            return fixed_name((!fallback_artist.is_empty()).then(|| fallback_artist.to_string()));
        };
        caption_for_album(item, fallback_artist)
    }

    fn note_album(&mut self, artist: &str, album: &str) {
        self.shown_artist = artist.to_string();
        self.shown_album = album.to_string();
    }

    fn arm_volume_overlay(&mut self) {
        self.volume_overlay = VOLUME_OVERLAY_FRAMES;
    }

    pub fn list_labels(&self) -> Vec<String> {
        match &self.screen {
            Screen::NowPlaying => Vec::new(),
            Screen::Menu => MENU_ITEMS.iter().map(|s| (*s).to_string()).collect(),
            Screen::Library => LIBRARY_ITEMS.iter().map(|s| (*s).to_string()).collect(),
            Screen::Search => self.search_labels(),
            Screen::Artists => self.artists.iter().map(MetadataLibraryItem::get_title).collect(),
            Screen::Albums { .. } | Screen::AllAlbums => self.albums.iter().map(MetadataLibraryItem::get_title).collect(),
            Screen::Playlists => self.playlists.iter().map(|(_, name)| name.clone()).collect(),
            Screen::Tracks { .. } | Screen::AllTracks => self.tracks.iter().map(MetadataLibraryItem::get_title).collect(),
            Screen::TrackActions { .. } | Screen::AlbumActions { .. } => ITEM_ACTION_ITEMS.iter().map(|s| (*s).to_string()).collect(),
            Screen::Queue => self.queue_labels(),
            Screen::QueueActions { .. } => QUEUE_ACTION_ITEMS.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    pub fn list_drills(&self) -> bool {
        matches!(
            self.screen,
            Screen::Library | Screen::Artists | Screen::Albums { .. } | Screen::AllAlbums | Screen::Playlists | Screen::Search
        )
    }

    pub fn on_search(&self) -> bool {
        matches!(self.screen, Screen::Search)
    }

    pub fn search_query_line(&self) -> String {
        let ch = SEARCH_CHARS.get(self.search_picker).copied().unwrap_or(b'a') as char;
        if self.search_editing {
            format!("{}_  [{ch}]", self.search_query)
        } else {
            self.search_query.clone()
        }
    }

    pub fn search_highlight_results(&self) -> bool {
        self.on_search() && !self.search_editing
    }

    fn search_labels(&self) -> Vec<String> {
        self.search_results
            .iter()
            .map(|item| match item {
                MetadataLibraryItem::Artist { .. } => format!("ART {}", item.get_title()),
                MetadataLibraryItem::Album { .. } => format!("ALB {}", item.get_title()),
                MetadataLibraryItem::SongItem(_) => format!("TRK {}", item.get_title()),
                other => other.get_title(),
            })
            .collect()
    }

    fn queue_labels(&self) -> Vec<String> {
        let current = self.song.as_ref().map(|s| s.file.as_str());
        self.queue
            .as_ref()
            .map(|page| {
                page.items
                    .iter()
                    .map(|s| {
                        let mark = if current == Some(s.file.as_str()) { "> " } else { "  " };
                        format!("{mark}{}", s.get_title())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn list_len(&self) -> usize {
        match &self.screen {
            Screen::NowPlaying => 0,
            Screen::Menu => MENU_ITEMS.len(),
            Screen::Library => LIBRARY_ITEMS.len(),
            Screen::Search => self.search_results.len(),
            Screen::Artists => self.artists.len(),
            Screen::Albums { .. } | Screen::AllAlbums => self.albums.len(),
            Screen::Playlists => self.playlists.len(),
            Screen::Tracks { .. } | Screen::AllTracks => self.tracks.len(),
            Screen::TrackActions { .. } | Screen::AlbumActions { .. } => ITEM_ACTION_ITEMS.len(),
            Screen::Queue => self.queue.as_ref().map_or(0, |p| p.items.len()),
            Screen::QueueActions { .. } => QUEUE_ACTION_ITEMS.len(),
        }
    }

    pub fn apply_event(&mut self, event: StateChangeEvent) -> Vec<UserCommand> {
        let mut follow_up = Vec::new();
        match event {
            StateChangeEvent::CurrentSongEvent(song) => {
                self.total_secs = song.time.map_or(0, |d| d.as_secs() as u32);
                self.song = Some(song);
            }
            StateChangeEvent::SongTimeEvent(progress) => {
                self.elapsed_secs = progress.current_time.as_secs() as u32;
                self.total_secs = progress.total_time.as_secs() as u32;
            }
            StateChangeEvent::VolumeChangeEvent(volume) => self.volume = volume,
            StateChangeEvent::PlaybackStateEvent(state) => {
                self.paused = !matches!(state, PlayerState::PLAYING);
            }
            StateChangeEvent::CurrentQueueEvent(page) => self.queue = page,
            StateChangeEvent::MetadataLocalItems(items) => match self.screen {
                Screen::Search => {
                    self.search_results = items;
                    self.clamp_cursor();
                }
                Screen::Artists => {
                    self.artists = items;
                    self.clamp_cursor();
                }
                Screen::Albums { .. } | Screen::AllAlbums => {
                    self.albums = items;
                    self.clamp_cursor();
                }
                Screen::Tracks { .. } => {
                    self.tracks = items;
                    self.clamp_cursor();
                }
                _ => {}
            },
            StateChangeEvent::MetadataArtistsPage { items, offset, total } => {
                if matches!(self.screen, Screen::Artists) {
                    self.artists = items;
                    self.artist_offset = offset;
                    self.artist_total = total;
                    match self.artist_after_load {
                        AfterLoad::First => self.cursor = 0,
                        AfterLoad::Global(g) => {
                            self.cursor = g.saturating_sub(offset).min(self.artists.len().saturating_sub(1));
                        }
                        AfterLoad::Clamp => self.clamp_cursor(),
                    }
                    self.artist_after_load = AfterLoad::Clamp;
                    follow_up = self.finish_letter_pending_artists();
                }
            }
            StateChangeEvent::MetadataSongsPage { items, offset, total } => {
                if matches!(self.screen, Screen::AllTracks) {
                    self.tracks = items;
                    self.song_offset = offset;
                    self.song_total = total;
                    match self.song_after_load {
                        AfterLoad::First => self.cursor = 0,
                        AfterLoad::Global(g) => {
                            self.cursor = g.saturating_sub(offset).min(self.tracks.len().saturating_sub(1));
                        }
                        AfterLoad::Clamp => self.clamp_cursor(),
                    }
                    self.song_after_load = AfterLoad::Clamp;
                    follow_up = self.finish_letter_pending_songs();
                }
            }
            StateChangeEvent::SavedPlaylists(items) => {
                if matches!(self.screen, Screen::Playlists) {
                    self.playlists = items.into_iter().map(|pl| (pl.id, pl.name)).collect();
                    self.clamp_cursor();
                }
            }
            _ => {}
        }
        follow_up
    }

    pub fn on_connected(&mut self) -> Vec<UserCommand> {
        self.connected = true;
        vec![
            UserCommand::Queue(QueueCommand::QueryCurrentSong),
            UserCommand::Player(PlayerCommand::QueryCurrentPlayerInfo),
            UserCommand::System(SystemRequest::QueryCurrentVolume),
        ]
    }

    pub fn on_disconnected(&mut self) {
        self.connected = false;
    }

    pub fn handle(&mut self, input: Input) -> Option<Vec<UserCommand>> {
        match input {
            Input::Quit => return None,
            Input::VolDown | Input::VolUp => {
                self.arm_volume_overlay();
                let request = if input == Input::VolDown {
                    SystemRequest::VolDown
                } else {
                    SystemRequest::VolUp
                };
                return Some(vec![UserCommand::System(request)]);
            }
            Input::Char(c) => return Some(self.type_search(c)),
            Input::Backspace => return Some(self.delete_search_char()),
            Input::MenuHold => {
                if self.on_letter_list() {
                    self.open_letter_picker();
                }
            }
            Input::Menu => {
                if self.letter_ui != LetterUi::Off {
                    self.clear_letter_ui();
                } else {
                    self.open_menu();
                }
            }
            Input::Play => return Some(vec![UserCommand::Player(PlayerCommand::TogglePlay)]),
            Input::Back => {
                if self.letter_ui != LetterUi::Off {
                    self.clear_letter_ui();
                    return Some(Vec::new());
                }
                self.back();
                return Some(Vec::new());
            }
            Input::StepLeft => return Some(self.wheel(-1)),
            Input::StepRight => return Some(self.wheel(1)),
            Input::North => {
                if self.letter_ui != LetterUi::Off {
                    self.clear_letter_ui();
                    return Some(Vec::new());
                }
                if self.behavior == Behavior::Mpod {
                    if matches!(self.screen, Screen::NowPlaying) {
                        self.open_menu();
                    } else {
                        self.back();
                    }
                } else {
                    match self.screen {
                        Screen::NowPlaying => self.open_menu(),
                        Screen::Search if self.search_editing => self.move_search_picker(-1),
                        Screen::Artists => return Some(self.move_artists(-1)),
                        Screen::AllTracks => return Some(self.move_songs(-1)),
                        _ => self.move_cursor(-1),
                    }
                }
            }
            Input::South => {
                if self.behavior == Behavior::Creative {
                    return Some(self.wheel(1));
                }
                return Some(self.south());
            }
            Input::West => {
                if self.letter_ui != LetterUi::Off {
                    self.clear_letter_ui();
                    return Some(Vec::new());
                }
                if self.behavior == Behavior::Mpod && !self.on_search() {
                    return Some(vec![UserCommand::Player(PlayerCommand::Prev)]);
                }
                if self.on_search() {
                    return Some(self.search_west());
                }
                match self.screen {
                    Screen::NowPlaying => {
                        return Some(vec![UserCommand::Player(PlayerCommand::Prev)]);
                    }
                    _ => self.back(),
                }
            }
            Input::East => {
                if self.letter_ui != LetterUi::Off {
                    return Some(Vec::new());
                }
                if self.behavior == Behavior::Mpod && !(self.on_search() && self.search_editing) {
                    return Some(vec![UserCommand::Player(PlayerCommand::Next)]);
                }
                if self.on_search() && self.search_editing {
                    if !self.search_results.is_empty() {
                        self.search_editing = false;
                        self.cursor = 0;
                    }
                    return Some(Vec::new());
                }
                return Some(self.activate(false));
            }
            Input::Center => {
                if matches!(self.letter_ui, LetterUi::Picker) {
                    return Some(self.confirm_letter_picker());
                }
                if self.on_search() && self.search_editing {
                    return Some(self.append_picker_char());
                }
                return Some(self.activate(true));
            }
        }
        Some(Vec::new())
    }

    fn wheel(&mut self, step: i32) -> Vec<UserCommand> {
        if matches!(self.screen, Screen::NowPlaying) {
            return match self.behavior {
                Behavior::PocketDap | Behavior::Mpod => {
                    self.arm_volume_overlay();
                    vec![UserCommand::System(if step < 0 {
                        SystemRequest::VolDown
                    } else {
                        SystemRequest::VolUp
                    })]
                }
                Behavior::Creative | Behavior::Playlist => Vec::new(),
            };
        }
        if matches!(self.letter_ui, LetterUi::Picker) {
            self.move_letter_picker(step);
            return Vec::new();
        }
        if matches!(self.letter_ui, LetterUi::Scrub { .. }) {
            self.move_letter_picker(step);
            self.letter_ui = LetterUi::Scrub { idle: 0 };
            return self.jump_to_letter(self.letter_char());
        }
        if self.search_editing && self.on_search() {
            self.move_search_picker(step);
            return Vec::new();
        }
        if self.on_letter_list() {
            self.wheel_burst = self.wheel_burst.saturating_add(1);
            self.wheel_burst_idle = 0;
            if self.wheel_burst >= SCRUB_ARM_STEPS {
                self.wheel_burst = 0;
                self.open_letter_scrub();
                self.move_letter_picker(step);
                self.letter_ui = LetterUi::Scrub { idle: 0 };
                return self.jump_to_letter(self.letter_char());
            }
        }
        if matches!(self.screen, Screen::Artists) {
            return self.move_artists(step);
        }
        if matches!(self.screen, Screen::AllTracks) {
            return self.move_songs(step);
        }
        self.move_cursor(step);
        Vec::new()
    }

    fn south(&mut self) -> Vec<UserCommand> {
        let play_everywhere = matches!(self.behavior, Behavior::PocketDap | Behavior::Playlist | Behavior::Mpod);
        if play_everywhere || matches!(self.screen, Screen::NowPlaying) {
            return vec![UserCommand::Player(PlayerCommand::TogglePlay)];
        }
        self.wheel(1)
    }

    fn open_menu(&mut self) {
        self.clear_letter_ui();
        self.screen = Screen::Menu;
        self.cursor = 0;
    }

    fn clear_letter_ui(&mut self) {
        self.letter_ui = LetterUi::Off;
        self.letter_pending = None;
        self.wheel_burst = 0;
        self.wheel_burst_idle = 0;
    }

    fn open_letter_picker(&mut self) {
        self.seed_letter_picker();
        self.letter_ui = LetterUi::Picker;
        self.letter_pending = None;
    }

    fn open_letter_scrub(&mut self) {
        self.seed_letter_picker();
        self.letter_ui = LetterUi::Scrub { idle: 0 };
    }

    fn seed_letter_picker(&mut self) {
        let alphabet = self.letter_alphabet();
        let hint = self
            .list_labels()
            .get(self.cursor)
            .and_then(|s| s.chars().find(|c| c.is_alphabetic()))
            .map(|c| c.to_ascii_lowercase())
            .unwrap_or('a');
        self.letter_picker = alphabet.iter().position(|&c| c == hint).unwrap_or(0);
    }

    fn move_letter_picker(&mut self, step: i32) {
        let len = self.letter_alphabet().len() as i32;
        if len == 0 {
            return;
        }
        let next = i32::try_from(self.letter_picker).unwrap_or(0) + step;
        self.letter_picker = next.rem_euclid(len) as usize;
    }

    fn confirm_letter_picker(&mut self) -> Vec<UserCommand> {
        let ch = self.letter_char();
        self.letter_ui = LetterUi::Off;
        self.jump_to_letter(ch)
    }

    fn title_starts_with_letter(title: &str, letter: char) -> bool {
        let want = letter.to_ascii_lowercase();
        title
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_ascii_lowercase() == want)
            .unwrap_or(false)
    }

    fn find_letter_index(titles: impl IntoIterator<Item = String>, letter: char) -> Option<usize> {
        titles.into_iter().position(|t| Self::title_starts_with_letter(&t, letter))
    }

    fn jump_to_letter(&mut self, letter: char) -> Vec<UserCommand> {
        match self.screen {
            Screen::AllAlbums => {
                if let Some(i) = Self::find_letter_index(self.albums.iter().map(MetadataLibraryItem::get_title), letter) {
                    self.cursor = i;
                }
                Vec::new()
            }
            Screen::Playlists => {
                if let Some(i) = Self::find_letter_index(self.playlists.iter().map(|(_, n)| n.clone()), letter) {
                    self.cursor = i;
                }
                Vec::new()
            }
            Screen::Artists => self.jump_artists_letter(letter),
            Screen::AllTracks => self.jump_songs_letter(letter),
            _ => Vec::new(),
        }
    }

    fn jump_artists_letter(&mut self, letter: char) -> Vec<UserCommand> {
        if let Some(i) = Self::find_letter_index(self.artists.iter().map(MetadataLibraryItem::get_title), letter) {
            self.cursor = i;
            self.letter_pending = None;
            return Vec::new();
        }
        self.letter_pending = Some(letter);
        self.artist_after_load = AfterLoad::First;
        self.query_artists_page(0, ArtistPageMode::Offset)
    }

    fn jump_songs_letter(&mut self, letter: char) -> Vec<UserCommand> {
        if let Some(i) = Self::find_letter_index(self.tracks.iter().map(MetadataLibraryItem::get_title), letter) {
            self.cursor = i;
            self.letter_pending = None;
            return Vec::new();
        }
        self.letter_pending = Some(letter);
        self.song_after_load = AfterLoad::First;
        self.query_songs_page(0)
    }

    fn finish_letter_pending_artists(&mut self) -> Vec<UserCommand> {
        let Some(letter) = self.letter_pending else {
            return Vec::new();
        };
        if let Some(i) = Self::find_letter_index(self.artists.iter().map(MetadataLibraryItem::get_title), letter) {
            self.cursor = i;
            self.letter_pending = None;
            return Vec::new();
        }
        let next = self.artist_offset + self.artists.len();
        if next >= self.artist_total || self.artists.is_empty() {
            self.letter_pending = None;
            return Vec::new();
        }
        self.artist_after_load = AfterLoad::First;
        self.query_artists_page(next, ArtistPageMode::Offset)
    }

    fn finish_letter_pending_songs(&mut self) -> Vec<UserCommand> {
        let Some(letter) = self.letter_pending else {
            return Vec::new();
        };
        if let Some(i) = Self::find_letter_index(self.tracks.iter().map(MetadataLibraryItem::get_title), letter) {
            self.cursor = i;
            self.letter_pending = None;
            return Vec::new();
        }
        let next = self.song_offset + self.tracks.len();
        if next >= self.song_total || self.tracks.is_empty() {
            self.letter_pending = None;
            return Vec::new();
        }
        self.song_after_load = AfterLoad::First;
        self.query_songs_page(next)
    }

    fn move_cursor(&mut self, step: i32) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let next = i32::try_from(self.cursor).unwrap_or(0) + step;
        self.cursor = next.rem_euclid(len as i32) as usize;
    }

    fn clamp_cursor(&mut self) {
        let len = self.list_len();
        if len == 0 {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(len - 1);
        }
    }

    fn back(&mut self) {
        self.clear_letter_ui();
        self.screen = match &self.screen {
            Screen::NowPlaying | Screen::Menu => Screen::NowPlaying,
            Screen::Search => Screen::Library,
            Screen::Library => Screen::Menu,
            Screen::Artists | Screen::AllAlbums | Screen::Playlists | Screen::AllTracks => Screen::Library,
            Screen::Albums { .. } if self.from_search => Screen::Search,
            Screen::Albums { .. } => Screen::Artists,
            Screen::Tracks {
                back: ListBack::Search, ..
            } => Screen::Search,
            Screen::Tracks {
                back: ListBack::AllAlbums, ..
            } => Screen::AllAlbums,
            Screen::Tracks {
                back: ListBack::Playlists, ..
            } => Screen::Playlists,
            Screen::Tracks {
                back: ListBack::AllTracks, ..
            } => Screen::AllTracks,
            Screen::Tracks {
                back: ListBack::Artist,
                artist,
                ..
            } => Screen::Albums { artist: artist.clone() },
            Screen::TrackActions {
                back: ListBack::AllTracks, ..
            } => Screen::AllTracks,
            Screen::TrackActions {
                back, artist, album_id, ..
            } => Screen::Tracks {
                artist: artist.clone(),
                album_id: album_id.clone(),
                back: *back,
            },
            Screen::AlbumActions { back, artist, .. } => match back {
                ListBack::Search => Screen::Search,
                ListBack::AllAlbums => Screen::AllAlbums,
                ListBack::Playlists => Screen::Playlists,
                ListBack::AllTracks => Screen::AllTracks,
                ListBack::Artist => Screen::Albums { artist: artist.clone() },
            },
            Screen::Queue => Screen::Menu,
            Screen::QueueActions { .. } => Screen::Queue,
        };
        if matches!(self.screen, Screen::Search) {
            self.search_editing = self.search_results.is_empty();
        }
        self.cursor = 0;
    }

    fn open_queue(&mut self) -> Vec<UserCommand> {
        self.screen = Screen::Queue;
        self.cursor = 0;
        vec![UserCommand::Queue(QueueCommand::QueryCurrentQueue(
            CurrentQueueQuery::CurrentSongPage,
        ))]
    }

    fn open_artists(&mut self) -> Vec<UserCommand> {
        self.screen = Screen::Artists;
        self.cursor = 0;
        self.from_search = false;
        self.artists.clear();
        self.artist_offset = 0;
        self.artist_total = 0;
        self.artist_after_load = AfterLoad::First;
        self.query_artists_page(0, ArtistPageMode::Offset)
    }

    fn open_library(&mut self) -> Vec<UserCommand> {
        self.screen = Screen::Library;
        self.cursor = 0;
        Vec::new()
    }

    fn open_all_albums(&mut self) -> Vec<UserCommand> {
        self.screen = Screen::AllAlbums;
        self.cursor = 0;
        self.albums.clear();
        vec![UserCommand::Metadata(MetadataCommand::QueryAlbums)]
    }

    fn open_playlists(&mut self) -> Vec<UserCommand> {
        self.screen = Screen::Playlists;
        self.cursor = 0;
        self.playlists.clear();
        vec![UserCommand::Playlist(PlaylistCommand::QuerySavedPlaylists)]
    }

    fn open_all_tracks(&mut self) -> Vec<UserCommand> {
        self.screen = Screen::AllTracks;
        self.cursor = 0;
        self.tracks.clear();
        self.song_offset = 0;
        self.song_total = 0;
        self.song_after_load = AfterLoad::First;
        self.query_songs_page(0)
    }

    fn query_songs_page(&self, offset: usize) -> Vec<UserCommand> {
        vec![UserCommand::Metadata(MetadataCommand::QuerySongsPage {
            offset,
            limit: ARTIST_PAGE,
        })]
    }

    fn move_songs(&mut self, step: i32) -> Vec<UserCommand> {
        let len = self.tracks.len();
        if len == 0 {
            return self.query_songs_page(0);
        }
        let next = i32::try_from(self.cursor).unwrap_or(0) + step;
        if next < 0 {
            if self.song_offset == 0 {
                return Vec::new();
            }
            let target = self.song_offset - 1;
            self.song_after_load = AfterLoad::Global(target);
            let start = target.saturating_sub(ARTIST_PAGE.saturating_sub(1));
            return self.query_songs_page(start);
        }
        if next >= len as i32 {
            let next_off = self.song_offset + len;
            if next_off >= self.song_total {
                return Vec::new();
            }
            self.song_after_load = AfterLoad::First;
            return self.query_songs_page(next_off);
        }
        self.cursor = next as usize;
        Vec::new()
    }

    fn query_artists_page(&self, offset: usize, mode: ArtistPageMode) -> Vec<UserCommand> {
        vec![UserCommand::Metadata(MetadataCommand::QueryArtistsPage {
            offset,
            limit: ARTIST_PAGE,
            mode,
        })]
    }

    fn move_artists(&mut self, step: i32) -> Vec<UserCommand> {
        let len = self.artists.len();
        if len == 0 {
            return self.query_artists_page(0, ArtistPageMode::Offset);
        }
        let next = i32::try_from(self.cursor).unwrap_or(0) + step;
        if next < 0 {
            if self.artist_offset == 0 {
                return Vec::new();
            }
            let target = self.artist_offset - 1;
            self.artist_after_load = AfterLoad::Global(target);
            let start = target.saturating_sub(ARTIST_PAGE.saturating_sub(1));
            return self.query_artists_page(start, ArtistPageMode::Offset);
        }
        if next >= len as i32 {
            let next_off = self.artist_offset + len;
            if next_off >= self.artist_total {
                return Vec::new();
            }
            self.artist_after_load = AfterLoad::First;
            return self.query_artists_page(next_off, ArtistPageMode::Offset);
        }
        self.cursor = next as usize;
        Vec::new()
    }

    pub fn artists_letter(&self) -> Option<char> {
        if !matches!(self.screen, Screen::Artists) {
            return None;
        }
        let name = self.artists.get(self.cursor)?.get_title();
        Some(artist_letter_hint(&name))
    }

    fn open_search(&mut self) -> Vec<UserCommand> {
        self.screen = Screen::Search;
        self.search_editing = true;
        self.from_search = true;
        self.cursor = 0;
        self.search_cmd()
    }

    fn search_cmd(&self) -> Vec<UserCommand> {
        if self.search_query.trim().is_empty() {
            return Vec::new();
        }
        vec![UserCommand::Metadata(MetadataCommand::SearchLibrary(
            self.search_query.clone(),
            SEARCH_LIMIT,
        ))]
    }

    fn type_search(&mut self, c: char) -> Vec<UserCommand> {
        if !self.on_search() {
            return Vec::new();
        }
        if !c.is_ascii_graphic() && c != ' ' {
            return Vec::new();
        }
        self.search_editing = true;
        self.search_query.push(c.to_ascii_lowercase());
        self.search_cmd()
    }

    fn delete_search_char(&mut self) -> Vec<UserCommand> {
        if !self.on_search() {
            return Vec::new();
        }
        self.search_editing = true;
        self.search_query.pop();
        if self.search_query.trim().is_empty() {
            self.search_results.clear();
            self.cursor = 0;
            return Vec::new();
        }
        self.search_cmd()
    }

    fn append_picker_char(&mut self) -> Vec<UserCommand> {
        let c = SEARCH_CHARS.get(self.search_picker).copied().unwrap_or(b'a') as char;
        self.type_search(c)
    }

    fn move_search_picker(&mut self, step: i32) {
        let len = SEARCH_CHARS.len() as i32;
        let next = i32::try_from(self.search_picker).unwrap_or(0) + step;
        self.search_picker = next.rem_euclid(len) as usize;
    }

    fn search_west(&mut self) -> Vec<UserCommand> {
        if self.search_editing {
            if self.search_query.is_empty() {
                self.back();
                Vec::new()
            } else {
                self.delete_search_char()
            }
        } else {
            self.search_editing = true;
            Vec::new()
        }
    }

    fn activate_search(&mut self, center: bool) -> Vec<UserCommand> {
        let Some(item) = self.search_results.get(self.cursor).cloned() else {
            return Vec::new();
        };
        self.from_search = true;
        match item {
            MetadataLibraryItem::Artist { name } => {
                self.screen = Screen::Albums { artist: name.clone() };
                self.cursor = 0;
                self.albums.clear();
                vec![UserCommand::Metadata(MetadataCommand::QueryAlbumsByArtist(name))]
            }
            MetadataLibraryItem::Album { mut id, name, artist, .. } => {
                if id.is_empty() {
                    id = name.clone();
                }
                self.note_album(&artist, &name);
                if center {
                    self.screen = Screen::Tracks {
                        artist: String::new(),
                        album_id: id.clone(),
                        back: ListBack::Search,
                    };
                    self.cursor = 0;
                    self.tracks.clear();
                    vec![UserCommand::Metadata(MetadataCommand::QuerySongsByAlbum(id))]
                } else {
                    self.screen = Screen::AlbumActions {
                        artist: String::new(),
                        album_id: id,
                        back: ListBack::Search,
                    };
                    self.cursor = 0;
                    Vec::new()
                }
            }
            MetadataLibraryItem::SongItem(song) => {
                let file = song.file;
                if center {
                    self.screen = Screen::NowPlaying;
                    song_cmds(file, 0)
                } else {
                    self.screen = Screen::TrackActions {
                        artist: String::new(),
                        album_id: String::new(),
                        file,
                        back: ListBack::Search,
                    };
                    self.cursor = 0;
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn activate(&mut self, center: bool) -> Vec<UserCommand> {
        match &self.screen.clone() {
            Screen::NowPlaying => {
                if center {
                    vec![UserCommand::Player(PlayerCommand::TogglePlay)]
                } else {
                    vec![UserCommand::Player(PlayerCommand::Next)]
                }
            }
            Screen::Menu => match self.cursor {
                0 => {
                    self.screen = Screen::NowPlaying;
                    Vec::new()
                }
                1 => self.open_library(),
                _ => self.open_queue(),
            },
            Screen::Search => self.activate_search(center),
            Screen::Library => match self.cursor {
                0 => self.open_artists(),
                1 => self.open_all_albums(),
                2 => self.open_playlists(),
                3 => self.open_all_tracks(),
                _ => self.open_search(),
            },
            Screen::Artists => {
                if let Some(item) = self.artists.get(self.cursor) {
                    let artist = item.get_id();
                    self.screen = Screen::Albums { artist: artist.clone() };
                    self.cursor = 0;
                    self.albums.clear();
                    vec![UserCommand::Metadata(MetadataCommand::QueryAlbumsByArtist(artist))]
                } else {
                    Vec::new()
                }
            }
            Screen::Albums { artist } => {
                if let Some(item) = self.albums.get(self.cursor) {
                    let album_id = item.get_id();
                    let (album_artist, album_name) = album_pair(item, artist);
                    self.note_album(&album_artist, &album_name);
                    if center {
                        self.screen = Screen::Tracks {
                            artist: artist.clone(),
                            album_id: album_id.clone(),
                            back: ListBack::Artist,
                        };
                        self.cursor = 0;
                        self.tracks.clear();
                        vec![UserCommand::Metadata(MetadataCommand::QuerySongsByAlbum(album_id))]
                    } else {
                        self.screen = Screen::AlbumActions {
                            artist: artist.clone(),
                            album_id,
                            back: ListBack::Artist,
                        };
                        self.cursor = 0;
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            Screen::Tracks { artist, album_id, back } => {
                if let Some(item) = self.tracks.get(self.cursor) {
                    let file = item_file(item);
                    let back = *back;
                    if center {
                        self.screen = Screen::NowPlaying;
                        song_cmds(file, 0)
                    } else {
                        self.screen = Screen::TrackActions {
                            artist: artist.clone(),
                            album_id: album_id.clone(),
                            file,
                            back,
                        };
                        self.cursor = 0;
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            Screen::AllAlbums => {
                if let Some(item) = self.albums.get(self.cursor) {
                    let album_id = item.get_id();
                    let (album_artist, album_name) = album_pair(item, "");
                    self.note_album(&album_artist, &album_name);
                    if center {
                        self.screen = Screen::Tracks {
                            artist: String::new(),
                            album_id: album_id.clone(),
                            back: ListBack::AllAlbums,
                        };
                        self.cursor = 0;
                        self.tracks.clear();
                        vec![UserCommand::Metadata(MetadataCommand::QuerySongsByAlbum(album_id))]
                    } else {
                        self.screen = Screen::AlbumActions {
                            artist: String::new(),
                            album_id,
                            back: ListBack::AllAlbums,
                        };
                        self.cursor = 0;
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            Screen::Playlists => {
                let Some((id, _)) = self.playlists.get(self.cursor).cloned() else {
                    return Vec::new();
                };
                self.shown_artist.clear();
                self.shown_album.clear();
                self.screen = Screen::Tracks {
                    artist: String::new(),
                    album_id: id.clone(),
                    back: ListBack::Playlists,
                };
                self.cursor = 0;
                self.tracks.clear();
                vec![UserCommand::Playlist(PlaylistCommand::QuerySavedPlaylistSongs(id))]
            }
            Screen::AllTracks => {
                if let Some(item) = self.tracks.get(self.cursor) {
                    let file = item_file(item);
                    if center {
                        self.screen = Screen::NowPlaying;
                        song_cmds(file, 0)
                    } else {
                        self.screen = Screen::TrackActions {
                            artist: String::new(),
                            album_id: String::new(),
                            file,
                            back: ListBack::AllTracks,
                        };
                        self.cursor = 0;
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            Screen::TrackActions {
                artist,
                album_id,
                file,
                back,
            } => {
                let file = file.clone();
                let artist = artist.clone();
                let album_id = album_id.clone();
                let back = *back;
                let cmds = song_cmds(file, self.cursor);
                if self.cursor == 0 {
                    self.screen = Screen::NowPlaying;
                } else if back == ListBack::AllTracks {
                    self.screen = Screen::AllTracks;
                    self.cursor = 0;
                } else {
                    self.screen = Screen::Tracks { artist, album_id, back };
                    self.cursor = 0;
                }
                cmds
            }
            Screen::AlbumActions { artist, album_id, back } => {
                let album_id = album_id.clone();
                let artist = artist.clone();
                let back = *back;
                let cmds = album_cmds(album_id, self.cursor);
                if self.cursor == 0 {
                    self.screen = Screen::NowPlaying;
                } else {
                    self.screen = match back {
                        ListBack::Search => Screen::Search,
                        ListBack::AllAlbums => Screen::AllAlbums,
                        ListBack::Playlists => Screen::Playlists,
                        ListBack::AllTracks => Screen::AllTracks,
                        ListBack::Artist => Screen::Albums { artist },
                    };
                    self.cursor = 0;
                }
                cmds
            }
            Screen::Queue => {
                if let Some(song) = self.queue.as_ref().and_then(|p| p.items.get(self.cursor)) {
                    let file = song.file.clone();
                    if center {
                        self.screen = Screen::NowPlaying;
                        vec![UserCommand::Player(PlayerCommand::PlayItem(file))]
                    } else {
                        self.screen = Screen::QueueActions { file };
                        self.cursor = 0;
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            Screen::QueueActions { file } => {
                let file = file.clone();
                let cmds = match self.cursor {
                    0 => vec![UserCommand::Player(PlayerCommand::PlayItem(file))],
                    1 => vec![
                        UserCommand::Queue(QueueCommand::MoveItemAfterCurrent(file)),
                        UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::CurrentSongPage)),
                    ],
                    _ => vec![
                        UserCommand::Queue(QueueCommand::RemoveItem(file)),
                        UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::CurrentSongPage)),
                    ],
                };
                if self.cursor == 0 {
                    self.screen = Screen::NowPlaying;
                } else {
                    self.screen = Screen::Queue;
                    self.cursor = 0;
                }
                cmds
            }
        }
    }
}

fn item_file(item: &MetadataLibraryItem) -> String {
    match item {
        MetadataLibraryItem::SongItem(song) => song.file.clone(),
        other => other.get_id(),
    }
}

fn song_cmds(file: String, action: usize) -> Vec<UserCommand> {
    match action {
        0 => vec![UserCommand::Queue(QueueCommand::AddSongAndPlay(file))],
        1 => vec![
            UserCommand::Queue(QueueCommand::AddSongAfterCurrent(file)),
            UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::CurrentSongPage)),
        ],
        _ => vec![
            UserCommand::Queue(QueueCommand::AddSongToQueue(file)),
            UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::CurrentSongPage)),
        ],
    }
}

fn album_cmds(album_id: String, action: usize) -> Vec<UserCommand> {
    match action {
        0 => vec![UserCommand::Queue(QueueCommand::AddAlbumAndPlay(album_id))],
        1 => vec![
            UserCommand::Queue(QueueCommand::AddAlbumAfterCurrent(album_id)),
            UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::CurrentSongPage)),
        ],
        _ => vec![
            UserCommand::Queue(QueueCommand::AddAlbumToQueue(album_id)),
            UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::CurrentSongPage)),
        ],
    }
}

fn artist_letter_hint(name: &str) -> char {
    match name.trim().chars().next() {
        Some(c) if c.is_alphabetic() => c.to_uppercase().next().unwrap_or('#'),
        _ => '#',
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toggle() -> Vec<UserCommand> {
        vec![UserCommand::Player(PlayerCommand::TogglePlay)]
    }

    #[test]
    fn play_pause_stays_on_every_screen() {
        for behavior in [Behavior::PocketDap, Behavior::Playlist, Behavior::Mpod] {
            let screens = [Screen::Menu, Screen::Queue, Screen::Artists, Screen::NowPlaying, Screen::Search];
            for screen in screens {
                let mut app = App::new();
                app.behavior = behavior;
                app.screen = screen.clone();
                app.search_editing = true;
                app.search_picker = 3;
                let cmds = app.handle(Input::South).expect("play/pause");
                assert_eq!(cmds, toggle());
                assert_eq!(app.screen, screen);
                assert_eq!(app.search_picker, 3);
            }
        }
    }

    #[test]
    fn mpod_menu_skips_and_wheel_volume() {
        let mut playing = App::new();
        playing.behavior = Behavior::Mpod;
        playing.screen = Screen::Artists;
        assert_eq!(playing.handle(Input::South).expect("play"), toggle());
        assert_eq!(playing.screen, Screen::Artists);

        let mut menu = App::new();
        menu.behavior = Behavior::Mpod;
        menu.screen = Screen::Menu;
        assert_eq!(
            menu.handle(Input::East).expect("next"),
            vec![UserCommand::Player(PlayerCommand::Next)]
        );
        assert_eq!(menu.screen, Screen::Menu);
        menu.handle(Input::North).expect("menu");
        assert_eq!(menu.screen, Screen::NowPlaying);

        let mut now = App::new();
        now.behavior = Behavior::Mpod;
        now.screen = Screen::NowPlaying;
        assert_eq!(
            now.handle(Input::StepRight).expect("vol"),
            vec![UserCommand::System(SystemRequest::VolUp)]
        );
    }

    #[test]
    fn creative_play_and_back_are_side_buttons() {
        let mut menu = App::new();
        menu.behavior = Behavior::Creative;
        menu.screen = Screen::Menu;
        assert_eq!(menu.handle(Input::Play).expect("play"), toggle());
        assert_eq!(menu.screen, Screen::Menu);
        let scrolled = menu.handle(Input::South).expect("scroll");
        assert!(scrolled.is_empty());
        assert_eq!(menu.cursor, 1);

        let mut artists = App::new();
        artists.behavior = Behavior::Creative;
        artists.screen = Screen::Artists;
        artists.handle(Input::Back).expect("back");
        assert_eq!(artists.screen, Screen::Library);
    }

    #[test]
    fn wheel_on_now_playing_follows_behavior() {
        let mut pocket = App::new();
        pocket.screen = Screen::NowPlaying;
        assert_eq!(
            pocket.handle(Input::StepRight).expect("vol"),
            vec![UserCommand::System(SystemRequest::VolUp)]
        );

        let mut mpod = App::new();
        mpod.behavior = Behavior::Mpod;
        mpod.screen = Screen::NowPlaying;
        assert_eq!(
            mpod.handle(Input::StepLeft).expect("vol"),
            vec![UserCommand::System(SystemRequest::VolDown)]
        );

        for behavior in [Behavior::Creative, Behavior::Playlist] {
            let mut app = App::new();
            app.behavior = behavior;
            app.screen = Screen::NowPlaying;
            assert!(app.handle(Input::StepRight).expect("idle").is_empty());
            assert_eq!(app.screen, Screen::NowPlaying);
        }
    }

    #[test]
    fn library_lists_albums_playlists_and_tracks() {
        let mut app = App::new();
        app.screen = Screen::Menu;
        app.cursor = 1;
        assert!(app.handle(Input::Center).expect("library").is_empty());
        assert_eq!(app.screen, Screen::Library);
        assert_eq!(
            app.list_labels(),
            vec![
                "Artists".to_string(),
                "Albums".to_string(),
                "Playlists".to_string(),
                "Tracks".to_string(),
                "Search".to_string(),
            ]
        );

        app.cursor = 0;
        assert!(matches!(
            app.handle(Input::Center).expect("artists").as_slice(),
            [UserCommand::Metadata(MetadataCommand::QueryArtistsPage { .. })]
        ));
        assert_eq!(app.screen, Screen::Artists);
        app.handle(Input::West).expect("back");
        assert_eq!(app.screen, Screen::Library);

        app.cursor = 1;
        assert_eq!(
            app.handle(Input::Center).expect("albums"),
            vec![UserCommand::Metadata(MetadataCommand::QueryAlbums)]
        );
        assert_eq!(app.screen, Screen::AllAlbums);
        app.handle(Input::West).expect("back");

        app.cursor = 2;
        assert_eq!(
            app.handle(Input::Center).expect("playlists"),
            vec![UserCommand::Playlist(PlaylistCommand::QuerySavedPlaylists)]
        );
        assert_eq!(app.screen, Screen::Playlists);
        app.handle(Input::West).expect("back");

        app.cursor = 3;
        assert!(matches!(
            app.handle(Input::Center).expect("tracks").as_slice(),
            [UserCommand::Metadata(MetadataCommand::QuerySongsPage { .. })]
        ));
        assert_eq!(app.screen, Screen::AllTracks);
        app.handle(Input::West).expect("back");
        assert_eq!(app.screen, Screen::Library);

        app.cursor = 4;
        assert!(app.handle(Input::Center).expect("search").is_empty());
        assert_eq!(app.screen, Screen::Search);
        app.handle(Input::West).expect("back");
        assert_eq!(app.screen, Screen::Library);
    }

    #[test]
    fn hold_menu_letter_picker_jumps_on_albums() {
        let mut app = App::new();
        app.screen = Screen::AllAlbums;
        app.albums = vec![
            MetadataLibraryItem::Album {
                name: "Abbey Road".into(),
                id: "1".into(),
                artist: "Beatles".into(),
                year: None,
            },
            MetadataLibraryItem::Album {
                name: "Revolver".into(),
                id: "2".into(),
                artist: "Beatles".into(),
                year: None,
            },
        ];
        app.handle(Input::MenuHold).expect("hold");
        assert!(app.letter_picking());
        assert_eq!(app.letter_overlay(), Some('a'));
        // Only a and r exist in the list — one step reaches r (not full A–Z).
        app.handle(Input::StepRight).expect("pick");
        assert_eq!(app.letter_overlay(), Some('r'));
        assert!(app.handle(Input::Center).expect("jump").is_empty());
        assert!(!app.letter_picking());
        assert_eq!(app.cursor, 1);
        assert_eq!(app.screen, Screen::AllAlbums);
    }

    #[test]
    fn letter_alphabet_only_lists_present_letters() {
        assert_eq!(
            App::letters_from_titles(["Abbey Road".into(), "Revolver".into(), "Rubber Soul".into()]),
            vec!['a', 'r']
        );
        assert!(App::letters_from_titles(["!!!".into(), "123".into()]).is_empty());
    }

    #[test]
    fn letter_picker_uses_present_letters_on_artists_and_tracks() {
        let mut app = App::new();
        app.screen = Screen::Artists;
        app.artists = vec![
            MetadataLibraryItem::Artist { name: "Abba".into() },
            MetadataLibraryItem::Artist { name: "Yes".into() },
        ];
        // More artists exist off-page — picker must still only offer a/y.
        app.artist_total = 200;
        app.handle(Input::MenuHold).expect("hold");
        assert_eq!(app.letter_overlay(), Some('a'));
        app.handle(Input::StepRight).expect("pick");
        assert_eq!(app.letter_overlay(), Some('y'));

        let mut app = App::new();
        app.screen = Screen::AllTracks;
        app.tracks = vec![
            MetadataLibraryItem::SongItem(Song {
                file: "a.flac".into(),
                title: Some("Air".into()),
                ..Song::default()
            }),
            MetadataLibraryItem::SongItem(Song {
                file: "z.flac".into(),
                title: Some("Zoo".into()),
                ..Song::default()
            }),
        ];
        app.song_total = 500;
        app.handle(Input::MenuHold).expect("hold");
        assert_eq!(app.letter_overlay(), Some('a'));
        app.handle(Input::StepRight).expect("pick");
        assert_eq!(app.letter_overlay(), Some('z'));
    }

    #[test]
    fn fast_wheel_scrubs_letter_on_playlists() {
        let mut app = App::new();
        app.screen = Screen::Playlists;
        app.playlists = vec![("1".into(), "Ambient".into()), ("2".into(), "Rock".into())];
        for _ in 0..SCRUB_ARM_STEPS {
            app.handle(Input::StepRight).expect("arm");
        }
        // 3 scrolls land on Rock; 4th arms scrub, steps a↔r to a, jumps to Ambient.
        assert_eq!(app.letter_overlay(), Some('a'));
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn wheel_scrolls_artists_without_volume() {
        for behavior in [Behavior::PocketDap, Behavior::Mpod, Behavior::Creative, Behavior::Playlist] {
            let mut app = App::new();
            app.behavior = behavior;
            app.screen = Screen::Artists;
            app.artists = vec![
                MetadataLibraryItem::Artist { name: "A".into() },
                MetadataLibraryItem::Artist { name: "B".into() },
            ];
            app.artist_total = 2;
            let cmds = app.handle(Input::StepRight).expect("scroll");
            assert!(cmds.is_empty());
            assert_eq!(app.cursor, 1);
            assert_eq!(app.screen, Screen::Artists);
        }
    }

    #[test]
    fn center_on_menu_selects() {
        let mut app = App::new();
        app.screen = Screen::Menu;
        let cmds = app.handle(Input::Center).expect("select");
        assert!(cmds.is_empty());
        assert_eq!(app.screen, Screen::NowPlaying);
    }

    #[test]
    fn creative_and_mpod_show_volume_overlay_only_while_adjusting() {
        for behavior in [Behavior::Creative, Behavior::Mpod] {
            let mut app = App::new();
            app.behavior = behavior;
            app.screen = Screen::NowPlaying;
            assert!(!app.volume_overlay_visible());
            app.handle(Input::VolUp).expect("vol");
            assert!(app.volume_overlay_visible());
            for _ in 0..60 {
                app.tick();
            }
            assert!(!app.volume_overlay_visible());
        }

        let mut mpod = App::new();
        mpod.behavior = Behavior::Mpod;
        mpod.screen = Screen::NowPlaying;
        mpod.handle(Input::StepRight).expect("wheel vol");
        assert!(mpod.volume_overlay_visible());

        let mut pocket = App::new();
        pocket.behavior = Behavior::PocketDap;
        pocket.screen = Screen::NowPlaying;
        pocket.handle(Input::VolUp).expect("vol");
        pocket.handle(Input::StepRight).expect("wheel vol");
        assert!(pocket.volume_overlay_visible());
    }

    #[test]
    fn status_names_the_artist_and_scrolls_the_album() {
        let mut app = App::new();
        app.screen = Screen::Artists;
        app.artists = vec![MetadataLibraryItem::Artist { name: "Beatles".into() }];
        assert_eq!(app.status_caption(), StatusCaption::Fixed("Beatles".into()));
        app.tick();
        assert_eq!(app.marquee_px(), 0);

        app.screen = Screen::Albums { artist: "Beatles".into() };
        app.albums = vec![MetadataLibraryItem::Album {
            name: "Abbey Road".into(),
            id: "1".into(),
            artist: String::new(),
            year: None,
        }];
        assert_eq!(app.status_caption(), StatusCaption::Scroll("Beatles - Abbey Road".into()));
        app.tick();
        app.tick();
        assert_eq!(app.marquee_px(), 1);
        app.screen = Screen::NowPlaying;
        assert_eq!(app.status_caption(), StatusCaption::Fixed("Now playing".into()));
        app.screen = Screen::Menu;
        assert_eq!(app.status_caption(), StatusCaption::Fixed("Menu".into()));
    }
}
