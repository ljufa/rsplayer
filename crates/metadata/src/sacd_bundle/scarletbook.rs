use std::io::{Read, Seek, SeekFrom};

use anyhow::{anyhow, Result};

pub const MASTER_TOC_LBA: u64 = 510;
pub const SACD_LSN_SIZE: u64 = 2048;
/// DST frame header size for `frame_format=2` sectors (8-byte sync + 24 zero bytes).
/// `frame_format=0` sectors are raw DSD with no header.
const SACD_SECTOR_HEADER_FF2: u64 = 32;

/// Returns the number of DSD audio bytes per sector for the given `frame_format`.
/// Only `frame_format=2` (uncompressed DSD) is supported; it has a 32-byte sector header.
pub const fn audio_bytes_per_sector(_frame_format: u8) -> u64 {
    SACD_LSN_SIZE - SACD_SECTOR_HEADER_FF2
}

/// Returns the byte offset within each sector where DSD audio data begins.
pub const fn sector_header_size(_frame_format: u8) -> u64 {
    SACD_SECTOR_HEADER_FF2
}
const SACD_PSN_SIZE: u64 = 2064;
const SACD_PSN_HEADER: u64 = 16;
pub const SACD_SAMPLING_FREQUENCY: u32 = 2_822_400;

const MASTER_TOC_MARKER: &[u8; 8] = b"SACDMTOC";
const TWOCH_TOC_MARKER: &[u8; 8] = b"TWOCHTOC";
const MULCH_TOC_MARKER: &[u8; 8] = b"MULCHTOC";
const SACDTRL1_MARKER: &[u8; 8] = b"SACDTRL1";

/// Sector encoding of the ISO: either raw 2064-byte physical sectors or 2048-byte data-only.
#[derive(Debug, Clone, Copy)]
pub enum SectorMode {
    DataOnly,
    Physical,
}

impl SectorMode {
    /// Byte offset in the file for the start of a sector's 2048-byte data payload.
    pub const fn data_offset(self, lba: u64) -> u64 {
        match self {
            SectorMode::DataOnly => lba * SACD_LSN_SIZE,
            SectorMode::Physical => lba * SACD_PSN_SIZE + SACD_PSN_HEADER,
        }
    }
}

/// A parsed SACD area (stereo or multichannel).
#[derive(Debug, Clone)]
pub struct SacdArea {
    pub toc_start_lba: u32,
    /// Number of sectors in the area TOC region (used to bound SACDTRL1 scan).
    pub toc_size: u16,
    pub channel_count: u8,
    pub track_count: u8,
    /// Offset into the tracklist arrays where this area's tracks start.
    pub track_offset: u8,
    pub is_stereo: bool,
    /// 0 = uncompressed DSD, 3 = DST compressed (unsupported).
    pub frame_format: u8,
}

/// Start sector and length in sectors for one SACD audio track.
#[derive(Debug, Clone)]
pub struct SacdTrack {
    pub start_lsn: u32,
    pub length_lsn: u32,
}

impl SacdTrack {
    /// Duration in seconds at DSD64 for the given channel count and frame format.
    pub fn duration_secs(&self, channel_count: u8, frame_format: u8) -> f64 {
        let audio_bytes = f64::from(self.length_lsn) * audio_bytes_per_sector(frame_format) as f64;
        // DSD64 bit rate per channel: 2,822,400 bits/sec = 352,800 bytes/sec
        let bytes_per_sec = f64::from(SACD_SAMPLING_FREQUENCY) / 8.0;
        audio_bytes / (f64::from(channel_count) * bytes_per_sec)
    }

    /// Total DSD frames (one frame = one bit per channel) for the given channel count and frame format.
    pub fn total_dsd_frames(&self, channel_count: u8, frame_format: u8) -> u64 {
        u64::from(self.length_lsn) * audio_bytes_per_sector(frame_format) * 8 / u64::from(channel_count)
    }
}

fn read_u16_be(data: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

fn read_u32_be(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

fn read_sector<F: Read + Seek>(
    file: &mut F,
    mode: SectorMode,
    lba: u64,
) -> std::io::Result<[u8; SACD_LSN_SIZE as usize]> {
    file.seek(SeekFrom::Start(mode.data_offset(lba)))?;
    let mut buf = [0u8; SACD_LSN_SIZE as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Try to detect the sector encoding by looking for the SACDMTOC signature at LBA 510.
pub fn detect_sector_mode<F: Read + Seek>(file: &mut F) -> Result<SectorMode> {
    let mut marker = [0u8; 8];

    // Try physical 2064-byte sectors first.
    let phys_offset = MASTER_TOC_LBA * SACD_PSN_SIZE + SACD_PSN_HEADER;
    if file.seek(SeekFrom::Start(phys_offset)).is_ok()
        && file.read_exact(&mut marker).is_ok()
        && &marker == MASTER_TOC_MARKER
    {
        return Ok(SectorMode::Physical);
    }

    // Fall back to data-only 2048-byte sectors.
    let data_offset = MASTER_TOC_LBA * SACD_LSN_SIZE;
    if file.seek(SeekFrom::Start(data_offset)).is_ok()
        && file.read_exact(&mut marker).is_ok()
        && &marker == MASTER_TOC_MARKER
    {
        return Ok(SectorMode::DataOnly);
    }

    Err(anyhow!(
        "Not a valid SACD ISO: SACDMTOC signature not found at LBA {MASTER_TOC_LBA}"
    ))
}

/// Read all areas from the Master TOC.
pub fn read_areas<F: Read + Seek>(file: &mut F, mode: SectorMode) -> Result<Vec<SacdArea>> {
    let master_toc = read_sector(file, mode, MASTER_TOC_LBA).map_err(|e| anyhow!("Failed to read Master TOC: {e}"))?;

    if &master_toc[0..8] != MASTER_TOC_MARKER {
        return Err(anyhow!("Invalid Master TOC signature"));
    }

    // master_toc_t PACKED byte layout:
    // [64..68] area_1_toc_1_start (u32 BE)
    // [72..76] area_2_toc_1_start (u32 BE)
    // [84..86] area_1_toc_size    (u16 BE)
    // [86..88] area_2_toc_size    (u16 BE)
    let area_1_start = read_u32_be(&master_toc, 64);
    let area_2_start = read_u32_be(&master_toc, 72);
    let area_1_size = read_u16_be(&master_toc, 84);
    let area_2_size = read_u16_be(&master_toc, 86);

    let mut areas = Vec::new();
    for (start, size) in [(area_1_start, area_1_size), (area_2_start, area_2_size)] {
        if start == 0 {
            continue;
        }
        match read_area_toc(file, mode, start, size) {
            Ok(a) => areas.push(a),
            Err(e) => log::warn!("sacd: skipping area TOC at LBA {start}: {e}"),
        }
    }

    if areas.is_empty() {
        return Err(anyhow!("No valid SACD areas found"));
    }
    Ok(areas)
}

fn read_area_toc<F: Read + Seek>(
    file: &mut F,
    mode: SectorMode,
    toc_start_lba: u32,
    toc_size: u16,
) -> Result<SacdArea> {
    let toc =
        read_sector(file, mode, u64::from(toc_start_lba)).map_err(|e| anyhow!("IO error reading area TOC: {e}"))?;

    let id = &toc[0..8];
    let is_stereo = id == TWOCH_TOC_MARKER;
    if id != TWOCH_TOC_MARKER && id != MULCH_TOC_MARKER {
        return Err(anyhow!("Unknown area TOC marker: {}", String::from_utf8_lossy(id)));
    }

    // area_toc_t PACKED byte layout:
    // [21]  frame_format (lower 4 bits): 0/2 = uncompressed DSD, 3 = DST
    // [32]  channel_count
    // [68]  track_offset
    // [69]  track_count
    let frame_format = toc[21] & 0x0F;
    let channel_count = toc[32];
    let track_offset = toc[68];
    let track_count = toc[69];

    Ok(SacdArea {
        toc_start_lba,
        toc_size,
        channel_count,
        track_count,
        track_offset,
        is_stereo,
        frame_format,
    })
}

/// Read per-track start/length from the SACDTRL1 sector embedded in the area TOC region.
pub fn read_tracks<F: Read + Seek>(file: &mut F, mode: SectorMode, area: &SacdArea) -> Result<Vec<SacdTrack>> {
    // Only frame_format=2 is uncompressed DSD (with 32-byte sector header).
    // frame_format=0 and frame_format=3 are DST-compressed and cannot be decoded.
    if area.frame_format != 2 {
        return Err(anyhow!(
            "DST-compressed SACD (frame_format={}) is not supported; only uncompressed DSD (frame_format=2) is playable",
            area.frame_format
        ));
    }

    // The SACDTRL1 sector is somewhere within the area TOC region. Scan for it rather
    // than relying on a hardcoded byte offset in the area TOC header (which varies across rippers).
    let scan_limit = u64::from(area.toc_size).max(32);
    let trl1 = (0..scan_limit)
        .find_map(|offset| {
            let lba = u64::from(area.toc_start_lba) + offset;
            read_sector(file, mode, lba)
                .ok()
                .filter(|s| &s[0..8] == SACDTRL1_MARKER)
        })
        .ok_or_else(|| {
            anyhow!(
                "SACDTRL1 sector not found in area TOC (scanned {} sectors from LBA {})",
                scan_limit,
                area.toc_start_lba
            )
        })?;

    // area_tracklist_offset_t PACKED layout:
    // [8..1028]    track_start_lsn[255]  (255 × u32 BE = 1020 bytes)
    // [1028..2048] track_length_lsn[255] (255 × u32 BE = 1020 bytes)
    let base = area.track_offset as usize;
    let mut tracks = Vec::new();

    for i in 0..area.track_count as usize {
        let idx = base + i;
        if idx >= 255 {
            break;
        }
        let start_off = 8 + idx * 4;
        let len_off = 8 + 255 * 4 + idx * 4;
        let start_lsn = read_u32_be(&trl1, start_off);
        let length_lsn = read_u32_be(&trl1, len_off);
        tracks.push(SacdTrack { start_lsn, length_lsn });
    }

    Ok(tracks)
}

/// Read `sector_count` consecutive sectors starting at `start_lsn` and return the audio data
/// de-interleaved into planar channel order expected by `DsdDecoder`.
///
/// SACD sectors store DSD bytes channel-interleaved (byte 0 = ch0, byte 1 = ch1, …).
/// `DsdDecoder` expects planar layout: all ch0 bytes first, then all ch1 bytes, etc.
pub fn read_audio_sectors<F: Read + Seek>(
    file: &mut F,
    mode: SectorMode,
    start_lsn: u64,
    sector_count: u64,
    channel_count: usize,
    frame_format: u8,
) -> std::io::Result<Vec<u8>> {
    let hdr = sector_header_size(frame_format) as usize;
    let audio_per_sector = SACD_LSN_SIZE as usize - hdr;
    let total_bytes = sector_count as usize * audio_per_sector;
    let mut interleaved = vec![0u8; total_bytes];

    let mut sector_buf = [0u8; SACD_LSN_SIZE as usize];
    for i in 0..sector_count {
        let offset = mode.data_offset(start_lsn + i);
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut sector_buf)?;
        let dst = i as usize * audio_per_sector;
        interleaved[dst..dst + audio_per_sector].copy_from_slice(&sector_buf[hdr..]);
    }

    if channel_count <= 1 {
        return Ok(interleaved);
    }

    // De-interleave: [ch0,ch1,ch0,ch1,…] → [ch0,ch0,…,ch1,ch1,…]
    let bytes_per_channel = total_bytes / channel_count;
    let mut planar = vec![0u8; total_bytes];
    for (frame, chunk) in interleaved.chunks(channel_count).enumerate() {
        for (ch, &byte) in chunk.iter().enumerate() {
            planar[ch * bytes_per_channel + frame] = byte;
        }
    }

    Ok(planar)
}
