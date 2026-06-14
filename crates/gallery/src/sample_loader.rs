//! Sample loader — reads A2UI sample JSON files.
//!
//! By default the specification tree is embedded into the binary at compile
//! time ([`SPEC_DIR`] + [`load_samples`]), so the gallery is fully
//! self-contained and distributable. The legacy filesystem reader
//! [`load_samples_from_dir`] is retained for the `A2UI_SPEC_DIR` dev override.

use std::fs;
use std::path::Path;

use include_dir::{include_dir, Dir};

use a2ui_core::message_processor::MessageProcessor;
use a2ui_core::protocol::server_to_client::A2uiMessage;

/// The full A2UI specification tree, embedded at compile time.
///
/// Makes the binary self-contained: no on-disk spec directory is required at
/// runtime. Paths inside are relative to the spec root, e.g.
/// `v1_0/catalogs/minimal/examples`.
pub static SPEC_DIR: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/a2ui/specification");

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
                eprintln!(
                    "Warning: failed to parse sample {:?}: {}",
                    full_path, e
                );
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
