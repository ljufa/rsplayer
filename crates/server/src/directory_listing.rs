//! Server-side folder browsing for the settings folder picker
//! (`StorageCommand::ListDirectories`).
//!
//! The web UI cannot open the server's filesystem itself (it may be a browser
//! on another machine, the desktop webview or the Android app), so the backend
//! lists one level of folders at a time. Browsing starts from the platform's
//! well-known folders ([`hardware::platform::library_root_candidates`]) plus
//! the ones only discoverable at runtime: Android removable storage under
//! `/storage`, Windows drive letters, and network filesystems mounted outside
//! `RSPlayer`.

use std::collections::HashSet;
use std::fs::{self, DirEntry};
use std::path::{Component, Path, PathBuf};

use api_models::state::{DirectoryEntry, DirectoryListing, ExternalMount, PathCrumb};
use hardware::platform::{self, LibraryRoot, TargetOs};

/// Sub-folders returned per listing; far more than any real library level.
const MAX_ENTRIES: usize = 2000;
/// Children inspected per sub-folder when counting, so one huge flat folder
/// on a slow disk cannot stall the listing.
const MAX_PEEK: usize = 5000;

/// The folders browsing starts from, most useful first, existing ones only.
pub fn library_roots(external_mounts: &[ExternalMount]) -> Vec<LibraryRoot> {
    let os = TargetOs::current();
    let default_music = platform::default_music_dir(os);
    let home_music = home_music_dir();
    let mut roots = platform::library_root_candidates(os, default_music.as_deref(), home_music.as_deref());
    match os {
        TargetOs::Android => roots.extend(storage_volumes(Path::new("/storage"))),
        TargetOs::Windows => roots.extend(windows_drives()),
        TargetOs::Linux | TargetOs::MacOs | TargetOs::Other => {}
    }
    roots.extend(external_mounts.iter().map(|mount| LibraryRoot {
        path: mount.mount_point.clone(),
        label: mount.source.clone(),
    }));
    let mut seen = HashSet::new();
    roots.retain(|root| Path::new(&root.path).is_dir() && seen.insert(root.path.clone()));
    roots
}

/// List `path` (or the roots, for an empty path).
pub fn list(path: &str, roots: &[LibraryRoot], extensions: &[String]) -> DirectoryListing {
    list_capped(path, roots, extensions, MAX_ENTRIES)
}

/// The reply sent instead of a listing when browsing is not allowed.
pub fn refused(path: &str, reason: &str) -> DirectoryListing {
    DirectoryListing {
        path: path.to_string(),
        error: Some(reason.to_string()),
        ..DirectoryListing::default()
    }
}

fn list_capped(path: &str, roots: &[LibraryRoot], extensions: &[String], cap: usize) -> DirectoryListing {
    let extensions: HashSet<String> = extensions.iter().map(|ext| ext.to_lowercase()).collect();
    if path.is_empty() {
        return DirectoryListing {
            entries: roots
                .iter()
                .map(|root| inspect(Path::new(&root.path), root.label.clone(), Some(root.label.clone()), &extensions))
                .collect(),
            ..DirectoryListing::default()
        };
    }

    let dir = Path::new(path);
    let mut listing = DirectoryListing {
        path: path.to_string(),
        parent: parent_of(dir, roots),
        breadcrumbs: breadcrumbs(dir, roots),
        ..DirectoryListing::default()
    };
    match fs::read_dir(dir) {
        Err(e) => listing.error = Some(e.to_string()),
        Ok(read) => {
            let mut folders: Vec<(String, PathBuf)> = read
                .filter_map(Result::ok)
                .filter(is_dir)
                .filter_map(|entry| {
                    // Non-UTF-8 names could not be sent back as a path to open.
                    let name = entry.file_name().into_string().ok()?;
                    (!is_hidden(&name)).then(|| (name, entry.path()))
                })
                .collect();
            folders.sort_by_cached_key(|(name, _)| name.to_lowercase());
            listing.truncated = folders.len() > cap;
            folders.truncate(cap);
            listing.entries = folders
                .into_iter()
                .map(|(name, path)| inspect(&path, name, None, &extensions))
                .collect();
        }
    }
    listing
}

/// Describe one folder: whether it can be opened and a shallow count of the
/// sub-folders and audio files it contains.
fn inspect(path: &Path, name: String, label: Option<String>, extensions: &HashSet<String>) -> DirectoryEntry {
    let mut entry = DirectoryEntry {
        name,
        path: path.to_string_lossy().into_owned(),
        label,
        subdirs: 0,
        audio_files: 0,
        readable: false,
    };
    let Ok(read) = fs::read_dir(path) else {
        return entry;
    };
    entry.readable = true;
    for child in read.filter_map(Result::ok).take(MAX_PEEK) {
        let file_name = child.file_name();
        let Some(child_name) = file_name.to_str() else { continue };
        if is_hidden(child_name) {
            continue;
        }
        if is_dir(&child) {
            entry.subdirs += 1;
        } else if Path::new(child_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.contains(&ext.to_lowercase()))
        {
            entry.audio_files += 1;
        }
    }
    entry
}

/// Directory test that follows symlinks but avoids a `stat` per plain entry.
fn is_dir(entry: &DirEntry) -> bool {
    match entry.file_type() {
        Ok(kind) if kind.is_symlink() => entry.path().is_dir(),
        Ok(kind) => kind.is_dir(),
        Err(_) => false,
    }
}

/// Dot-folders plus the system folders Windows and ext filesystems put at a
/// volume root — never a music library, and often unreadable.
fn is_hidden(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('$') || name == "lost+found" || name.eq_ignore_ascii_case("System Volume Information")
}

/// "Up" from a library root goes back to the roots, not to its (often
/// unreadable) parent such as `/storage/emulated`.
fn parent_of(dir: &Path, roots: &[LibraryRoot]) -> Option<String> {
    if roots.iter().any(|root| Path::new(&root.path) == dir) {
        return None;
    }
    dir.parent().map(|parent| parent.to_string_lossy().into_owned())
}

/// Crumbs from the deepest enclosing library root (shown by its label) down
/// to `dir`; from the filesystem root when no library root encloses it.
fn breadcrumbs(dir: &Path, roots: &[LibraryRoot]) -> Vec<PathCrumb> {
    let mut crumbs = Vec::new();
    let mut base = PathBuf::new();
    let enclosing = roots
        .iter()
        .filter(|root| dir.starts_with(&root.path))
        .max_by_key(|root| Path::new(&root.path).components().count());
    if let Some(root) = enclosing {
        base.push(&root.path);
        crumbs.push(PathCrumb {
            name: root.label.clone(),
            path: root.path.clone(),
        });
    }
    let rest = enclosing.map_or(dir, |root| dir.strip_prefix(&root.path).unwrap_or_else(|_| Path::new("")));
    for component in rest.components() {
        base.push(component);
        let name = match component {
            // A Windows drive prefix is shown together with its root dir.
            Component::Prefix(_) | Component::CurDir => continue,
            Component::RootDir => base.to_string_lossy().into_owned(),
            Component::Normal(name) => name.to_string_lossy().into_owned(),
            Component::ParentDir => "..".to_string(),
        };
        crumbs.push(PathCrumb {
            name,
            path: base.to_string_lossy().into_owned(),
        });
    }
    crumbs
}

#[cfg(not(target_os = "android"))]
fn home_music_dir() -> Option<String> {
    dirs::audio_dir().map(|dir| dir.to_string_lossy().into_owned())
}

#[cfg(target_os = "android")]
const fn home_music_dir() -> Option<String> {
    None
}

/// Android SD cards and USB drives appear as `/storage/<volume id>` next to
/// the internal `emulated` volume and the per-user `self` alias.
fn storage_volumes(storage: &Path) -> Vec<LibraryRoot> {
    let Ok(read) = fs::read_dir(storage) else {
        return Vec::new();
    };
    let mut volumes: Vec<LibraryRoot> = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (name != "self" && name != "emulated").then(|| LibraryRoot {
                path: entry.path().to_string_lossy().into_owned(),
                label: format!("External storage ({name})"),
            })
        })
        .collect();
    volumes.sort_by(|a, b| a.path.cmp(&b.path));
    volumes
}

/// Existing drive letters from `C:` on (A: and B: are legacy floppy letters
/// that can be slow to probe).
fn windows_drives() -> Vec<LibraryRoot> {
    (b'C'..=b'Z')
        .map(|letter| format!("{}:\\", char::from(letter)))
        .filter(|drive| Path::new(drive).is_dir())
        .map(|drive| LibraryRoot {
            label: format!("Drive {}", &drive[..2]),
            path: drive,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exts() -> Vec<String> {
        ["flac", "mp3"].iter().map(ToString::to_string).collect()
    }

    fn root(path: &Path, label: &str) -> LibraryRoot {
        LibraryRoot {
            path: path.to_string_lossy().into_owned(),
            label: label.to_string(),
        }
    }

    fn names(listing: &DirectoryListing) -> Vec<&str> {
        listing.entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn lists_visible_subfolders_only_sorted_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        for dir in ["beta", "Alpha", ".hidden", "lost+found"] {
            fs::create_dir(tmp.path().join(dir)).unwrap();
        }
        fs::write(tmp.path().join("song.flac"), b"").unwrap();
        let roots = [root(tmp.path(), "Music")];

        let listing = list(tmp.path().to_str().unwrap(), &roots, &exts());

        assert_eq!(names(&listing), ["Alpha", "beta"]);
        assert_eq!(listing.error, None);
        assert!(!listing.truncated);
    }

    #[test]
    fn counts_subfolders_and_audio_files_one_level_deep() {
        let tmp = tempfile::tempdir().unwrap();
        let album = tmp.path().join("Album");
        fs::create_dir_all(album.join("CD1").join("deeper")).unwrap();
        for file in ["01.FLAC", "02.mp3", "cover.jpg", ".03.flac"] {
            fs::write(album.join(file), b"").unwrap();
        }
        fs::write(album.join("CD1").join("04.flac"), b"").unwrap();
        let roots = [root(tmp.path(), "Music")];

        let listing = list(tmp.path().to_str().unwrap(), &roots, &exts());

        let entry = &listing.entries[0];
        assert_eq!((entry.subdirs, entry.audio_files, entry.readable), (1, 2, true));
        assert_eq!(entry.path, album.to_string_lossy());
        assert_eq!(entry.label, None);
    }

    #[test]
    fn root_has_no_parent_and_children_get_crumbs_from_the_root_label() {
        let tmp = tempfile::tempdir().unwrap();
        let disc = tmp.path().join("Album").join("CD1");
        fs::create_dir_all(&disc).unwrap();
        let roots = [root(tmp.path(), "Music")];

        let at_root = list(tmp.path().to_str().unwrap(), &roots, &exts());
        assert_eq!(at_root.parent, None);
        assert_eq!(at_root.breadcrumbs.len(), 1);
        assert_eq!(at_root.breadcrumbs[0].name, "Music");

        let at_disc = list(disc.to_str().unwrap(), &roots, &exts());
        let crumbs: Vec<&str> = at_disc.breadcrumbs.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(crumbs, ["Music", "Album", "CD1"]);
        assert_eq!(at_disc.breadcrumbs[2].path, disc.to_string_lossy());
        assert_eq!(at_disc.parent.as_deref(), Some(tmp.path().join("Album").to_str().unwrap()));
    }

    #[test]
    fn empty_path_lists_the_roots_by_label() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.mp3"), b"").unwrap();
        let roots = [root(tmp.path(), "Music")];

        let listing = list("", &roots, &exts());

        assert_eq!(names(&listing), ["Music"]);
        assert_eq!(listing.entries[0].label.as_deref(), Some("Music"));
        assert_eq!(listing.entries[0].audio_files, 1);
        assert!(listing.breadcrumbs.is_empty());
    }

    #[test]
    fn missing_folder_reports_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");

        let listing = list(missing.to_str().unwrap(), &[], &exts());

        assert!(listing.error.is_some());
        assert!(listing.entries.is_empty());
        assert_eq!(listing.path, missing.to_string_lossy());
    }

    #[test]
    fn listing_is_capped_and_flagged_as_truncated() {
        let tmp = tempfile::tempdir().unwrap();
        for dir in ["a", "b", "c"] {
            fs::create_dir(tmp.path().join(dir)).unwrap();
        }

        let listing = list_capped(tmp.path().to_str().unwrap(), &[], &exts(), 2);

        assert_eq!(names(&listing), ["a", "b"]);
        assert!(listing.truncated);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_folder_is_listed_as_not_readable() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let locked = tmp.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let readable_anyway = fs::read_dir(&locked).is_ok(); // running as root

        let listing = list(tmp.path().to_str().unwrap(), &[], &exts());

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if !readable_anyway {
            assert!(!listing.entries[0].readable);
        }
    }

    #[cfg(unix)]
    #[test]
    fn without_an_enclosing_root_crumbs_start_at_the_filesystem_root() {
        let tmp = tempfile::tempdir().unwrap();

        let listing = list(tmp.path().to_str().unwrap(), &[], &exts());

        assert_eq!(listing.breadcrumbs[0].name, "/");
        assert_eq!(listing.breadcrumbs.last().unwrap().path, tmp.path().to_string_lossy());
    }

    #[test]
    fn android_storage_volumes_skip_the_internal_aliases() {
        let tmp = tempfile::tempdir().unwrap();
        for dir in ["self", "emulated", "1A2B-3C4D"] {
            fs::create_dir(tmp.path().join(dir)).unwrap();
        }

        let volumes = storage_volumes(tmp.path());

        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].label, "External storage (1A2B-3C4D)");
    }

    #[test]
    fn refused_listing_echoes_the_path_with_the_reason() {
        let listing = refused("/srv", "disabled");
        assert_eq!((listing.path.as_str(), listing.error.as_deref()), ("/srv", Some("disabled")));
    }
}
