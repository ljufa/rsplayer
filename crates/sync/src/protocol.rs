//! Wire protocol between grouped rsplayer instances.
//!
//! Streams carry postcard-encoded messages framed with a u32 little-endian
//! length prefix. Clock probes travel as QUIC datagrams so they never queue
//! behind retransmitted stream bytes.

use anyhow::ensure;
use iroh::endpoint::{RecvStream, SendStream};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use api_models::state::MultiroomGroupState;

pub const ALPN: &[u8] = b"rsplayer/sync/1";
/// Version 2: audio payload changed from f32 LE to i16 LE.
pub const PROTOCOL_VERSION: u16 = 2;

/// Upper bound for control messages (group state, song metadata).
pub const MAX_CONTROL_FRAME_BYTES: u32 = 256 * 1024;
/// Upper bound for one audio chunk frame (fits >1s of 192kHz/2ch i16).
pub const MAX_AUDIO_FRAME_BYTES: u32 = 4 * 1024 * 1024;

/// Messages sent by the leader on the control (bidirectional) stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlToFollower {
    Hello {
        protocol_version: u16,
        leader_name: String,
    },
    /// Group membership as seen by the leader; the follower rewrites it to
    /// its own perspective before showing it in the UI.
    GroupState(MultiroomGroupState),
    /// A new audio session starts; its chunks arrive on a fresh uni stream.
    StreamStart {
        session_id: u64,
        spec: StreamSpec,
        song: SongMeta,
        /// Loudness-normalization gain in hundredths of dB, applied by the
        /// follower because the leader tees PCM before its DSP chain.
        gain_db_hundredths: Option<i32>,
    },
    /// Flush: stop playing the given session immediately (stop/seek/track change).
    StreamStop { session_id: u64 },
    /// Measured drift of the leader's actual output vs the session timeline
    /// (`actual − nominal`, µs). Followers add it to chunk timestamps.
    TimelineCorrection { session_id: u64, offset_micros: i64 },
    SongProgress { current_secs: u64, total_secs: u64 },
    Ping,
}

/// Messages sent by the follower on the control (bidirectional) stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlToLeader {
    HelloAck {
        protocol_version: u16,
        follower_name: String,
    },
    LeaveGroup,
    /// Telemetry: how much audio the follower currently has buffered.
    BufferReport { session_id: u64, buffered_ms: u32 },
    Pong,
}

/// PCM stream parameters. Samples are always interleaved i16 little-endian
/// (see [`encode_sample`] / [`decode_sample`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSpec {
    pub rate: u32,
    pub channels: u8,
}

/// Minimal song metadata so the follower UI can mirror the leader.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SongMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// One timestamped PCM chunk, sent on the per-session uni stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioChunk {
    pub session_id: u64,
    pub seq: u64,
    /// When to play the first frame, on the leader's monotonic clock (µs).
    pub play_at_micros: u64,
    pub frames: u32,
    /// Interleaved i16 little-endian samples.
    pub payload: Vec<u8>,
}

/// Converts one decoded f32 sample to the i16 LE wire format.
///
/// Scale is 32768 so that samples decoded from 16-bit sources (which
/// Symphonia converts to f32 by dividing by 32768) survive the wire
/// bit-exactly; 24-bit+ sources are quantized to 16 bits.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn encode_sample(s: f32) -> [u8; 2] {
    (((s * 32768.0).round()).clamp(-32768.0, 32767.0) as i16).to_le_bytes()
}

/// Converts one i16 LE wire sample back to f32.
#[must_use]
pub fn decode_sample(b: [u8; 2]) -> f32 {
    f32::from(i16::from_le_bytes(b)) / 32768.0
}

/// Clock-sync probes, exchanged as QUIC datagrams (postcard, no framing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockMsg {
    /// Follower → leader. `t1` = follower monotonic µs at send time.
    Ping { id: u32, t1: u64 },
    /// Leader → follower. `t2` = leader receive time, `t3` = leader send time.
    Pong { id: u32, t1: u64, t2: u64, t3: u64 },
}

/// Writes one length-prefixed postcard frame to a QUIC stream.
pub async fn write_frame<T: Serialize + Sync>(send: &mut SendStream, msg: &T) -> anyhow::Result<()> {
    let bytes = postcard::to_stdvec(msg)?;
    let len = u32::try_from(bytes.len())?;
    send.write_all(&len.to_le_bytes()).await?;
    send.write_all(&bytes).await?;
    Ok(())
}

/// Reads one length-prefixed postcard frame from a QUIC stream.
pub async fn read_frame<T: DeserializeOwned + Send>(recv: &mut RecvStream, max_len: u32) -> anyhow::Result<T> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf);
    ensure!(len <= max_len, "sync protocol frame too large: {len} bytes");
    let mut buf = vec![0u8; len as usize];
    recv.read_exact(&mut buf).await?;
    Ok(postcard::from_bytes(&buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_chunk_roundtrip() {
        let chunk = AudioChunk {
            session_id: 7,
            seq: 42,
            play_at_micros: 1_234_567,
            frames: 2,
            payload: [1.0f32, -1.0, 0.5, -0.5].iter().flat_map(|s| encode_sample(*s)).collect(),
        };
        let bytes = postcard::to_stdvec(&chunk).unwrap();
        let back: AudioChunk = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(chunk, back);
    }

    #[test]
    fn sample_wire_format_is_transparent_for_16bit_sources() {
        // A 16-bit source decoded the way Symphonia does it (/32768) must
        // survive encode → decode bit-exactly.
        for v in [i16::MIN, -12_345, -1, 0, 1, 12_345, i16::MAX] {
            let decoded = f32::from(v) / 32768.0;
            assert_eq!(decode_sample(encode_sample(decoded)), decoded, "sample {v}");
        }
    }

    #[test]
    fn sample_encoding_clamps_out_of_range() {
        assert_eq!(encode_sample(1.5), i16::MAX.to_le_bytes());
        assert_eq!(encode_sample(-1.5), i16::MIN.to_le_bytes());
        // 1.0 * 32768 overflows i16 and must clamp, not wrap.
        assert_eq!(encode_sample(1.0), i16::MAX.to_le_bytes());
        assert_eq!(encode_sample(-1.0), i16::MIN.to_le_bytes());
    }

    #[test]
    fn control_roundtrip() {
        let msg = ControlToFollower::StreamStart {
            session_id: 1,
            spec: StreamSpec { rate: 44_100, channels: 2 },
            song: SongMeta {
                title: "t".into(),
                artist: "a".into(),
                album: "b".into(),
            },
            gain_db_hundredths: Some(-125),
        };
        let bytes = postcard::to_stdvec(&msg).unwrap();
        let back: ControlToFollower = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(msg, back);
    }
}
