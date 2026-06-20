//! Sample loader — reads A2UI sample JSON files.
//!
//! By default the specification tree is embedded into the binary at compile
//! time ([`SPEC_DIR`] + [`load_samples`]), so the gallery is fully
//! self-contained and distributable. The legacy filesystem reader
//! [`load_samples_from_dir`] is retained for the `A2UI_SPEC_DIR` dev override.

use std::fs;
use std::path::Path;

use include_dir::{Dir, include_dir};

use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::protocol::server_to_client::A2uiMessage;

/// The full A2UI specification tree, embedded at compile time.
///
/// Makes the binary self-contained: no on-disk spec directory is required at
/// runtime. Paths inside are relative to the spec root, e.g.
/// `v1_0/catalogs/minimal/examples`.
pub static SPEC_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/a2ui/specification");

/// A loaded sample with metadata and parsed messages.
pub struct Sample {
    /// Display name (from the sample JSON).
    pub name: String,
    /// Description (from the sample JSON).
    pub description: String,
    /// File path for display purposes.
    pub file_path: String,
    /// Parsed A2UI messages.
    pub messages: Vec<A2uiMessage>,
}

/// Load all `.json` sample files from the given directory.
///
/// Files are sorted by filename (they are numbered `1_*.json`, `2_*.json`, etc.).
/// Files that fail to parse are skipped silently.
pub fn load_samples_from_dir(dir: &str) -> Vec<Sample> {
    let path = Path::new(dir);
    if !path.is_dir() {
        return Vec::new();
    }

    let dir_entries = match fs::read_dir(path) {
        Ok(de) => de,
        Err(e) => {
            eprintln!("Warning: cannot read sample directory {:?}: {}", dir, e);
            return Vec::new();
        }
    };

    let mut entries: Vec<String> = dir_entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.ends_with(".json") {
                Some(file_name)
            } else {
                None
            }
        })
        .collect();

    // Sort by filename — the numbering prefix ensures correct ordering.
    entries.sort();

    let mut samples = Vec::new();

    for file_name in &entries {
        let full_path = path.join(file_name);
        let content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: cannot read {:?}: {}", full_path, e);
                continue;
            }
        };

        match MessageProcessor::load_sample(&content) {
            Ok((name, description, messages)) => {
                samples.push(Sample {
                    name,
                    description,
                    file_path: file_name.clone(),
                    messages,
                });
            }
            Err(e) => {
                eprintln!("Warning: failed to parse sample {:?}: {}", full_path, e);
            }
        }
    }

    samples
}

/// Load all `.json` samples from an embedded subdirectory of [`SPEC_DIR`].
///
/// `subpath` is relative to the spec root, e.g.
/// `"v1_0/catalogs/minimal/examples"`. Direct children only (not recursive).
/// Files are sorted by filename; files that fail to parse are skipped silently.
pub fn load_samples(subpath: &str) -> Vec<Sample> {
    let dir = match SPEC_DIR.get_dir(subpath) {
        Some(d) => d,
        None => {
            eprintln!("Warning: embedded sample directory not found: {subpath:?}");
            return Vec::new();
        }
    };

    let mut files: Vec<&include_dir::File> = dir.files().collect();
    files.sort_by_key(|f| f.path().to_string_lossy().to_string());
    files.retain(|f| f.path().extension().is_some_and(|ext| ext == "json"));

    let mut samples = Vec::new();
    for file in files {
        let file_name = file
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let content = match std::str::from_utf8(file.contents()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: sample {file_name:?} is not valid UTF-8: {e}");
                continue;
            }
        };
        match MessageProcessor::load_sample(content) {
            Ok((name, description, messages)) => samples.push(Sample {
                name,
                description,
                file_path: file_name,
                messages,
            }),
            Err(e) => {
                eprintln!("Warning: failed to parse sample {file_name:?}: {e}");
            }
        }
    }
    samples
}

// ---------------------------------------------------------------------------
// Scenario loader — auto-detects the bare-array and wrapped-sample formats
// ---------------------------------------------------------------------------

/// Parse a single scenario file, auto-detecting its format.
///
/// Two top-level shapes are accepted:
/// - **Bare array** — a JSON array of A2UI messages, as used by the ad-hoc
///   `a2ui-json/*.json` scenario files (e.g. `[{createSurface},
///   {updateComponents}]`). The sample `name` is derived from the file stem and
///   `description` reports the message count.
/// - **Wrapped object** — `{name, description, messages}` as used by the embedded
///   spec samples; delegated to [`MessageProcessor::load_sample`].
///
/// Returns `None` on any parse failure so the caller can log and skip the file.
pub fn parse_scenario(file_name: &str, content: &str) -> Option<Sample> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;

    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name)
        .to_string();

    let (name, description, messages) = match value {
        serde_json::Value::Array(arr) => {
            // Bare message array: keep only the elements that deserialize to a
            // known A2uiMessage (mirrors the graceful filter in load_sample).
            let messages: Vec<A2uiMessage> = arr
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect();
            let count = messages.len();
            (stem, format!("{count} messages"), messages)
        }
        serde_json::Value::Object(_) => MessageProcessor::load_sample(content).ok()?,
        // A bare scalar (string/number/…) is neither shape.
        _ => return None,
    };

    Some(Sample {
        name,
        description,
        file_path: file_name.to_string(),
        messages,
    })
}

/// Load all `.json` scenario files from a directory, auto-detecting each file's
/// format via [`parse_scenario`].
///
/// Mirrors [`load_samples_from_dir`] (sorted by filename, log + skip failures)
/// but additionally accepts the bare-array format used by `a2ui-json/`. Returns
/// an empty `Vec` if `dir` is missing or unreadable.
pub fn load_scenarios(dir: &str) -> Vec<Sample> {
    let path = Path::new(dir);
    if !path.is_dir() {
        return Vec::new();
    }

    let dir_entries = match fs::read_dir(path) {
        Ok(de) => de,
        Err(e) => {
            eprintln!("Warning: cannot read scenario directory {:?}: {}", dir, e);
            return Vec::new();
        }
    };

    let mut entries: Vec<String> = dir_entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.ends_with(".json") {
                Some(file_name)
            } else {
                None
            }
        })
        .collect();

    // Sort by filename so numbered scenarios (1.json, 2.json, …) stay ordered.
    entries.sort();

    let mut samples = Vec::new();
    for file_name in &entries {
        let full_path = path.join(file_name);
        let content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: cannot read {:?}: {}", full_path, e);
                continue;
            }
        };

        match parse_scenario(file_name, &content) {
            Some(sample) => samples.push(sample),
            None => eprintln!("Warning: failed to parse scenario {:?}", full_path),
        }
    }

    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bare-array scenario (the `a2ui-json/*.json` shape): name comes from the
    /// file stem, description reports the message count.
    #[test]
    fn parse_bare_array_scenario() {
        let content = r#"[
            {"version":"v1.0","createSurface":{"surfaceId":"s","catalogId":"c"}},
            {"version":"v1.0","updateComponents":{"surfaceId":"s","components":[
                {"id":"root","component":"Text","text":"Hi"}
            ]}}
        ]"#;
        let sample = parse_scenario("1.json", content).expect("bare array should parse");
        assert_eq!(sample.name, "1", "name derived from file stem");
        assert_eq!(sample.file_path, "1.json");
        assert_eq!(sample.messages.len(), 2);
        assert!(
            sample.description.contains('2'),
            "description reports the message count: {}",
            sample.description
        );
    }

    /// Wrapped-sample object still round-trips through `load_sample`.
    #[test]
    fn parse_wrapped_sample_still_works() {
        let content = r#"{
            "name": "Demo",
            "description": "wrapped",
            "messages": [
                {"version":"v1.0","createSurface":{"surfaceId":"s","catalogId":"c"}}
            ]
        }"#;
        let sample = parse_scenario("demo.json", content).expect("wrapped sample should parse");
        assert_eq!(sample.name, "Demo");
        assert_eq!(sample.description, "wrapped");
        assert_eq!(sample.messages.len(), 1);
    }

    /// Non-object/array JSON and malformed input both yield `None`.
    #[test]
    fn parse_invalid_returns_none() {
        assert!(parse_scenario("bad.json", "not json").is_none());
        assert!(parse_scenario("scalar.json", "\"just a string\"").is_none());
    }
}
