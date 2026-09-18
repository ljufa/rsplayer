//! Keeps the shared fjall journal small so startup replay stays fast.
//!
//! fjall keeps one write-ahead journal for all keyspaces and only truncates it
//! once every keyspace's memtable has been flushed. Its default flush trigger
//! is 64 MiB *per keyspace*, which slow, tiny writes (e.g. playback progress
//! once a second) can take months to reach, while every restart replays the
//! whole journal. [`reset_bloated_keyspaces`] runs once per database to drop
//! keyspaces created with that default, and [`flush_all`] forces memtables to
//! disk on demand.

use fjall::{Database, KeyspaceCreateOptions};
use log::{info, warn};

/// Memtable flush threshold for small, frequently rewritten keyspaces.
pub const SMALL_MEMTABLE_BYTES: u64 = 2 * 1024 * 1024;

const MIGRATIONS_KEYSPACE: &str = "_migrations";
const PLAYER_STATE_RESET_MARKER: &str = "player_state_reset_v1";

/// Options for keyspaces that receive many small writes.
#[must_use]
pub fn small_memtable_options() -> KeyspaceCreateOptions {
    KeyspaceCreateOptions::default().max_memtable_size(SMALL_MEMTABLE_BYTES)
}

/// Deletes `player_state` once so it is recreated with
/// [`small_memtable_options`]; stored options only apply at creation. Only the
/// resume position and paused flag are lost. Call before any service opens it.
pub fn reset_bloated_keyspaces(db: &Database) {
    let Ok(migrations) = db.keyspace(MIGRATIONS_KEYSPACE, KeyspaceCreateOptions::default) else {
        warn!("Maintenance: cannot open {MIGRATIONS_KEYSPACE} keyspace, skipping reset");
        return;
    };
    if migrations.get(PLAYER_STATE_RESET_MARKER).ok().flatten().is_some() {
        return;
    }
    if db.list_keyspace_names().iter().any(|n| &**n == "player_state") {
        match db.keyspace("player_state", KeyspaceCreateOptions::default) {
            Ok(ks) => match db.delete_keyspace(ks) {
                Ok(()) => info!("Maintenance: dropped bloated player_state keyspace"),
                Err(e) => {
                    warn!("Maintenance: failed to drop player_state: {e}");
                    return;
                }
            },
            Err(e) => {
                warn!("Maintenance: failed to open player_state: {e}");
                return;
            }
        }
    }
    if let Err(e) = migrations.insert(PLAYER_STATE_RESET_MARKER, b"1") {
        warn!("Maintenance: failed to store reset marker: {e}");
    }
}

/// Flushes every keyspace's active memtable to disk and waits, letting fjall
/// truncate the journal. Blocking; run from a blocking context.
pub fn flush_all(db: &Database) {
    for name in db.list_keyspace_names() {
        let Ok(ks) = db.keyspace(&name, KeyspaceCreateOptions::default) else {
            continue;
        };
        if let Err(e) = ks.rotate_memtable_and_wait() {
            warn!("Maintenance: flush of keyspace {} failed: {e}", &*name);
        }
    }
    info!("Maintenance: memtables flushed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_drops_player_state_once_and_flush_all_succeeds() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let db = Database::builder(tmp.path().join("t.db")).open().expect("open");
        let ks = db.keyspace("player_state", KeyspaceCreateOptions::default).expect("ks");
        ks.insert("k", "v").expect("insert");
        drop(ks);

        reset_bloated_keyspaces(&db);
        assert!(!db.list_keyspace_names().iter().any(|n| &**n == "player_state"));

        let ks = db.keyspace("player_state", small_memtable_options).expect("recreate");
        ks.insert("k2", "v2").expect("insert");
        reset_bloated_keyspaces(&db);
        assert_eq!(ks.get("k2").expect("get").as_deref(), Some(&b"v2"[..]));

        flush_all(&db);
        assert_eq!(ks.get("k2").expect("get").as_deref(), Some(&b"v2"[..]));
    }
}
