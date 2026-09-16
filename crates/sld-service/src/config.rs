//! Thin file-loading wrapper around `sld_core::config::AppConfig`. The
//! schema itself lives in `sld-core` (see that crate for why); this module
//! only knows about TOML files, where to look for one, and defaulting.

use sld_core::config::AppConfig;
use std::path::{Path, PathBuf};

/// Overrides every other search location when set. Used by installers that
/// know exactly where they put the config file rather than relying on
/// convention (see `packaging/`).
const CONFIG_PATH_ENV: &str = "SLD_CONFIG_PATH";

/// Where a config file might be, checked in order. The first one that
/// actually exists on disk wins; if none do, `load_default_config` falls
/// back to built-in defaults. Every installer this project ships places a
/// config file at one of these -- see docs/installers.md.
pub fn candidate_config_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(p) = std::env::var(CONFIG_PATH_ENV) {
        candidates.push(PathBuf::from(p));
    }

    // Dev/tarball layout: run from the extracted archive (or repo root
    // during development), config/ alongside the binary.
    candidates.push(PathBuf::from("config/default.toml"));

    // Per-user config (XDG on Linux, the same convention macOS Flatpak/CLI
    // tools also tend to use). This is what the Flatpak's sandboxed
    // `--filesystem=xdg-config/logitech-sim-led:create` grant maps to.
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        });
    if let Some(base) = xdg_config_home {
        candidates.push(base.join("logitech-sim-led").join("default.toml"));
    }

    // System-wide locations the .deb and .pkg installers use.
    if cfg!(target_os = "linux") {
        candidates.push(PathBuf::from("/etc/logitech-sim-led/default.toml"));
    }
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(
            "/usr/local/etc/logitech-sim-led/default.toml",
        ));
    }

    candidates
}

/// Loads the first candidate path (see `candidate_config_paths`) that
/// exists on disk, or built-in defaults if none do.
pub fn load_default_config() -> anyhow::Result<AppConfig> {
    for path in candidate_config_paths() {
        if path.exists() {
            return load(&path);
        }
    }
    tracing::warn!(
        "no config file found (checked CWD-relative, XDG, and system paths -- see docs/installers.md); using built-in defaults"
    );
    Ok(AppConfig::default())
}

fn load(path: &Path) -> anyhow::Result<AppConfig> {
    let text = std::fs::read_to_string(path)?;
    let cfg: AppConfig = toml::from_str(&text)?;
    tracing::info!(path = %path.display(), "loaded config");
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    // std::env is process-global, so this test owns SLD_CONFIG_PATH for
    // its duration and cleans up after itself -- nothing else in this
    // crate reads or writes that variable.
    #[test]
    fn env_override_takes_priority_over_every_other_candidate() {
        // SAFETY: single-threaded within this test, and this variable
        // isn't touched by any other test in this crate.
        unsafe {
            std::env::set_var(CONFIG_PATH_ENV, "/tmp/explicit-override.toml");
        }
        let candidates = candidate_config_paths();
        unsafe {
            std::env::remove_var(CONFIG_PATH_ENV);
        }

        assert_eq!(candidates[0], PathBuf::from("/tmp/explicit-override.toml"));
    }

    #[test]
    fn cwd_relative_default_is_a_candidate_when_no_override_set() {
        // SAFETY: see above.
        unsafe {
            std::env::remove_var(CONFIG_PATH_ENV);
        }
        let candidates = candidate_config_paths();
        assert!(candidates.contains(&PathBuf::from("config/default.toml")));
    }
}
