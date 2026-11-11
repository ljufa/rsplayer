//! Folder artwork fallback for files without an embedded picture.
//!
//! The scanner looks for a cover image next to the file (`cover.jpg`,
//! `folder.png`, ...). For multi-disc layouts (`Album/CD1/...`) the album
//! directory one level up is searched too.
//!
//! A folder image is stored once in the artwork directory under an id
//! derived from its path, size and modification time, so every track of an
//! album shares one file, and a changed image gets a new id.

use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use log::warn;

use crate::album_identity::is_disc_dir;

/// File stems tried in order of preference (case-insensitive).
const COVER_STEMS: &[&str] = &["cover", "folder", "front", "album", "albumart", "albumartsmall"];
/// Image extensions accepted (case-insensitive).
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];
/// Prefix of stored folder artwork ids, so they never clash with the uuids
/// used for embedded pictures.
const ID_PREFIX: &str = "folder-";

/// Returns the artwork id for the folder image belonging to `audio_file`,
/// copying the image into `artwork_dir` if it is not stored yet. `None` when
/// no folder image exists or it could not be stored.
#[must_use]
pub fn artwork_id_for(audio_file: &Path, artwork_dir: &Path) -> Option<String> {
    let image = find_cover(audio_file.parent()?)?;
    store(&image, artwork_dir)
}

/// The best cover image in `dir`, or in its parent when `dir` looks like a
/// disc subdirectory (`CD1`, `Disc 2`, ...).
#[must_use]
pub fn find_cover(dir: &Path) -> Option<PathBuf> {
    cover_in(dir).or_else(|| {
        let name = dir.file_name()?.to_str()?;
        if is_disc_dir(name) { cover_in(dir.parent()?) } else { None }
    })
}

fn cover_in(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Some(rank) = cover_rank(&path) else { continue };
        if best.as_ref().is_none_or(|(r, p)| rank < *r || (rank == *r && path < *p)) && path.is_file() {
            best = Some((rank, path));
        }
    }
    best.map(|(_, p)| p)
}

/// Preference of `path` as a cover image (lower is better), `None` when it
/// is not a recognised cover file name.
fn cover_rank(path: &Path) -> Option<usize> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let ext_rank = IMAGE_EXTS.iter().position(|e| *e == ext)?;
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let stem_rank = COVER_STEMS.iter().position(|s| *s == stem)?;
    Some(stem_rank * IMAGE_EXTS.len() + ext_rank)
}

/// Copies `image` into `artwork_dir` (once) and returns its id.
fn store(image: &Path, artwork_dir: &Path) -> Option<String> {
    let meta = fs::metadata(image).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let id = image_id(image, meta.len(), modified);
    let target = artwork_dir.join(&id);
    if target.exists() {
        return Some(id);
    }
    // Write under a temporary name first: scan workers may race on the same
    // album, and a reader must never see a half-written file.
    let tmp = artwork_dir.join(format!("{id}.{}.tmp", std::process::id()));
    let result = fs::copy(image, &tmp).and_then(|_| fs::rename(&tmp, &target));
    match result {
        Ok(()) => Some(id),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            if target.exists() {
                Some(id)
            } else {
                warn!("Error storing folder artwork {}: {e}", image.display());
                None
            }
        }
    }
}

/// Stable id (FNV-1a 64) of an image path, size and modification time.
fn image_id(path: &Path, size: u64, modified: u64) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let bytes = path.to_string_lossy();
    for b in bytes.as_bytes().iter().chain(&size.to_le_bytes()).chain(&modified.to_le_bytes()) {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{ID_PREFIX}{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rsp_folder_art_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn prefers_cover_over_folder_and_ignores_case() {
        let dir = temp_dir("prefer");
        fs::write(dir.join("Folder.jpg"), b"f").unwrap();
        fs::write(dir.join("COVER.PNG"), b"c").unwrap();
        fs::write(dir.join("back.jpg"), b"b").unwrap();
        assert_eq!(find_cover(&dir).unwrap().file_name().unwrap(), "COVER.PNG");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn none_without_known_names() {
        let dir = temp_dir("none");
        fs::write(dir.join("scan01.jpg"), b"x").unwrap();
        fs::write(dir.join("cover.txt"), b"x").unwrap();
        assert!(find_cover(&dir).is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn disc_dir_falls_back_to_album_dir() {
        let dir = temp_dir("disc");
        let cd = dir.join("CD 2");
        fs::create_dir_all(&cd).unwrap();
        fs::write(dir.join("folder.jpg"), b"f").unwrap();
        assert_eq!(find_cover(&cd).unwrap(), dir.join("folder.jpg"));
        // A non-disc subdirectory does not look upwards.
        let other = dir.join("Bonus");
        fs::create_dir_all(&other).unwrap();
        assert!(find_cover(&other).is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stores_once_per_image() {
        let dir = temp_dir("store");
        let art = dir.join("artwork");
        fs::create_dir_all(&art).unwrap();
        fs::write(dir.join("cover.jpg"), b"image-bytes").unwrap();
        let a = artwork_id_for(&dir.join("01.flac"), &art).unwrap();
        let b = artwork_id_for(&dir.join("02.flac"), &art).unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with(ID_PREFIX));
        assert_eq!(fs::read(art.join(&a)).unwrap(), b"image-bytes");
        assert_eq!(fs::read_dir(&art).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
