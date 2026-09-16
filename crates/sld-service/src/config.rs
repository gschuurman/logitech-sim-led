//! Thin file-loading wrapper around `sld_core::config::AppConfig`. The
//! schema itself lives in `sld-core` (see that crate for why); this module
//! only knows about TOML files, where to look for one, and defaulting.

use sld_core::config::AppConfig;
use std::path::{Path, PathBuf};

/// Overrides every other search location when set. Used by installers that
/// know exactly where they put the config file rather than relying on
/// convention (see `packaging/`).
const CONFIG_PATH_ENV: &str = "SLD_CONFIG_PATH";

/// Per-user, always-writable config location: `%LOCALAPPDATA%` on Windows,
/// XDG_CONFIG_HOME (falling back to `~/.config`) on Linux/macOS -- the
/// same convention the Flatpak's sandboxed
/// `--filesystem=xdg-config/logitech-sim-led:create` grant maps to.
/// Nothing writes here until the first time settings are saved from the
/// dashboard's Settings panel (see `save`); until then it just isn't one
/// of the candidates that exists.
pub fn per_user_config_path() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        return std::env::var("LOCALAPPDATA").ok().map(|d| {
            PathBuf::from(d)
                .join("logitech-sim-led")
                .join("default.toml")
        });
    }
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .map(|base| base.join("logitech-sim-led").join("default.toml"))
}

/// Where a config file might be, checked in order. The first one that
/// actually exists on disk wins; if none do, `load_default_config` falls
/// back to built-in defaults. Every installer this project ships places a
/// config file at one of these -- see docs/installers.md.
///
/// The per-user path ranks above the CWD-relative/system ones
/// specifically so that settings saved from the dashboard (which always
/// write there -- see `save`) take priority over the vendor-shipped
/// default on the next load, without needing to touch it. It simply won't
/// exist -- and so won't win -- until the first save.
pub fn candidate_config_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(p) = std::env::var(CONFIG_PATH_ENV) {
        candidates.push(PathBuf::from(p));
    }

    if let Some(p) = per_user_config_path() {
        candidates.push(p);
    }

    // Dev/tarball layout: run from the extracted archive (or repo root
    // during development), config/ alongside the binary. Also where the
    // Windows MSI's bundled default.toml lands (read-only in practice --
    // Program Files isn't writable by a standard user token).
    candidates.push(PathBuf::from("config/default.toml"));

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

/// Persists `cfg` (the whole thing, not just whatever changed -- settings
/// not touched by the caller are carried through unchanged) to the
/// per-user writable location, creating its parent directory if needed.
/// Always writes there rather than back to wherever `cfg` was originally
/// loaded from, since that could be a read-only vendor-installed path
/// (e.g. the Windows MSI's `Program Files\...\config\default.toml`) --
/// same reasoning as `SLD_CONFIG_PATH` taking priority in
/// `candidate_config_paths` when set explicitly. Returns the path written
/// to.
pub fn save(cfg: &AppConfig) -> anyhow::Result<PathBuf> {
    let path = match std::env::var(CONFIG_PATH_ENV) {
        Ok(p) => PathBuf::from(p),
        Err(_) => per_user_config_path()
            .ok_or_else(|| anyhow::anyhow!("no writable per-user config location on this platform (neither LOCALAPPDATA nor XDG_CONFIG_HOME/HOME set)"))?,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(cfg)?;
    std::fs::write(&path, text)?;
    tracing::info!(path = %path.display(), "saved config");
    Ok(path)
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
