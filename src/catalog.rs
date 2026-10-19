use serde_json::{json, Value};

static CATALOG_JSON: &str = include_str!("../catalog/functions.json");

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub name: String,
    pub description: String,
    pub write: bool,
}

/// Bundled Moodle Web Service function catalog (from the open Moodle docs dump).
pub struct Catalog {
    entries: Vec<CatalogEntry>,
    release: Option<String>,
}

impl Catalog {
    pub fn load() -> Catalog {
        let parsed: Value = serde_json::from_str(CATALOG_JSON).unwrap_or(Value::Null);
        let release = parsed
            .get("moodle")
            .and_then(|moodle| moodle.get("release"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let entries = parsed
            .get("functions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
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
                })
            })
            .collect();
        Catalog { entries, release }
    }

    pub fn get(&self, name: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Search by function name / description substring.
    pub fn search(&self, query: &str, limit: usize) -> Vec<CatalogEntry> {
        let needle = query.trim().to_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                needle.is_empty()
                    || entry.name.to_lowercase().contains(&needle)
                    || entry.description.to_lowercase().contains(&needle)
            })
            .take(limit.max(1))
            .cloned()
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
}
