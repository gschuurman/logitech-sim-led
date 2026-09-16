//! Thin file-loading wrapper around `sld_core::config::AppConfig`. The
//! schema itself lives in `sld-core` (see that crate for why); this module
//! only knows about TOML files and defaulting.

use sld_core::config::AppConfig;
use std::path::Path;

pub fn load_or_default(path: impl AsRef<Path>) -> anyhow::Result<AppConfig> {
    let path = path.as_ref();
    if path.exists() {
        let text = std::fs::read_to_string(path)?;
        let cfg: AppConfig = toml::from_str(&text)?;
        Ok(cfg)
    } else {
        tracing::warn!(path = %path.display(), "config file not found, using built-in defaults (see config/default.toml)");
        Ok(AppConfig::default())
    }
}
