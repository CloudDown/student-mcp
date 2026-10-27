//! Short-lived, account-scoped cache for successive reads of the same document.
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) struct File {
    pub filename: String,
    pub mime: String,
    pub bytes: Arc<Vec<u8>>,
}

struct Entry {
    key: [u8; 32],
    file: Arc<File>,
    expires: Instant,
}

#[derive(Default)]
struct Cache(Vec<Entry>);

impl Cache {
    fn get(&mut self, url: &str, now: Instant) -> Option<Arc<File>> {
        self.0.retain(|entry| entry.expires > now);
        let key: [u8; 32] = Sha256::digest(url.as_bytes()).into();
        self.0
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| Arc::clone(&entry.file))
    }

    fn insert(&mut self, url: &str, file: Arc<File>, now: Instant) {
        const MAX_BYTES: usize = 24 * 1024 * 1024;
        if file.bytes.len() > MAX_BYTES {
            return;
        }
        let key: [u8; 32] = Sha256::digest(url.as_bytes()).into();
        self.0
            .retain(|entry| entry.expires > now && entry.key != key);
        self.0.push(Entry {
            key,
            file,
            expires: now + Duration::from_secs(60),
        });
        while self.0.len() > 4
            || self
                .0
                .iter()
                .map(|entry| entry.file.bytes.len())
                .sum::<usize>()
                > MAX_BYTES
        {
            self.0.remove(0);
        }
    }
}

static CACHE: Mutex<Cache> = Mutex::new(Cache(Vec::new()));

pub(super) fn get(url: &str) -> Option<Arc<File>> {
    CACHE.lock().ok()?.get(url, Instant::now())
}

pub(super) fn insert(url: &str, file: Arc<File>) {
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(url, file, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_cache_expires_and_never_crosses_account_tokens() {
        let mut cache = Cache::default();
        let now = Instant::now();
        let file = Arc::new(File {
            filename: "notes.pdf".into(),
            mime: "application/pdf".into(),
            bytes: Arc::new(vec![1]),
        });
        cache.insert("https://school/file?token=alice", Arc::clone(&file), now);
        assert!(Arc::ptr_eq(
            &cache.get("https://school/file?token=alice", now).unwrap(),
            &file
        ));
        assert!(cache.get("https://school/file?token=bob", now).is_none());
        assert!(cache
            .get(
                "https://school/file?token=alice",
                now + Duration::from_secs(61)
            )
            .is_none());
    }
}
