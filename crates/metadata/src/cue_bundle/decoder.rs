//! [`CueTrimDecoder`]: cuts decoded audio to a cue track's exact samples.

use symphonia::core::audio::{AsGenericAudioBufferRef, GenericAudioBuffer, GenericAudioBufferRef};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, FinalizeResult};
use symphonia::core::errors::Result;
use symphonia::core::packet::PacketRef;

/// Cuts decoded audio to the frames `[0, len)` of a cue track.
///
/// Wraps the file's decoder for packets from [`super::CueTrackReader`]
/// (timestamps rebased to the track start) and drops the part of the first
/// packet that belongs to the previous track and the part of the last packet
/// that belongs to the next. Packets fully inside the track pass through
/// without a copy.
pub struct CueTrimDecoder {
    inner: Box<dyn AudioDecoder>,
    /// Track length in frames, `None` = runs to the end of the file.
    len: Option<u64>,
    trimmed: Option<GenericAudioBuffer>,
    last_was_trimmed: bool,
}

impl CueTrimDecoder {
    pub fn new(inner: Box<dyn AudioDecoder>, len: Option<u64>) -> Self {
        Self {
            inner,
            len,
            trimmed: None,
            last_was_trimmed: false,
        }
    }
}

/// Frames to cut from the start and end of a buffer whose first frame is at
/// `first` (track time) and which holds `frames` frames.
fn cuts(first: i64, frames: u64, len: Option<u64>) -> (u64, u64) {
    let head = (-first).max(0).unsigned_abs().min(frames);
    let last = first.saturating_add(frames.cast_signed());
    let tail = len.map_or(0, |len| last.saturating_sub(len.cast_signed()).max(0).unsigned_abs());
    (head, tail.min(frames - head))
}

fn to_owned(buf: &GenericAudioBufferRef<'_>) -> GenericAudioBuffer {
    match buf {
        GenericAudioBufferRef::U8(b) => GenericAudioBuffer::U8((*b).clone()),
        GenericAudioBufferRef::U16(b) => GenericAudioBuffer::U16((*b).clone()),
        GenericAudioBufferRef::U24(b) => GenericAudioBuffer::U24((*b).clone()),
        GenericAudioBufferRef::U32(b) => GenericAudioBuffer::U32((*b).clone()),
        GenericAudioBufferRef::S8(b) => GenericAudioBuffer::S8((*b).clone()),
        GenericAudioBufferRef::S16(b) => GenericAudioBuffer::S16((*b).clone()),
        GenericAudioBufferRef::S24(b) => GenericAudioBuffer::S24((*b).clone()),
        GenericAudioBufferRef::S32(b) => GenericAudioBuffer::S32((*b).clone()),
        GenericAudioBufferRef::F32(b) => GenericAudioBuffer::F32((*b).clone()),
        GenericAudioBufferRef::F64(b) => GenericAudioBuffer::F64((*b).clone()),
    }
}

impl AudioDecoder for CueTrimDecoder {
    fn reset(&mut self) {
        self.inner.reset();
        self.trimmed = None;
        self.last_was_trimmed = false;
    }

    fn codec_info(&self) -> &CodecInfo {
        self.inner.codec_info()
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        self.inner.codec_params()
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        let first = packet.pts.get() + packet.trim_start.get().cast_signed();
        if cuts(first, packet.dur.get(), self.len) == (0, 0) {
            self.last_was_trimmed = false;
            return self.inner.decode_ref(packet);
        }
        let decoded = self.inner.decode_ref(packet)?;
        let (head, tail) = cuts(first, decoded.frames() as u64, self.len);
        let mut owned = to_owned(&decoded);
        #[allow(clippy::cast_possible_truncation)]
        owned.trim(head as usize, tail as usize);
        self.last_was_trimmed = true;
        Ok(self.trimmed.insert(owned).as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        self.inner.finalize()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        match (&self.trimmed, self.last_was_trimmed) {
            (Some(buf), true) => buf.as_generic_audio_buffer_ref(),
            _ => self.inner.last_decoded(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::cuts;

    #[test]
    fn computes_head_and_tail_cuts() {
        // Packet fully inside.
        assert_eq!(cuts(0, 4096, Some(10_000)), (0, 0));
        // Straddles the start: 1000 frames belong to the previous track.
        assert_eq!(cuts(-1000, 4096, Some(10_000)), (1000, 0));
        // Straddles the end.
        assert_eq!(cuts(8000, 4096, Some(10_000)), (0, 2096));
        // Track shorter than one packet: both ends cut.
        assert_eq!(cuts(-100, 4096, Some(500)), (100, 3496));
        // Last track of the file: no end cut.
        assert_eq!(cuts(1_000_000, 4096, None), (0, 0));
    }
}
