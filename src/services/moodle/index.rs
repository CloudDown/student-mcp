//! Private, bounded metadata snapshots. No file downloads, LLM or credentials.
//! Refresh never falls back to stale content after an upstream error.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const TTL: i64 = 15 * 60;
const MAX_BYTES: u64 = 24 * 1024 * 1024;
const MAX_RECORDS: usize = 128;

pub(super) struct Index {
    root: PathBuf,
    key: String,
    snapshot: std::sync::Mutex<Option<Data>>,
}

#[derive(Default, Serialize, Deserialize)]
struct Data {
    version: u32,
    records: BTreeMap<String, Record>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    fetched: i64,
    value: Value,
}

impl Index {
    pub fn for_account(base: &str, token: &str, account: &str) -> Option<Self> {
        // Unit fixtures must never touch real configuration or real index files.
        #[cfg(test)]
        crate::config::tests::override_config_path()?;
        let digest = Sha256::digest(serde_json::to_vec(&(base, token, account)).ok()?);
        let key = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut index = Self {
            root: crate::config::config_path().with_extension("index"),
            key,
            snapshot: std::sync::Mutex::new(None),
        };
        let data = index.read();
        *index.snapshot.get_mut().ok()? = data;
        Some(index)
    }

    fn path(&self) -> PathBuf {
        self.root.join(format!("{}.json", self.key))
    }

    fn read(&self) -> Option<Data> {
        if !fs::symlink_metadata(&self.root).ok()?.is_dir() {
            return None;
        }
        let path = self.path();
        let meta = fs::symlink_metadata(&path).ok()?;
        if !meta.is_file() || meta.len() > MAX_BYTES {
            return None;
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() as u64 > MAX_BYTES {
            return None;
        }
        let data: Data = serde_json::from_slice(&bytes).ok()?;
        (data.version == 1).then_some(data)
    }

    pub fn get(&self, key: &str, now: i64) -> Option<(Value, Value)> {
        // Do not read through an externally replaced directory symlink either.
        if !fs::symlink_metadata(&self.root).ok()?.is_dir() {
            return None;
        }
        let snapshot = self.snapshot.lock().ok()?;
        let record = snapshot.as_ref()?.records.get(key)?;
        let entries = record.value.as_array()?;
        if entries.iter().any(|entry| entry["id"].as_i64().is_none()) {
            return None;
        }
        if key.starts_with("course:")
            && entries.iter().any(|section| {
                section["modules"].as_array().is_none_or(|modules| {
                    modules.iter().any(|module| module["id"].as_i64().is_none())
                })
            })
        {
            return None;
        }
        if record.fetched > now || now - record.fetched >= TTL {
            return None;
        }
        Some((
            record.value.clone(),
            json!({"status":"cached", "fetchedAt":record.fetched,
            "ageSeconds":now - record.fetched, "expiresInSeconds":TTL - (now - record.fetched)}),
        ))
    }

    /// Cache failure must never turn a successful school read into a tool error.
    pub fn put(&self, key: &str, value: &Value, now: i64) -> bool {
        self.save(key, value, now).is_some()
    }

    fn save(&self, key: &str, value: &Value, now: i64) -> Option<()> {
        if value.to_string().len() as u64 > MAX_BYTES / 2 {
            return None;
        }
        private_dir(&self.root)?;
        // Serialize short local read/merge/write/prune operations across processes.
        // Never hold this lock over a school request, unlink it or follow symlinks.
        let lock_path = self.root.join("index.lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|meta| !meta.is_file()) {
            return None;
        }
        let lock = private_file()
            .create(true)
            .truncate(false)
            .open(lock_path)
            .ok()?;
        lock.lock_exclusive().ok()?;
        let path = self.path();
        if fs::symlink_metadata(&path).is_ok_and(|meta| !meta.is_file()) {
            return None;
        }
        let mut data = self.read().unwrap_or_default();
        data.version = 1;
        data.records
            .retain(|_, record| record.fetched <= now && now - record.fetched < TTL);
        data.records.insert(
            key.into(),
            Record {
                fetched: now,
                value: value.clone(),
            },
        );
        let mut bytes = serde_json::to_vec(&data).ok()?;
        while data.records.len() > MAX_RECORDS || bytes.len() as u64 > MAX_BYTES {
            let oldest = data
                .records
                .iter()
                .filter(|(name, _)| name.as_str() != key)
                .min_by_key(|(_, record)| record.fetched)
                .map(|(name, _)| name.clone())?;
            data.records.remove(&oldest);
            bytes = serde_json::to_vec(&data).ok()?;
        }
        let temp = self
            .root
            .join(format!("{}.{}.tmp", self.key, crate::config::random_hex(8)));
        let result = (|| {
            let mut file = private_file().create_new(true).open(&temp).ok()?;
            file.write_all(&bytes).ok()?;
            file.sync_all().ok()?;
            fs::rename(&temp, path).ok()?;
            if let Ok(mut snapshot) = self.snapshot.lock() {
                *snapshot = Some(data);
            }
            self.prune(now);
            Some(())
        })();
        if result.is_none() {
            let _ = fs::remove_file(temp);
        }
        result
    }

    fn prune(&self, now: i64) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        let mut files = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path == self.path() {
                    return None;
                }
                let stem = path.file_stem()?.to_str()?;
                if path.extension()?.to_str()? != "json"
                    || stem.len() != 64
                    || !stem.bytes().all(|ch| ch.is_ascii_hexdigit())
                {
                    return None;
                }
                let meta = fs::symlink_metadata(&path).ok()?;
                if !meta.is_file() {
                    return None;
                }
                let modified = meta
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_secs() as i64;
                Some((path, modified))
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|(_, time)| *time);
        // The snapshot just written is retained separately; keep at most 15 others.
        let excess = files.len().saturating_sub(15);
        for (i, (path, modified)) in files.into_iter().enumerate() {
            if i < excess || now.saturating_sub(modified) > 86_400 {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn private_file() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn private_dir(path: &Path) -> Option<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Some(()),
        Err(_) if fs::symlink_metadata(path).ok()?.is_dir() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).ok()?;
            }
            Some(())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_survive_new_instances_expire_and_isolate_accounts_and_credentials() {
        crate::config::tests::with_temp_config(|| {
            let make =
                |token, account| Index::for_account("https://school.test", token, account).unwrap();
            let index = make("private-token", "a");
            assert!(index.put(
                "course:1",
                &json!([{"id":1,"description":"Consigne","modules":[]}]),
                1000
            ));
            assert!(make("private-token", "a").get("course:1", 1001).is_some());
            assert!(make("private-token", "b").get("course:1", 1001).is_none());
            assert!(make("different-token", "a").get("course:1", 1001).is_none());
            assert!(index.get("course:1", 1000 + TTL).is_none());
            assert!(index.get("course:1", 999).is_none());
            assert!(!fs::read_to_string(index.path())
                .unwrap()
                .contains("private-token"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(index.path()).unwrap().permissions().mode() & 0o777,
                    0o600
                );
                assert_eq!(
                    fs::metadata(&index.root).unwrap().permissions().mode() & 0o777,
                    0o700
                );
            }
        });
    }

    #[test]
    fn capacity_is_bounded_even_when_all_writes_share_the_same_timestamp() {
        crate::config::tests::with_temp_config(|| {
            let now = crate::services::moodle::unix_now();
            let mut last = None;
            for id in 0..20 {
                let index =
                    Index::for_account("https://school.test", "token", &id.to_string()).unwrap();
                assert!(index.put("courses", &json!([]), now));
                last = Some(index);
            }
            let index = last.unwrap();
            assert!(index.get("courses", now).is_some());
            let count = fs::read_dir(&index.root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .count();
            assert_eq!(count, 16);
        });
    }

    #[test]
    fn malformed_snapshot_values_are_cache_misses_not_empty_course_proofs() {
        crate::config::tests::with_temp_config(|| {
            let index = Index::for_account("https://school.test", "token", "a").unwrap();
            assert!(index.put("course:1", &json!([{"id":1,"modules":"unexpected"}]), 1000));
            assert!(index.get("course:1", 1001).is_none());
        });
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_storage_is_not_read_or_overwritten() {
        crate::config::tests::with_temp_config(|| {
            let index = Index::for_account("https://school.test", "secret", "a").unwrap();
            let target = crate::config::config_path().with_extension("untouched");
            fs::write(&target, "unchanged").unwrap();
            fs::create_dir(&index.root).unwrap();
            std::os::unix::fs::symlink(&target, index.path()).unwrap();
            assert!(!index.put("courses", &json!([]), 1));
            assert!(index.get("courses", 1).is_none());
            assert_eq!(fs::read_to_string(target).unwrap(), "unchanged");
        });
    }
}
