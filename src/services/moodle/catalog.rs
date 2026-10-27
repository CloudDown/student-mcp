use serde_json::{json, Value};
use std::sync::OnceLock;

static CATALOG_JSON: &str = include_str!("catalog/functions.json");

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub name: String,
    pub description: String,
    pub write: bool,
    pub parameters: Value,
}

/// Bundled Moodle Web Service function catalog (from the open Moodle docs dump).
pub struct Catalog {
    entries: Vec<CatalogEntry>,
    release: Option<String>,
}

impl Catalog {
    pub fn load() -> &'static Catalog {
        static CATALOG: OnceLock<Catalog> = OnceLock::new();
        CATALOG.get_or_init(Self::parse)
    }

    fn parse() -> Catalog {
        let parsed: Value = serde_json::from_str(CATALOG_JSON).expect("valid bundled catalog");
        let release = parsed
            .get("moodle")
            .and_then(|moodle| moodle.get("release"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let entries = parsed
            .get("functions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let name = entry.get("name").and_then(Value::as_str)?.to_string();
                let description = entry
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let write = entry.get("type").and_then(Value::as_str) == Some("write");
                Some(CatalogEntry {
                    name,
                    description,
                    write,
                    parameters: entry.get("parameters").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        Catalog { entries, release }
    }

    pub fn get(&self, name: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Search by function name, then description. Name matches come first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<CatalogEntry> {
        let needle = query.trim().to_lowercase();
        let mut matched: Vec<(u8, &CatalogEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                if needle.is_empty() {
                    return Some((2, entry));
                }
                let name = entry.name.to_lowercase();
                if name == needle {
                    Some((0, entry))
                } else if name.contains(&needle) {
                    Some((1, entry))
                } else if entry.description.to_lowercase().contains(&needle) {
                    Some((2, entry))
                } else {
                    None
                }
            })
            .collect();
        matched.sort_by_key(|entry| entry.0);
        matched
            .into_iter()
            .take(limit.max(1))
            .map(|(_, entry)| entry.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn release(&self) -> Option<&str> {
        self.release.as_deref()
    }
}

/// JSON schema for tools that accept Web Service parameters by exact name.
pub fn ws_call_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "args": { "type": "object", "description": "Exact parameter names of the Web Service function" }
        },
        "required": ["name"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_with_hundreds_of_functions() {
        let catalog = Catalog::load();
        assert!(catalog.entries.len() > 700);
        assert!(catalog.release.is_some());
        let entry = catalog
            .get("core_enrol_get_users_courses")
            .expect("read function in catalog");
        assert!(!entry.write);
        assert_eq!(entry.parameters["userid"]["type"], "integer");
    }

    #[test]
    fn search_finds_and_limit_applies() {
        let catalog = Catalog::load();
        let results = catalog.search("enrol_get_users", 5);
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .any(|entry| entry.name == "core_enrol_get_users_courses"));
    }

    #[test]
    fn search_ranks_an_exact_function_name_first() {
        let catalog = Catalog::load();
        let results = catalog.search("core_enrol_get_users_courses", 5);
        assert_eq!(results[0].name, "core_enrol_get_users_courses");
    }

    #[test]
    fn shared_catalog_matches_fresh_parse() {
        let fresh = Catalog::parse();
        let cached = Catalog::load();
        assert!(std::ptr::eq(cached, Catalog::load()));
        assert_eq!(fresh.release(), cached.release());
        assert_eq!(fresh.len(), cached.len());
        for entry in &fresh.entries {
            let actual = cached.get(&entry.name).unwrap();
            assert_eq!(entry.description, actual.description);
            assert_eq!(entry.write, actual.write);
            assert_eq!(entry.parameters, actual.parameters);
        }
    }

    #[test]
    #[ignore = "manual performance measurement"]
    fn benchmark_catalog_loading() {
        use std::{hint::black_box, time::Instant};
        let start = Instant::now();
        for _ in 0..100 {
            black_box(Catalog::parse());
        }
        let fresh = start.elapsed();
        black_box(Catalog::load());
        let start = Instant::now();
        for _ in 0..100 {
            black_box(Catalog::load());
        }
        eprintln!(
            "100 catalog loads: fresh={fresh:?}, cached={:?}",
            start.elapsed()
        );
    }
}
