//! Follower-side audio reception and scheduled playback.
//!
//! After the group handshake, [`run_grouped_follower`] owns the connection:
//! it drives the control stream, keeps the clock offset fresh via datagram
//! probes, accepts one uni stream per audio session and feeds a
//! [`SyncSink`] with chunks scheduled on the local monotonic clock.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use log::{debug, info, warn};
use tokio::sync::{broadcast, mpsc};

use api_models::player::Song;
use api_models::settings::RsPlayerSettings;
use api_models::state::{PlayerState, SongProgress, StateChangeEvent};
use dsp::DspHandle;
use playback::rsp::sync_sink::{ScheduledChunk, SyncSink, SyncSinkConfig};

use crate::clock::{ClockState, ExchangeSample, MonoClock, OffsetEstimator};
use crate::protocol::{AudioChunk, ClockMsg, ControlToFollower, ControlToLeader, MAX_AUDIO_FRAME_BYTES, MAX_CONTROL_FRAME_BYTES, read_frame, write_frame};

/// Everything needed to open the local audio output for received streams.
#[derive(Clone)]
pub struct SinkParams {
    pub audio_device: String,
    pub rsp_settings: RsPlayerSettings,
    pub software_gain: Option<Arc<AtomicU8>>,
    pub vu_meter_enabled: bool,
    pub dsp_handle: Option<DspHandle>,
    pub latency_offset_ms: i32,
    pub changes_tx: broadcast::Sender<StateChangeEvent>,
}

#[derive(Clone)]
struct PendingSession {
    spec: crate::protocol::StreamSpec,
    gain_db_hundredths: Option<i32>,
    /// Live timeline correction from the leader (`actual − nominal`, µs),
    /// added to every chunk timestamp; the sink slews to it gradually.
    correction_micros: Arc<AtomicI64>,
}

#[derive(Default)]
struct Sessions {
    pending: Mutex<HashMap<u64, PendingSession>>,
    sinks: Mutex<HashMap<u64, SyncSink>>,
}

/// Drives a grouped follower connection until the leader disconnects or the
/// control stream errors. Returns when the group membership ended.
pub(crate) async fn run_grouped_follower(
    conn: Connection,
    mut send: SendStream,
    mut recv: RecvStream,
    params: SinkParams,
    events_tx: mpsc::Sender<crate::Event>,
) {
    let clock = Arc::new(ClockState::default());
    let sessions = Arc::new(Sessions::default());

    let clock_task = tokio::spawn(run_clock(conn.clone(), clock.clone()));
    let audio_task = tokio::spawn(accept_audio_streams(conn.clone(), sessions.clone(), clock.clone(), params.clone()));

    loop {
        match read_frame::<ControlToFollower>(&mut recv, MAX_CONTROL_FRAME_BYTES).await {
            Ok(ControlToFollower::StreamStart {
                session_id,
                spec,
                song,
                gain_db_hundredths,
            }) => {
                debug!("StreamStart: session {session_id}, {}Hz {}ch", spec.rate, spec.channels);
                sessions.pending.lock().expect("lock poisoned").insert(
                    session_id,
                    PendingSession {
                        spec,
                        gain_db_hundredths,
                        correction_micros: Arc::new(AtomicI64::new(0)),
                    },
                );
                let _ = params.changes_tx.send(StateChangeEvent::CurrentSongEvent(Song {
                    title: Some(song.title).filter(|s| !s.is_empty()),
                    artist: Some(song.artist).filter(|s| !s.is_empty()),
                    album: Some(song.album).filter(|s| !s.is_empty()),
                    ..Default::default()
                }));
                let _ = params.changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING));
            }
            Ok(ControlToFollower::StreamStop { session_id }) => {
                debug!("StreamStop: session {session_id}");
                sessions.pending.lock().expect("lock poisoned").remove(&session_id);
                let sink = sessions.sinks.lock().expect("lock poisoned").remove(&session_id);
                if let Some(sink) = sink {
                    sink.stop();
                }
                let _ = params.changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED));
            }
            Ok(ControlToFollower::TimelineCorrection { session_id, offset_micros }) => {
                if let Some(pending) = sessions.pending.lock().expect("lock poisoned").get(&session_id) {
                    pending.correction_micros.store(offset_micros, Ordering::Release);
                }
            }
            Ok(ControlToFollower::SongProgress { current_secs, total_secs }) => {
                let _ = params.changes_tx.send(StateChangeEvent::SongTimeEvent(SongProgress {
                    total_time: Duration::from_secs(total_secs),
                    current_time: Duration::from_secs(current_secs),
                }));
            }
            Ok(ControlToFollower::GroupState(state)) => {
                let _ = events_tx.send(crate::Event::LeaderGroupState(state)).await;
            }
            Ok(ControlToFollower::Ping) => {
                let _ = write_frame(&mut send, &ControlToLeader::Pong).await;
            }
            Ok(other) => {
                debug!("Ignoring control message: {other:?}");
            }
            Err(e) => {
                info!("Leader control stream ended: {e:#}");
                break;
            }
        }
    }

    clock_task.abort();
    audio_task.abort();
    let sinks: Vec<SyncSink> = std::mem::take(&mut *sessions.sinks.lock().expect("lock poisoned"))
        .into_values()
        .collect();
    for sink in sinks {
        sink.stop();
    }
    let _ = params.changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED));
}

/// Sends clock probes (burst on join, then steady) and folds the answers
/// into the shared [`ClockState`].
async fn run_clock(conn: Connection, state: Arc<ClockState>) {
    const BURST_COUNT: u32 = 10;
    let mut estimator = OffsetEstimator::new();
    let mut id: u32 = 0;
    loop {
        let delay = if id < BURST_COUNT {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(2)
        };
        tokio::select! {
            () = tokio::time::sleep(delay) => {
                id += 1;
                let ping = ClockMsg::Ping { id, t1: MonoClock::now_micros() };
                if let Ok(bytes) = postcard::to_stdvec(&ping)
                    && conn.send_datagram(bytes.into()).is_err()
                {
                    break;
                }
            }
            dgram = conn.read_datagram() => {
                match dgram {
                    Ok(dgram) => {
                        if let Ok(ClockMsg::Pong { t1, t2, t3, .. }) = postcard::from_bytes::<ClockMsg>(&dgram) {
                            let t4 = MonoClock::now_micros();
                            estimator.add_sample(ExchangeSample { t1, t2, t3, t4 }, &state);
                        }
                    }
                    Err(e) => {
                        debug!("Clock datagram channel closed: {e}");
                        break;
                    }
                }
            }
        }
    }
}

/// Accepts one uni stream per audio session for the lifetime of the
/// connection.
async fn accept_audio_streams(conn: Connection, sessions: Arc<Sessions>, clock: Arc<ClockState>, params: SinkParams) {
    while let Ok(stream) = conn.accept_uni().await {
        let sessions = sessions.clone();
        let clock = clock.clone();
        let params = params.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_audio_stream(stream, &sessions, &clock, &params).await {
                warn!("Multiroom audio stream ended with error: {e:#}");
            }
        });
    }
}

async fn handle_audio_stream(mut stream: RecvStream, sessions: &Sessions, clock: &ClockState, params: &SinkParams) -> Result<()> {
    let first: AudioChunk = read_frame(&mut stream, MAX_AUDIO_FRAME_BYTES).await?;
    let session_id = first.session_id;

    // The matching StreamStart travels on the control stream and may arrive
    // after the first audio bytes.
    let info = wait_for_pending(sessions, session_id).await.context("no StreamStart for audio session")?;

    // Chunks are scheduled ~buffer_ms ahead, so a short wait for the first
    // clock fix does not lose them.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !clock.is_synced() {
        if std::time::Instant::now() > deadline {
            bail!("clock never synchronized with leader");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    info!(
        "Multiroom audio session {session_id} starting (clock offset {}µs, rtt {}µs)",
        clock.offset_micros(),
        clock.rtt_micros()
    );

    let (tx, rx) = std::sync::mpsc::sync_channel::<ScheduledChunk>(256);
    let sink = SyncSink::start(
        SyncSinkConfig {
            rate: info.spec.rate,
            channels: info.spec.channels,
            gain_db_hundredths: info.gain_db_hundredths,
            latency_offset_ms: params.latency_offset_ms,
            audio_device: params.audio_device.clone(),
            rsp_settings: params.rsp_settings.clone(),
        },
        params.dsp_handle.clone(),
        params.software_gain.clone(),
        params.vu_meter_enabled,
        params.changes_tx.clone(),
        rx,
    )?;
    sessions.sinks.lock().expect("lock poisoned").insert(session_id, sink);

    let forward = |chunk: AudioChunk| -> bool {
        let corrected = chunk
            .play_at_micros
            .saturating_add_signed(info.correction_micros.load(Ordering::Acquire));
        let local_play_at_micros = clock.leader_to_local_micros(corrected);
        let samples: Vec<f32> = chunk
            .payload
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        match tx.try_send(ScheduledChunk {
            local_play_at_micros,
            samples,
        }) {
            Ok(()) => true,
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                warn!("Multiroom sink backlog full, dropping a chunk");
                true
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => false,
        }
    };

    if forward(first) {
        // Runs until the stream finishes / the connection dies (read error)
        // or the sink is stopped by a StreamStop (forward returns false).
        while let Ok(chunk) = read_frame::<AudioChunk>(&mut stream, MAX_AUDIO_FRAME_BYTES).await {
            if !forward(chunk) {
                break;
            }
        }
    }

    // Natural end of the stream: disconnect the channel so the sink drains
    // its scheduled tail, then wait for it without signalling a stop.
    drop(tx);
    let sink = sessions.sinks.lock().expect("lock poisoned").remove(&session_id);
    if let Some(sink) = sink {
        sink.join();
    }
    Ok(())
}

async fn wait_for_pending(sessions: &Sessions, session_id: u64) -> Option<PendingSession> {
    for _ in 0..40 {
        let info = sessions.pending.lock().expect("lock poisoned").get(&session_id).cloned();
        if info.is_some() {
            return info;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}
