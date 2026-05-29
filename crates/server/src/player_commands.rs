use api_models::state::StateChangeEvent;

use crate::command_context::CommandContext;

pub fn handle_player_command(cmd: api_models::common::PlayerCommand, ctx: &CommandContext) {
    use api_models::common::PlayerCommand::{
        CyclePlaybackMode, Next, Pause, Play, PlayItem, Prev, QueryCurrentPlayerInfo, Seek, SeekBackward, SeekForward,
        Stop, TogglePlay,
    };

    match cmd {
        Play => {
            ctx.player_service.stop_current_song();
            ctx.player_service.play_from_current_queue_song();
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
        CyclePlaybackMode => {
            ctx.send_event(StateChangeEvent::PlaybackModeChangedEvent(
                ctx.queue_service.cycle_playback_mode(),
            ));
        }
        QueryCurrentPlayerInfo => {
            let mode = ctx.queue_service.get_playback_mode();
            ctx.send_event(StateChangeEvent::PlaybackModeChangedEvent(mode));
            let settings = ctx.config_store.get_settings();
            ctx.send_event(StateChangeEvent::VuMeterEnabledEvent(
                settings.rs_player_settings.vu_meter_enabled,
            ));
        }
    }
}
