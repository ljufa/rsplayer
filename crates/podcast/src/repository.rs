//! Fjall-backed storage for subscriptions and episodes.
//!
//! Keyspaces:
//! * `podcasts` — `podcast_id` → [`Podcast`] JSON.
//! * `podcast_episodes` — `{podcast_id}/{u64::MAX - published}/{episode_id}`
//!   → [`Episode`] JSON, so a prefix scan yields a feed newest-first and a
//!   page is a plain skip/take.
//! * `podcast_episode_index` — `id:{episode_id}` and `url:{audio_url}` →
//!   episode key, for O(1) lookups from playback events.
//!
//! Re-importing a feed keeps the listener state (`position_secs`, `played`)
//! of episodes already stored.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use fjall::{Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use log::error;

use api_models::podcast::{Episode, Podcast};

pub trait PodcastRepository: Send + Sync {
    fn save_podcast(&self, podcast: &Podcast) -> Result<()>;
    fn get_podcast(&self, id: &str) -> Option<Podcast>;
    /// All subscriptions, sorted by title.
    fn list_podcasts(&self) -> Vec<Podcast>;
    /// Removes the podcast, its episodes and their index entries.
    fn delete_podcast(&self, id: &str) -> Result<()>;

    /// Inserts new episodes and refreshes feed-sourced fields of known ones
    /// (matched by id), preserving `position_secs`/`played`. Returns the
    /// number of episodes that were new.
    fn upsert_episodes(&self, episodes: &[Episode]) -> Result<usize>;
    /// Overwrites one episode (used for progress/played updates).
    fn save_episode(&self, episode: &Episode) -> Result<()>;
    fn get_episode(&self, id: &str) -> Option<Episode>;
    fn find_episode_by_url(&self, audio_url: &str) -> Option<Episode>;
    /// `(total, page)` of a podcast's episodes, newest first.
    fn episodes_page(&self, podcast_id: &str, offset: usize, limit: usize) -> (usize, Vec<Episode>);
    /// `(total, unplayed)` counts for a podcast.
    fn episode_counts(&self, podcast_id: &str) -> (usize, usize);
    /// Drops the oldest episodes beyond `keep`, never ones the listener has
    /// started. Returns how many were removed.
    fn prune_episodes(&self, podcast_id: &str, keep: usize) -> usize;
    fn flush(&self);
}

pub type ArcPodcastRepository = Arc<dyn PodcastRepository>;

pub struct FjallPodcastRepository {
    db: Database,
    podcasts: Keyspace,
    episodes: Keyspace,
    index: Keyspace,
}

fn episode_key(episode: &Episode) -> String {
    // Newest first: invert the publish timestamp. Unknown dates sort last.
    let ts = episode.published.map_or(0, |d| d.timestamp().max(0)).unsigned_abs();
    format!("{}/{:020}/{}", episode.podcast_id, u64::MAX - ts, episode.id)
}

fn id_index_key(id: &str) -> String {
    format!("id:{id}")
}

fn url_index_key(url: &str) -> String {
    format!("url:{url}")
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    serde_json::from_slice(bytes)
        .map_err(|e| error!("podcast store decode error: {e}"))
        .ok()
}

impl FjallPodcastRepository {
    pub fn new(db: &Database) -> Self {
        let open = |name: &str| {
            db.keyspace(name, KeyspaceCreateOptions::default)
                .unwrap_or_else(|e| panic!("Failed to open {name} keyspace: {e}"))
        };
        Self {
            db: db.clone(),
            podcasts: open("podcasts"),
            episodes: open("podcast_episodes"),
            index: open("podcast_episode_index"),
        }
    }

    /// Standalone constructor for tests — opens its own fjall database.
    pub fn new_standalone(db_path: &std::path::Path) -> Self {
        let db = Database::builder(db_path).open().expect("Failed to open podcast db");
        Self::new(&db)
    }

    fn episode_at(&self, key: &[u8]) -> Option<Episode> {
        let bytes = self.episodes.get(key).ok()??;
        decode(&bytes)
    }

    fn lookup(&self, index_key: &str) -> Option<Episode> {
        let key = self.index.get(index_key).ok()??;
        self.episode_at(&key)
    }

    fn write_episode(&self, episode: &Episode) -> Result<()> {
        let key = episode_key(episode);
        let json = serde_json::to_vec(episode).context("serialize episode")?;
        self.episodes.insert(&key, json).context("store episode")?;
        self.index.insert(id_index_key(&episode.id), key.as_bytes()).context("index episode id")?;
        self.index
            .insert(url_index_key(&episode.audio_url), key.as_bytes())
            .context("index episode url")?;
        Ok(())
    }

    fn remove_episode(&self, key: &[u8], episode: &Episode) {
        _ = self.episodes.remove(key);
        _ = self.index.remove(id_index_key(&episode.id));
        _ = self.index.remove(url_index_key(&episode.audio_url));
    }

    fn episode_entries(&self, podcast_id: &str) -> Vec<(Vec<u8>, Episode)> {
        self.episodes
            .prefix(format!("{podcast_id}/"))
            .filter_map(|guard| {
                let (key, value) = guard.into_inner().ok()?;
                let episode = decode::<Episode>(&value)?;
                Some((key.to_vec(), episode))
            })
            .collect()
    }
}

impl PodcastRepository for FjallPodcastRepository {
    fn save_podcast(&self, podcast: &Podcast) -> Result<()> {
        if podcast.id.is_empty() {
            return Err(anyhow!("refusing to save podcast without id"));
        }
        let json = serde_json::to_vec(podcast).context("serialize podcast")?;
        self.podcasts.insert(&podcast.id, json).context("store podcast")?;
        Ok(())
    }

    fn get_podcast(&self, id: &str) -> Option<Podcast> {
        let bytes = self.podcasts.get(id).ok()??;
        decode(&bytes)
    }

    fn list_podcasts(&self) -> Vec<Podcast> {
        let mut list: Vec<Podcast> = self
            .podcasts
            .iter()
            .filter_map(|guard| decode(&guard.value().ok()?))
            .collect();
        list.sort_by_key(|p| p.title.to_lowercase());
        list
    }

    fn delete_podcast(&self, id: &str) -> Result<()> {
        for (key, episode) in self.episode_entries(id) {
            self.remove_episode(&key, &episode);
        }
        self.podcasts.remove(id).context("delete podcast")?;
        Ok(())
    }

    fn upsert_episodes(&self, episodes: &[Episode]) -> Result<usize> {
        let mut added = 0;
        for incoming in episodes {
            if let Some(existing) = self.get_episode(&incoming.id) {
                let mut merged = incoming.clone();
                merged.position_secs = existing.position_secs;
                merged.played = existing.played;
                merged.local_path.clone_from(&existing.local_path);
                if merged != existing {
                    // The key embeds the publish date; drop the old row if it moved.
                    if episode_key(&existing) != episode_key(&merged) {
                        _ = self.episodes.remove(episode_key(&existing));
                    }
                    if existing.audio_url != merged.audio_url {
                        _ = self.index.remove(url_index_key(&existing.audio_url));
                    }
                    self.write_episode(&merged)?;
                }
            } else {
                self.write_episode(incoming)?;
                added += 1;
            }
        }
        Ok(added)
    }

    fn save_episode(&self, episode: &Episode) -> Result<()> {
        self.write_episode(episode)
    }

    fn get_episode(&self, id: &str) -> Option<Episode> {
        self.lookup(&id_index_key(id))
    }

    fn find_episode_by_url(&self, audio_url: &str) -> Option<Episode> {
        self.lookup(&url_index_key(audio_url))
    }

    fn episodes_page(&self, podcast_id: &str, offset: usize, limit: usize) -> (usize, Vec<Episode>) {
        let all = self.episode_entries(podcast_id);
        let total = all.len();
        let page = all.into_iter().skip(offset).take(limit).map(|(_, e)| e).collect();
        (total, page)
    }

    fn episode_counts(&self, podcast_id: &str) -> (usize, usize) {
        let all = self.episode_entries(podcast_id);
        let unplayed = all.iter().filter(|(_, e)| !e.played).count();
        (all.len(), unplayed)
    }

    fn prune_episodes(&self, podcast_id: &str, keep: usize) -> usize {
        let mut removed = 0;
        for (key, episode) in self.episode_entries(podcast_id).into_iter().skip(keep) {
            if episode.position_secs == 0 && episode.local_path.is_none() {
                self.remove_episode(&key, &episode);
                removed += 1;
            }
        }
        removed
    }

    fn flush(&self) {
        if let Err(e) = self.db.persist(PersistMode::SyncData) {
            error!("podcast store flush failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::TempDir;

    fn repo() -> (TempDir, FjallPodcastRepository) {
        let tmp = TempDir::new().unwrap();
        let repo = FjallPodcastRepository::new_standalone(&tmp.path().join("db"));
        (tmp, repo)
    }

    fn episode(podcast: &str, n: u32, day: u32) -> Episode {
        Episode {
            id: format!("{podcast}-e{n}"),
            podcast_id: podcast.into(),
            guid: format!("guid{n}"),
            title: format!("Episode {n}"),
            audio_url: format!("https://cdn/{podcast}/{n}.mp3"),
            published: Some(Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap()),
            duration_secs: Some(600),
            ..Default::default()
        }
    }

    #[test]
    fn podcasts_round_trip_sorted_by_title() {
        let (_t, repo) = repo();
        for (id, title) in [("b", "Zeta"), ("a", "alpha"), ("c", "Mid")] {
            repo.save_podcast(&Podcast {
                id: id.into(),
                title: title.into(),
                feed_url: format!("https://f/{id}"),
                ..Default::default()
            })
            .unwrap();
        }
        let titles: Vec<String> = repo.list_podcasts().into_iter().map(|p| p.title).collect();
        assert_eq!(titles, vec!["alpha", "Mid", "Zeta"]);
        assert_eq!(repo.get_podcast("a").unwrap().feed_url, "https://f/a");
        assert!(repo.save_podcast(&Podcast::default()).is_err());
    }

    #[test]
    fn episodes_page_newest_first_and_lookups() {
        let (_t, repo) = repo();
        let eps = vec![episode("p", 1, 1), episode("p", 2, 5), episode("p", 3, 3), episode("q", 9, 9)];
        assert_eq!(repo.upsert_episodes(&eps).unwrap(), 4);
        let (total, page) = repo.episodes_page("p", 0, 2);
        assert_eq!(total, 3);
        assert_eq!(page.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["p-e2", "p-e3"]);
        let (_, rest) = repo.episodes_page("p", 2, 10);
        assert_eq!(rest[0].id, "p-e1");
        assert_eq!(repo.get_episode("q-e9").unwrap().title, "Episode 9");
        assert_eq!(repo.find_episode_by_url("https://cdn/p/3.mp3").unwrap().id, "p-e3");
        assert!(repo.find_episode_by_url("https://nope").is_none());
    }

    #[test]
    fn upsert_preserves_listener_state_and_moves_rekeyed_rows() {
        let (_t, repo) = repo();
        let mut e = episode("p", 1, 1);
        repo.upsert_episodes(std::slice::from_ref(&e)).unwrap();
        e.position_secs = 120;
        e.played = true;
        repo.save_episode(&e).unwrap();

        // Feed now reports a new title and a corrected publish date.
        let mut refreshed = episode("p", 1, 20);
        refreshed.title = "Episode 1 (remastered)".into();
        assert_eq!(repo.upsert_episodes(&[refreshed]).unwrap(), 0);

        let stored = repo.get_episode("p-e1").unwrap();
        assert_eq!(stored.title, "Episode 1 (remastered)");
        assert_eq!(stored.position_secs, 120);
        assert!(stored.played);
        let (total, _) = repo.episodes_page("p", 0, 10);
        assert_eq!(total, 1, "old key must be removed after re-dating");
        assert_eq!(repo.episode_counts("p"), (1, 0));
    }

    #[test]
    fn prune_keeps_started_episodes_and_delete_removes_everything() {
        let (_t, repo) = repo();
        let mut eps: Vec<Episode> = (1..=5).map(|n| episode("p", n, n)).collect();
        eps[0].position_secs = 30; // oldest, but started
        repo.upsert_episodes(&eps).unwrap();
        assert_eq!(repo.prune_episodes("p", 2), 2);
        let (_, left) = repo.episodes_page("p", 0, 10);
        let ids: Vec<&str> = left.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["p-e5", "p-e4", "p-e1"]);

        repo.save_podcast(&Podcast {
            id: "p".into(),
            title: "P".into(),
            ..Default::default()
        })
        .unwrap();
        repo.delete_podcast("p").unwrap();
        assert!(repo.get_podcast("p").is_none());
        assert_eq!(repo.episodes_page("p", 0, 10).0, 0);
        assert!(repo.get_episode("p-e1").is_none());
        assert!(repo.find_episode_by_url("https://cdn/p/1.mp3").is_none());
    }
}
