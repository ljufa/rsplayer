//! Leader-side audio distribution.
//!
//! One ingestion task converts [`TeeEvent`]s from the playback thread into
//! timestamped [`AudioChunk`]s on a broadcast channel; each connected
//! follower has a writer task that turns them into `StreamStart`/uni-stream
//! chunks/`StreamStop` on its connection. Session metadata rides along with
//! every chunk, so a follower that connects (or lags) mid-track picks the
//! stream up at the next chunk.

use std::sync::Arc;

use iroh::endpoint::Connection;
use log::{debug, info, warn};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use api_models::state::StateChangeEvent;
use playback::rsp::tee::TeeEvent;

use crate::clock::MonoClock;
use crate::protocol::{AudioChunk, ClockMsg, ControlToFollower, SongMeta, StreamSpec, write_frame};

/// Everything a follower needs to start playing a session mid-stream.
pub struct SessionInfo {
    pub session_id: u64,
    pub spec: StreamSpec,
    pub gain_db_hundredths: Option<i32>,
    pub song: SongMeta,
}

#[derive(Clone)]
pub enum AudioMsg {
    Chunk {
        session: Arc<SessionInfo>,
        chunk: Arc<AudioChunk>,
    },
    SessionEnd {
        session_id: u64,
    },
    TimelineCorrection {
        session_id: u64,
        offset_micros: i64,
    },
}

/// Capacity of the audio broadcast: ~512 packets is several seconds of
/// typical decoder output — a follower further behind than that is dropped
/// into a gap rather than blocking everyone.
pub const AUDIO_BROADCAST_CAPACITY: usize = 512;

/// Consumes the playback tee and republishes timestamped chunks.
pub fn spawn_tee_ingestion(
    mut tee_rx: mpsc::Receiver<TeeEvent>,
    mut state_events: broadcast::Receiver<StateChangeEvent>,
    audio_tx: broadcast::Sender<AudioMsg>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut current_song = SongMeta::default();
        let mut current: Option<Arc<SessionInfo>> = None;
        let mut epoch_micros: u64 = 0;
        let mut seq: u64 = 0;
        loop {
            tokio::select! {
                event = tee_rx.recv() => {
                    let Some(event) = event else {
                        info!("Playback tee closed, stopping ingestion.");
                        break;
                    };
                    match event {
                        TeeEvent::SessionStart { session_id, spec, epoch_micros: epoch, gain_db_hundredths } => {
                            debug!("Audio session {session_id} started: {}Hz {}ch", spec.rate, spec.channels);
                            current = Some(Arc::new(SessionInfo {
                                session_id,
                                spec: StreamSpec { rate: spec.rate, channels: spec.channels },
                                gain_db_hundredths,
                                song: current_song.clone(),
                            }));
                            epoch_micros = epoch;
                            seq = 0;
                        }
                        TeeEvent::Chunk { session_id, first_frame, samples } => {
                            let Some(session) = current.as_ref().filter(|s| s.session_id == session_id) else {
                                continue;
                            };
                            let play_at_micros = epoch_micros + first_frame * 1_000_000 / u64::from(session.spec.rate);
                            let frames = samples.len() / usize::from(session.spec.channels);
                            let payload = samples.iter().flat_map(|s| crate::protocol::encode_sample(*s)).collect();
                            let chunk = Arc::new(AudioChunk {
                                session_id,
                                seq,
                                play_at_micros,
                                frames: u32::try_from(frames).unwrap_or(u32::MAX),
                                payload,
                            });
                            seq += 1;
                            let _ = audio_tx.send(AudioMsg::Chunk { session: session.clone(), chunk });
                        }
                        TeeEvent::SessionEnd { session_id } => {
                            debug!("Audio session {session_id} ended");
                            if current.as_ref().is_some_and(|s| s.session_id == session_id) {
                                current = None;
                            }
                            let _ = audio_tx.send(AudioMsg::SessionEnd { session_id });
                        }
                        TeeEvent::TimelineCorrection { session_id, offset_micros } => {
                            debug!("Timeline correction for session {session_id}: {offset_micros}µs");
                            let _ = audio_tx.send(AudioMsg::TimelineCorrection { session_id, offset_micros });
                        }
                    }
                }
                event = state_events.recv() => {
                    match event {
                        Ok(StateChangeEvent::CurrentSongEvent(song)) => {
                            current_song = SongMeta {
                                title: song.title.unwrap_or_default(),
                                artist: song.artist.unwrap_or_default(),
                                album: song.album.unwrap_or_default(),
                            };
                        }
                        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    })
}

/// Streams audio to one follower until its connection dies.
pub async fn run_follower_audio_writer(
    conn: Connection,
    mut audio_rx: broadcast::Receiver<AudioMsg>,
    control_tx: mpsc::Sender<ControlToFollower>,
) {
    let mut current: Option<(u64, iroh::endpoint::SendStream)> = None;
    loop {
        match audio_rx.recv().await {
            Ok(AudioMsg::Chunk { session, chunk }) => {
                if current.as_ref().map(|(id, _)| *id) != Some(session.session_id) {
                    if let Some((old_id, mut stream)) = current.take() {
                        let _ = stream.finish();
                        let _ = control_tx.send(ControlToFollower::StreamStop { session_id: old_id }).await;
                    }
                    if control_tx
                        .send(ControlToFollower::StreamStart {
                            session_id: session.session_id,
                            spec: session.spec,
                            song: session.song.clone(),
                            gain_db_hundredths: session.gain_db_hundredths,
                        })
                        .await
                        .is_err()
                    {
                        break; // control writer gone → connection is down
                    }
                    match conn.open_uni().await {
                        Ok(stream) => current = Some((session.session_id, stream)),
                        Err(e) => {
                            debug!("Failed to open audio stream to follower: {e}");
                            break;
                        }
                    }
                }
                if let Some((_, stream)) = current.as_mut()
                    && let Err(e) = write_frame(stream, &*chunk).await
                {
                    debug!("Audio write to follower failed: {e:#}");
                    break;
                }
            }
            Ok(AudioMsg::SessionEnd { session_id }) => {
                if current.as_ref().is_some_and(|(id, _)| *id == session_id) {
                    let (_, mut stream) = current.take().expect("checked above");
                    let _ = stream.finish();
                    let _ = control_tx.send(ControlToFollower::StreamStop { session_id }).await;
                }
            }
            Ok(AudioMsg::TimelineCorrection { session_id, offset_micros }) => {
                if current.as_ref().is_some_and(|(id, _)| *id == session_id)
                    && control_tx
                        .send(ControlToFollower::TimelineCorrection { session_id, offset_micros })
                        .await
                        .is_err()
                {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Follower audio writer lagged by {n} messages, followers will hear a gap");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Answers the follower's clock probes on this connection's datagrams.
pub async fn run_clock_responder(conn: Connection) {
    while let Ok(dgram) = conn.read_datagram().await {
        let t2 = MonoClock::now_micros();
        if let Ok(ClockMsg::Ping { id, t1 }) = postcard::from_bytes::<ClockMsg>(&dgram) {
            let t3 = MonoClock::now_micros();
            if let Ok(bytes) = postcard::to_stdvec(&ClockMsg::Pong { id, t1, t2, t3 }) {
                let _ = conn.send_datagram(bytes.into());
            }
        }
    }
}
