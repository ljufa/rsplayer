//! Transport commands (play/pause/seek/next…) → `PlayerService`. Rejected
//! with a notification while this instance is a grouped multiroom follower.

use api_models::state::StateChangeEvent;

use crate::command_context::CommandContext;

pub fn handle_player_command(cmd: api_models::common::PlayerCommand, ctx: &CommandContext) {
    use api_models::common::PlayerCommand::{
        CyclePlaybackMode, Next, Pause, Play, PlayItem, PlayRadioStation, Prev, QueryCurrentPlayerInfo, QueryPlaybackSource, ReturnToQueue,
        Seek, SeekBackward, SeekForward, Stop, TogglePlay,
    };
    use api_models::playback_source::PlaybackSource;

    // A grouped multiroom follower plays what the leader streams; local
    // transport commands would fight over the audio device.
    let is_transport = !matches!(cmd, QueryCurrentPlayerInfo | QueryPlaybackSource | CyclePlaybackMode);
    if is_transport && ctx.multiroom_follower_active.load(std::sync::atomic::Ordering::SeqCst) {
        ctx.send_error("Playback is controlled by the multiroom group leader. Leave the group to control it locally.");
        return;
    }

    match cmd {
        Play => {
            ctx.player_service.stop_current_song();
            ctx.player_service.play_current();
        }
        PlayItem(id) => {
            ctx.player_service.play_song(&id);
        }
        Pause | Stop => {
            ctx.player_service.stop_current_song();
        }
        TogglePlay => {
            ctx.player_service.toggle_play_pause();
        }
        Next => {
            ctx.player_service.play_next_song();
        }
        Prev => {
            ctx.player_service.play_prev_song();
        }
        Seek(sec) => {
            ctx.player_service.seek_current_song(sec);
        }
        SeekForward => {
            ctx.player_service.seek_relative(10);
        }
        SeekBackward => {
            ctx.player_service.seek_relative(-10);
        }
        PlayRadioStation(station) => match station.validated() {
            Ok(station) => {
                ctx.send_notification(&format!("Playing {}", station.name));
                ctx.player_service.play_source(PlaybackSource::Radio(station));
            }
            Err(e) => ctx.send_error(&e),
        },
        ReturnToQueue => {
            ctx.player_service.return_to_queue();
        }
        QueryPlaybackSource => {
            ctx.send_event(StateChangeEvent::PlaybackSourceEvent(ctx.player_service.playback_source()));
        }
        CyclePlaybackMode => {
            ctx.send_event(StateChangeEvent::PlaybackModeChangedEvent(ctx.queue_service.cycle_playback_mode()));
        }
        QueryCurrentPlayerInfo => {
            let mode = ctx.queue_service.get_playback_mode();
            ctx.send_event(StateChangeEvent::PlaybackModeChangedEvent(mode));
            let settings = ctx.config_store.get_settings();
            ctx.send_event(StateChangeEvent::VuMeterEnabledEvent(settings.rs_player_settings.vu_meter_enabled));
            if let Some(info) = ctx.player_service.get_current_player_info() {
                ctx.send_event(StateChangeEvent::PlayerInfoEvent(info));
            }
            // A follower's state comes from the group leader, and browser
            // playback's from the <audio> element, not the playback thread.
            let follower = ctx.multiroom_follower_active.load(std::sync::atomic::Ordering::SeqCst);
            if !follower && !settings.local_browser_playback {
                ctx.send_event(StateChangeEvent::PlaybackStateEvent(ctx.player_service.get_player_state()));
            }
        }
    }
}
