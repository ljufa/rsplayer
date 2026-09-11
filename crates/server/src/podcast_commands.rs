//! Podcast commands: subscriptions, episode queries, and queueing episodes.
//!
//! Anything that touches the network (`Search`, `Subscribe`, `Refresh`) is
//! handed to the podcast worker thread and answered later via events; the
//! rest is served synchronously from the local store.

use api_models::podcast::PodcastCommand;
use api_models::state::StateChangeEvent;
use podcast::PodcastJob;

use crate::command_context::CommandContext;

pub fn handle_podcast_command(cmd: PodcastCommand, ctx: &CommandContext) {
    let svc = &ctx.podcast_service;
    match cmd {
        PodcastCommand::Search(term) => svc.enqueue(PodcastJob::Search(term)),
        PodcastCommand::Subscribe { feed_url } => svc.enqueue(PodcastJob::Subscribe(feed_url)),
        PodcastCommand::Refresh(podcast_id) => svc.enqueue(PodcastJob::Refresh(podcast_id)),
        PodcastCommand::Unsubscribe(podcast_id) => match svc.unsubscribe(&podcast_id) {
            Ok(Some(podcast)) => ctx.send_notification(&format!("Unsubscribed from {}", podcast.title)),
            Ok(None) => ctx.send_notification("Podcast removed"),
            Err(e) => ctx.send_error(&format!("Unsubscribe failed: {e}")),
        },
        PodcastCommand::QueryPodcasts => ctx.send_event(StateChangeEvent::PodcastsEvent(svc.list_podcasts())),
        PodcastCommand::QueryEpisodes { podcast_id, offset, limit } => {
            ctx.send_event(StateChangeEvent::PodcastEpisodesEvent(svc.episodes_page(&podcast_id, offset, limit)));
        }
        PodcastCommand::SetPlayed(episode_id, played) => {
            if let Err(e) = svc.set_played(&episode_id, played) {
                ctx.send_error(&format!("Failed to update episode: {e}"));
            }
        }
        PodcastCommand::PlayEpisode(episode_id) => {
            let Some((episode, podcast)) = svc.episode_with_podcast(&episode_id) else {
                ctx.send_error("Episode not found");
                return;
            };
            ctx.player_service.stop_current_song();
            ctx.queue_service.add_song(&episode.to_song(&podcast));
            ctx.queue_service.set_current_to_last();
            // The player asks the podcast service for a resume position
            // before it starts the track, so a started episode continues.
            ctx.player_service.play_from_beginning();
            ctx.send_notification(&format!("Playing {}", episode.title));
        }
        PodcastCommand::AddEpisodeToQueue(episode_id) => {
            let Some((episode, podcast)) = svc.episode_with_podcast(&episode_id) else {
                ctx.send_error("Episode not found");
                return;
            };
            ctx.queue_service.add_song(&episode.to_song(&podcast));
            ctx.send_notification(&format!("Added {} to queue", episode.title));
        }
        PodcastCommand::AddEpisodeAfterCurrent(episode_id) => {
            let Some((episode, podcast)) = svc.episode_with_podcast(&episode_id) else {
                ctx.send_error("Episode not found");
                return;
            };
            ctx.queue_service.add_songs_after_current([episode.to_song(&podcast)]);
            ctx.send_notification(&format!("{} will play next", episode.title));
        }
    }
}
