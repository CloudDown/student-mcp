//! Process-local, expiring documents. An ID never replaces account authorization.
use super::Prepared;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(300);
const MAX_FILES: usize = 4;
const MAX_BYTES: usize = 24 * 1024 * 1024;
static CACHE: Mutex<Vec<Arc<Document>>> = Mutex::new(Vec::new());

pub(super) struct Document {
    pub id: String,
    scope: Option<[u8; 32]>,
    digest: [u8; 32],
    pub filename: String,
    pub mime: String,
    pub bytes: Arc<Vec<u8>>,
    pub prepared: OnceLock<Result<Prepared, String>>,
    expires: Instant,
}

impl Document {
    pub fn new(
        filename: &str,
        mime: &str,
        bytes: Arc<Vec<u8>>,
        scope: Option<[u8; 32]>,
    ) -> Arc<Self> {
        Arc::new(Self {
            id: crate::config::random_hex(16),
            scope,
            digest: Sha256::digest(bytes.as_slice()).into(),
            filename: filename.into(),
            mime: mime.into(),
            bytes,
            prepared: OnceLock::new(),
            expires: Instant::now() + TTL,
        })
    }

    pub fn remaining_seconds(&self) -> u64 {
        self.expires
            .saturating_duration_since(Instant::now())
            .as_secs()
    }
}

pub(super) fn remember(document: Arc<Document>) -> Arc<Document> {
    if document.scope.is_none() || document.bytes.len() > MAX_BYTES {
        return document;
    }
    let Ok(mut cache) = CACHE.lock() else {
        return document;
    };
    cache.retain(|entry| entry.expires > Instant::now());
    if let Some(known) = cache.iter().find(|entry| {
        entry.scope == document.scope
            && entry.digest == document.digest
            && entry.filename == document.filename
            && entry.mime == document.mime
    }) {
        return Arc::clone(known);
    }
    // Active Arc readers remain valid while the bounded lookup store evicts an entry.
    while cache.len() >= MAX_FILES
        || cache.iter().map(|entry| entry.bytes.len()).sum::<usize>() + document.bytes.len()
            > MAX_BYTES
    {
        cache.remove(0);
    }
    cache.push(Arc::clone(&document));
    drop(cache);
    crate::cache::start_expiry_worker();
    document
}

pub(super) fn get(scope: [u8; 32], id: &str) -> Result<Arc<Document>, String> {
    let mut cache = CACHE.lock().map_err(|_| "Document cache unavailable.")?;
    cache.retain(|entry| entry.expires > Instant::now());
    cache.iter().find(|entry| entry.scope == Some(scope) && entry.id == id)
        .map(Arc::clone)
        .ok_or_else(|| "Document unavailable for this account or expired. Call the same file tool with its original school file identifiers to load it again.".into())
}

pub(crate) fn expire(now: Instant) {
    if let Ok(mut cache) = CACHE.lock() {
        cache.retain(|entry| entry.expires > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_scoped_and_duplicate_reads_share_preparation() {
        let scope = Some([31; 32]);
        let first = remember(Document::new(
            "notes.pdf",
            "application/pdf",
            Arc::new(vec![31]),
            scope,
        ));
        assert!(first
            .prepared
            .set(Ok(Prepared::Text("prepared once".into())))
            .is_ok());
        let again = remember(Document::new(
            "notes.pdf",
            "application/pdf",
            Arc::new(vec![31]),
            scope,
        ));
        assert!(Arc::ptr_eq(&first, &again));
        assert!(matches!(
            again
                .prepared
                .get_or_init(|| panic!("must not prepare twice")),
            Ok(Prepared::Text(_))
        ));
        assert!(get([32; 32], &first.id).is_err());
        let other = remember(Document::new(
            "notes.pdf",
            "application/pdf",
            Arc::new(vec![31]),
            Some([32; 32]),
        ));
        assert_ne!(first.id, other.id);
        assert!(first.remaining_seconds() <= 300);
    }

    #[test]
    fn expired_ids_are_reclaimed_at_lookup() {
        let mut document = Document::new(
            "expired.txt",
            "text/plain",
            Arc::new(vec![1]),
            Some([93; 32]),
        );
        Arc::get_mut(&mut document).unwrap().expires = Instant::now() - Duration::from_secs(1);
        let id = document.id.clone();
        remember(document);
        assert!(get([93; 32], &id).is_err());
    }
}
