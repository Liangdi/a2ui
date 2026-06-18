//! Persisted user configuration for the gallery.
//!
//! Stored as TOML at `<config_dir>/a2ui/config.toml` (e.g.
//! `~/.config/a2ui/config.toml` on Linux). Currently holds the image-protocol
//! choice so a runtime switch (the `P` key) survives restarts. Missing or
//! unreadable file ⇒ defaults; writes never panic the app.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The persisted gallery configuration. Fields are `Option` so a minimal file
/// (or no file at all) round-trips cleanly and only set keys are serialized.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GalleryConfig {
    /// Image protocol preference, stored as its canonical name
    /// ([`a2ui_tui::components::image::ImageProtocol::as_str`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_protocol: Option<String>,
}

/// Resolve the config file path: `$XDG_CONFIG_HOME/a2ui/config.toml` (or
/// `~/.config/a2ui/config.toml` when `XDG_CONFIG_HOME` is unset). Returns
/// `None` only if no config/home directory can be determined.
pub fn config_path() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(base.join("a2ui").join("config.toml"))
}

/// Load the config, falling back to [`GalleryConfig::default`] when the file is
/// missing or fails to parse (a gallery must never refuse to start over a bad
/// config). Parse errors are logged to stderr.
pub fn load() -> GalleryConfig {
    let Some(path) = config_path() else {
        return GalleryConfig::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<GalleryConfig>(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Warning: ignoring unreadable config {}: {e}", path.display());
                GalleryConfig::default()
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => GalleryConfig::default(),
        Err(e) => {
            eprintln!("Warning: cannot read config {}: {e}", path.display());
            GalleryConfig::default()
        }
    }
}

/// Persist the config to disk, creating the directory tree as needed. Errors
/// are surfaced to the caller so the UI can show them (the gallery logs to
/// stderr and continues).
pub fn save(cfg: &GalleryConfig) -> io::Result<()> {
    let path = config_path().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "no config/home directory available")
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(cfg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `image_protocol` round-trips through TOML and only serializes when set.
    #[test]
    fn config_roundtrips_image_protocol() {
        let cfg = GalleryConfig {
            image_protocol: Some("halfblocks".to_string()),
        };
        let text = toml::to_string(&cfg).unwrap();
        assert!(text.contains("image_protocol = \"halfblocks\""));
        let back: GalleryConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.image_protocol.as_deref(), Some("halfblocks"));
    }

    /// An empty/default config serializes to nothing (no key emitted).
    #[test]
    fn default_config_omits_unset_keys() {
        let text = toml::to_string(&GalleryConfig::default()).unwrap();
        assert!(text.trim().is_empty(), "default config should be empty TOML");
    }

    /// An unknown / partial file still parses (extra keys ignored, missing
    /// `image_protocol` ⇒ `None`) — never break startup over a bad config.
    #[test]
    fn unknown_keys_and_missing_fields_are_tolerated() {
        let text = r#"some_future_key = 42
"#;
        let cfg: GalleryConfig = toml::from_str(text).unwrap();
        assert_eq!(cfg.image_protocol, None);
    }
}
