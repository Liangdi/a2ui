//! Sample loader — reads A2UI sample JSON files from a directory.

use std::fs;
use std::path::Path;

use crate::core::message_processor::MessageProcessor;
use crate::core::protocol::server_to_client::A2uiMessage;

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
