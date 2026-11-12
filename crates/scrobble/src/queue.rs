//! Persistent retry queue: scrobbles that could not be sent (service down,
//! rate limit, rejected credentials) wait here, per service, until a later
//! attempt succeeds. "Now playing" is never queued.
//!
//! Keyspace `scrobble_queue`: `{service}/{seq:020}` → [`Listen`] JSON, so a
//! prefix scan yields a service's scrobbles oldest first. Each service keeps
//! at most [`MAX_ENTRIES`]; beyond that the oldest are dropped.

use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use fjall::{Database, Keyspace};
use log::warn;

use crate::track::Listen;

pub const KEYSPACE: &str = "scrobble_queue";
pub const MAX_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Service {
    ListenBrainz,
    LastFm,
}

impl Service {
    pub const ALL: [Self; 2] = [Self::ListenBrainz, Self::LastFm];

    const fn prefix(self) -> &'static str {
        match self {
            Self::ListenBrainz => "lb/",
            Self::LastFm => "lfm/",
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ListenBrainz => "ListenBrainz",
            Self::LastFm => "Last.fm",
        }
    }
}

pub struct RetryQueue {
    ks: Keyspace,
    next_seq: AtomicU64,
    cap: usize,
}

/// A queued scrobble and the key to remove it with.
pub struct Entry {
    pub key: Vec<u8>,
    pub listen: Listen,
}

impl RetryQueue {
    pub fn new(db: &Database) -> Result<Self> {
        Self::with_capacity(db, MAX_ENTRIES)
    }

    pub fn with_capacity(db: &Database, cap: usize) -> Result<Self> {
        let ks = db
            .keyspace(KEYSPACE, config::db_maintenance::small_memtable_options)
            .with_context(|| format!("open {KEYSPACE} keyspace"))?;
        let last = Service::ALL
            .iter()
            .filter_map(|s| {
                let guard = ks.prefix(s.prefix()).next_back()?;
                let key = guard.key().ok()?;
                std::str::from_utf8(&key).ok()?.rsplit('/').next()?.parse::<u64>().ok()
            })
            .max()
            .unwrap_or(0);
        Ok(Self {
            ks,
            next_seq: AtomicU64::new(last + 1),
            cap,
        })
    }

    pub fn push(&self, service: Service, listen: &Listen) -> Result<()> {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let key = format!("{}{seq:020}", service.prefix());
        let json = serde_json::to_vec(listen).context("encode scrobble")?;
        self.ks.insert(key, json).context("store scrobble")?;
        let excess = self.len(service).saturating_sub(self.cap);
        if excess > 0 {
            warn!("{} retry queue full, dropping the {excess} oldest scrobble(s)", service.name());
            let oldest: Vec<Vec<u8>> = self
                .ks
                .prefix(service.prefix())
                .take(excess)
                .filter_map(|g| g.key().ok().map(|k| k.to_vec()))
                .collect();
            self.remove(&oldest);
        }
        Ok(())
    }

    /// Up to `n` oldest scrobbles of `service`. Entries that cannot be read
    /// are removed.
    pub fn batch(&self, service: Service, n: usize) -> Vec<Entry> {
        let mut broken = Vec::new();
        let entries = self
            .ks
            .prefix(service.prefix())
            .filter_map(|guard| {
                let (key, value) = guard.into_inner().ok()?;
                serde_json::from_slice::<Listen>(&value).map_or_else(
                    |_| {
                        broken.push(key.to_vec());
                        None
                    },
                    |listen| Some(Entry { key: key.to_vec(), listen }),
                )
            })
            .take(n)
            .collect();
        self.remove(&broken);
        entries
    }

    pub fn remove(&self, keys: &[Vec<u8>]) {
        for key in keys {
            if let Err(e) = self.ks.remove(key.as_slice()) {
                warn!("Failed to remove queued scrobble: {e}");
            }
        }
    }

    #[must_use]
    pub fn len(&self, service: Service) -> usize {
        self.ks.prefix(service.prefix()).count()
    }

    #[must_use]
    pub fn is_empty(&self, service: Service) -> bool {
        self.ks.prefix(service.prefix()).next().is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::ScrobbleTrack;
    use tempfile::TempDir;

    fn listen(n: i64) -> Listen {
        Listen {
            track: ScrobbleTrack {
                artist: "A".into(),
                title: format!("T{n}"),
                ..Default::default()
            },
            listened_at: n,
        }
    }

    #[test]
    fn survives_reopen_in_order_per_service() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("db");
        {
            let db = Database::builder(&path).open().unwrap();
            let q = RetryQueue::new(&db).unwrap();
            q.push(Service::ListenBrainz, &listen(1)).unwrap();
            q.push(Service::LastFm, &listen(2)).unwrap();
            q.push(Service::ListenBrainz, &listen(3)).unwrap();
            db.persist(fjall::PersistMode::SyncAll).unwrap();
        }
        let db = Database::builder(&path).open().unwrap();
        let q = RetryQueue::new(&db).unwrap();
        assert_eq!(q.len(Service::ListenBrainz), 2);
        assert_eq!(q.len(Service::LastFm), 1);
        // New entries go after the old ones.
        q.push(Service::ListenBrainz, &listen(4)).unwrap();
        let got: Vec<i64> = q.batch(Service::ListenBrainz, 10).iter().map(|e| e.listen.listened_at).collect();
        assert_eq!(got, vec![1, 3, 4]);
        let first_two: Vec<Vec<u8>> = q.batch(Service::ListenBrainz, 2).into_iter().map(|e| e.key).collect();
        q.remove(&first_two);
        assert_eq!(q.batch(Service::ListenBrainz, 10)[0].listen.listened_at, 4);
        assert!(!q.is_empty(Service::LastFm));
    }

    #[test]
    fn drops_oldest_beyond_capacity() {
        let tmp = TempDir::new().unwrap();
        let db = Database::builder(tmp.path().join("db")).open().unwrap();
        let q = RetryQueue::with_capacity(&db, 3).unwrap();
        for n in 1..=5 {
            q.push(Service::LastFm, &listen(n)).unwrap();
        }
        let got: Vec<i64> = q.batch(Service::LastFm, 10).iter().map(|e| e.listen.listened_at).collect();
        assert_eq!(got, vec![3, 4, 5]);
        assert!(q.is_empty(Service::ListenBrainz));
    }
}
