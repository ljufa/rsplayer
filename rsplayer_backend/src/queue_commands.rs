use api_models::common::QueueCommand::{
    self, AddLocalLibDirectory, AddSongToQueue, ClearQueue, LoadAlbumInQueue, LoadArtistInQueue, LoadPlaylistInQueue,
    LoadSongToQueue, QueryCurrentQueue, QueryCurrentSong, RemoveItem,
};
use api_models::player::Song;
use api_models::state::StateChangeEvent;

use crate::command_context::CommandContext;

fn get_songs_from_album(ctx: &CommandContext, album_id: &str) -> Vec<Song> {
    ctx.album_repository
        .find_by_id(album_id)
        .map(|alb| {
            alb.song_keys
                .iter()
                .filter_map(|sk| ctx.song_repository.find_by_id(sk))
                .collect()
        })
        .unwrap_or_default()
}

fn get_songs_from_artist(ctx: &CommandContext, artist: &str) -> Vec<Song> {
    ctx.album_repository
        .find_by_artist(artist)
        .iter()
        .flat_map(|alb| alb.song_keys.iter())
        .filter_map(|sk| ctx.song_repository.find_by_id(sk))
        .collect()
}

fn get_songs_from_genre(ctx: &CommandContext, genre: &str) -> Vec<Song> {
    ctx.album_repository
        .find_all_by_genre(20)
        .into_iter()
        .find(|(g, _)| g == genre)
        .map(|(_, albums)| albums)
        .unwrap_or_default()
        .iter()
        .filter_map(|alb| ctx.album_repository.find_by_id(&alb.title))
        .flat_map(|alb| alb.song_keys.into_iter())
        .filter_map(|sk| ctx.song_repository.find_by_id(&sk))
        .collect()
}

fn get_songs_from_decade(ctx: &CommandContext, decade: &str) -> Vec<Song> {
    ctx.album_repository
        .find_all_by_decade(20)
        .into_iter()
        .find(|(d, _)| d == decade)
        .map(|(_, albums)| albums)
        .unwrap_or_default()
        .iter()
        .filter_map(|alb| ctx.album_repository.find_by_id(&alb.title))
        .flat_map(|alb| alb.song_keys.into_iter())
        .filter_map(|sk| ctx.song_repository.find_by_id(&sk))
        .collect()
}

fn load_songs_and_play(ctx: &CommandContext, songs: Vec<Song>, notification: &str) {
    let count = songs.len();
    ctx.player_service.stop_current_song();
    ctx.queue_service.replace_all(songs.into_iter());
    ctx.player_service.play_from_beginning();
    ctx.send_notification(&format!("{count} {notification}"));
}

fn add_songs_to_queue(ctx: &CommandContext, songs: &[Song], notification: &str) {
    let count = songs.len();
    for song in songs {
        ctx.queue_service.add_song(song);
    }
    ctx.send_notification(&format!("{count} {notification}"));
}

#[allow(clippy::too_many_lines)]
pub fn handle_queue_command(cmd: QueueCommand, ctx: &CommandContext) {
    match cmd {
        AddSongToQueue(song_id) => {
            ctx.queue_service.add_song_by_id(&song_id);
            ctx.send_notification("1 song added to queue");
        }
        ClearQueue => {
            ctx.player_service.stop_current_song();
            ctx.queue_service.clear();
        }
        RemoveItem(song_id) => {
            ctx.queue_service.remove_song(&song_id);
        }
        QueueCommand::AddSongAfterCurrent(song_id) => {
            ctx.queue_service.add_song_after_current(&song_id);
            ctx.send_notification("Song added after current");
        }
        QueueCommand::AddSongAndPlay(song_id) => {
            ctx.player_service.stop_current_song();
            ctx.queue_service.add_song_by_id(&song_id);
            ctx.queue_service.set_current_to_last();
            ctx.player_service.play_from_beginning();
            ctx.send_notification("Song added and playing");
        }
        QueueCommand::AddDirectoryAfterCurrent(dir) => {
            ctx.queue_service.add_songs_from_dir_after_current(&dir);
            ctx.send_notification("Directory added after current");
        }
        QueueCommand::AddDirectoryAndPlay(dir) => {
            if let Some(first_key) = ctx.queue_service.add_songs_from_dir_after_current(&dir) {
                ctx.player_service.stop_current_song();
                ctx.queue_service.set_current_song(&first_key);
                ctx.player_service.play_from_beginning();
                ctx.send_notification("Directory added and playing");
            }
        }
        QueueCommand::AddArtistAfterCurrent(artist) => {
            let songs = get_songs_from_artist(ctx, &artist);
            ctx.queue_service.add_songs_after_current(songs);
            ctx.send_notification("Artist added after current");
        }
        QueueCommand::AddArtistAndPlay(artist) => {
            let songs = get_songs_from_artist(ctx, &artist);
            if let Some(first_key) = ctx.queue_service.add_songs_after_current(songs) {
                ctx.player_service.stop_current_song();
                ctx.queue_service.set_current_song(&first_key);
                ctx.player_service.play_from_beginning();
                ctx.send_notification("Artist added and playing");
            }
        }
        QueueCommand::AddAlbumAfterCurrent(album_id) => {
            let songs = get_songs_from_album(ctx, &album_id);
            ctx.queue_service.add_songs_after_current(songs);
            ctx.send_notification("Album added after current");
        }
        QueueCommand::AddAlbumAndPlay(album_id) => {
            let songs = get_songs_from_album(ctx, &album_id);
            if let Some(first_key) = ctx.queue_service.add_songs_after_current(songs) {
                ctx.player_service.stop_current_song();
                ctx.queue_service.set_current_song(&first_key);
                ctx.player_service.play_from_beginning();
                ctx.send_notification("Album added and playing");
            }
        }
        QueueCommand::LoadGenreInQueue(genre) => {
            let songs = get_songs_from_genre(ctx, &genre);
            load_songs_and_play(ctx, songs, &format!("songs from genre '{genre}' loaded into queue"));
        }
        QueueCommand::AddGenreToQueue(genre) => {
            let songs = get_songs_from_genre(ctx, &genre);
            add_songs_to_queue(ctx, &songs, &format!("songs from genre '{genre}' added to queue"));
        }
        QueueCommand::LoadDecadeInQueue(decade) => {
            let songs = get_songs_from_decade(ctx, &decade);
            load_songs_and_play(ctx, songs, &format!("songs from '{decade}' loaded into queue"));
        }
        QueueCommand::AddDecadeToQueue(decade) => {
            let songs = get_songs_from_decade(ctx, &decade);
            add_songs_to_queue(ctx, &songs, &format!("songs from '{decade}' added to queue"));
        }
        QueueCommand::MoveItem(from, to) => {
            ctx.queue_service.move_item(from, to);
        }
        QueueCommand::MoveItemAfterCurrent(from) => {
            ctx.queue_service.move_item_after_current(from);
            ctx.send_notification("Song moved after current");
        }
        LoadPlaylistInQueue(pl_id) => {
            ctx.player_service.stop_current_song();
            let pl_songs = ctx.playlist_service.get_playlist_page_by_name(&pl_id, 0, 20000).items;
            ctx.queue_service.replace_all(pl_songs.into_iter());
            ctx.player_service.play_from_beginning();
            ctx.send_notification("Playlist loaded into queue");
        }
        QueueCommand::AddPlaylistToQueue(pl_id) => {
            let pl_songs = ctx.playlist_service.get_playlist_page_by_name(&pl_id, 0, 20000).items;
            add_songs_to_queue(ctx, &pl_songs, "songs added to queue");
            ctx.send_notification("Playlist added to queue");
        }
        LoadAlbumInQueue(album_id) => {
            if let Some(album) = ctx.album_repository.find_by_id(&album_id) {
                ctx.player_service.stop_current_song();
                let songs = album
                    .song_keys
                    .iter()
                    .filter_map(|sk| ctx.song_repository.find_by_id(sk));
                ctx.queue_service.replace_all(songs);
                ctx.player_service.play_from_beginning();
                ctx.send_notification("Album loaded into queue");
            }
        }
        LoadArtistInQueue(name) => {
            ctx.player_service.stop_current_song();
            ctx.queue_service.clear();
            ctx.album_repository
                .find_by_artist(&name)
                .iter()
                .flat_map(|alb| &alb.song_keys)
                .for_each(|sk| {
                    ctx.queue_service.add_song_by_id(sk);
                });
            ctx.player_service.play_from_beginning();
            ctx.send_notification("All artist's albums loaded into queue");
        }
        QueueCommand::AddAlbumToQueue(album_id) => {
            if let Some(album) = ctx.album_repository.find_by_id(&album_id) {
                album.song_keys.iter().for_each(|sk| {
                    if let Some(song) = ctx.song_repository.find_by_id(sk) {
                        ctx.queue_service.add_song(&song);
                    }
                });
                ctx.send_notification("Album added to queue");
            }
        }
        QueueCommand::AddArtistToQueue(name) => {
            ctx.album_repository.find_by_artist(&name).iter().for_each(|alb| {
                alb.song_keys.iter().for_each(|sk| {
                    if let Some(song) = ctx.song_repository.find_by_id(sk) {
                        ctx.queue_service.add_song(&song);
                    }
                });
            });
            ctx.send_notification("All artist's albums added to queue");
        }
        LoadSongToQueue(song_id) => {
            ctx.player_service.stop_current_song();
            ctx.queue_service.clear();
            ctx.queue_service.add_song_by_id(&song_id);
            ctx.player_service.play_from_beginning();
            ctx.send_notification("Queue replaced with one song");
        }
        QueryCurrentSong => {
            if let Some(song) = ctx.queue_service.get_current_song() {
                ctx.send_event(StateChangeEvent::CurrentSongEvent(song));
            }
        }
        QueryCurrentQueue(query) => {
            let queue = ctx.queue_service.query_current_queue(query);
            ctx.send_event(StateChangeEvent::CurrentQueueEvent(queue));
        }
        AddLocalLibDirectory(dir) => {
            ctx.queue_service.add_songs_from_dir(&dir);
            ctx.send_notification(&format!("Dir {dir} added to queue"));
        }
        QueueCommand::LoadLocalLibDirectory(dir) => {
            ctx.player_service.stop_current_song();
            ctx.queue_service.load_songs_from_dir(&dir);
            ctx.send_notification(&format!("Dir {dir} loaded to queue"));
            ctx.player_service.play_from_beginning();
        }
    }
}
