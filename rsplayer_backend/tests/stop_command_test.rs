use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::{broadcast, mpsc};

use api_models::common::{PlayerCommand, UserCommand};
use api_models::player::Song;
use rsplayer::command_handler::handle_user_commands;
use rsplayer_config::Configuration;
use rsplayer_metadata::{
    album_repository::AlbumRepository,
    metadata_service::MetadataService,
    play_statistic_repository::PlayStatisticsRepository,
    queue_service::QueueService,
    song_repository::SongRepository,
};
use rsplayer_playback::rsp::player_service::PlayerService;

#[tokio::test]
async fn test_stop_command_resets_progress() {
    // 1. Setup
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path();

    // Create a temporary configuration for the test
    let config = Arc::new(Configuration::new());
    let mut settings = config.get_settings();
    settings.metadata_settings.db_path = db_path.join("metadata.db").to_str().unwrap().to_string();
    settings.playlist_settings.db_path = db_path.join("playlist.db").to_str().unwrap().to_string();
    settings.playback_queue_settings.db_path = db_path.join("queue.db").to_str().unwrap().to_string();

    // Set the player state db path for the test
    let player_state_db_path = db_path.join("player_state");
    settings.rs_player_settings.db_path = player_state_db_path.to_str().unwrap().to_string();

    config.save_settings(&settings);

    // Manually create and populate the player state database *before* the service starts
    {
        let player_state_db = sled::open(&player_state_db_path).unwrap();
        player_state_db.insert("last_played_song_progress", "30".as_bytes()).unwrap();
        player_state_db.flush().unwrap();
    } // The db is dropped and the lock is released here.

    let album_repository = Arc::new(AlbumRepository::default());
    let song_repository = Arc::new(SongRepository::default());
    let statistics_repository = Arc::new(PlayStatisticsRepository::default());
    let metadata_service = Arc::new(
        MetadataService::new(
            &config.get_settings().metadata_settings,
            song_repository.clone(),
            album_repository.clone(),
            statistics_repository.clone(),
        )
        .unwrap(),
    );
    let playlist_service = Arc::new(rsplayer_metadata::playlist_service::PlaylistService::new(&config.get_settings().playlist_settings));
    let queue_service = Arc::new(QueueService::new(
        &config.get_settings().playback_queue_settings,
        song_repository.clone(),
        statistics_repository.clone(),
    ));

    let (state_changes_tx, _) = broadcast::channel(20);

    let player_service = Arc::new(PlayerService::new(
        &config.get_settings(),
        metadata_service.clone(),
        queue_service.clone(),
        state_changes_tx.clone(),
    ));

    // Add a song to the queue
    queue_service.add_song(&Song {
        file: "test.mp3".to_string(),
        ..Default::default()
    });

    let (user_commands_tx, user_commands_rx) = mpsc::channel(5);

    // Spawn the command handler and get its handle
    let handler_task = tokio::spawn(handle_user_commands(
        player_service.clone(),
        metadata_service.clone(),
        playlist_service.clone(),
        queue_service.clone(),
        album_repository.clone(),
        song_repository.clone(),
        config.clone(),
        user_commands_rx,
        state_changes_tx.clone(),
    ));

    // 2. Act: Send the Stop command
    user_commands_tx.send(UserCommand::Player(PlayerCommand::Stop)).await.unwrap();

    // Give some time for the command to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Gracefully shut down the services to release the database lock
    drop(user_commands_tx); // Close the channel, which will stop the command handler
    handler_task.await.unwrap(); // Wait for the command handler to finish
    player_service.shutdown(); // Stop the player service's background tasks
    drop(player_service); // Drop the service to release the sled db lock

    // Give a moment for the OS to release the file lock
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 3. Assert: Check the database directly
    let player_state_db = sled::open(&player_state_db_path).unwrap();
    let progress = player_state_db.get("last_played_song_progress").unwrap().unwrap();
    assert_eq!(progress.as_ref(), "0".as_bytes(), "Playback progress should be reset to 0 in the database");
}