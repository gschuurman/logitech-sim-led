//! "Start with Windows" toggle, backed by the per-user Run registry key
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) -- no admin
//! rights needed, unlike a Startup-folder shortcut created by an
//! installer, and it's reversible from the tray menu at any time.
//!
//! Linux/macOS equivalents (an XDG `~/.config/autostart/*.desktop` file,
//! and a `~/Library/LaunchAgents/*.plist` respectively) aren't implemented
//! yet -- `is_supported()` gates the tray menu item off on those platforms
//! rather than showing a control that silently does nothing.

#[cfg(windows)]
mod imp {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "logitech-sim-led";

    fn open_run_key(write: bool) -> anyhow::Result<RegKey> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let access = if write {
            KEY_READ | KEY_WRITE
        } else {
            KEY_READ
        };
        Ok(hkcu.open_subkey_with_flags(RUN_KEY, access)?)
    }

    pub fn is_enabled() -> bool {
        open_run_key(false)
            .and_then(|k| Ok(k.get_value::<String, _>(VALUE_NAME)?))
            .is_ok()
    }

    pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
        let key = open_run_key(true)?;
        if enabled {
            let exe = std::env::current_exe()?;
            // Quote the path -- Program Files contains a space.
            key.set_value(VALUE_NAME, &format!("\"{}\"", exe.display()))?;
        } else {
            match key.delete_value(VALUE_NAME) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub fn is_supported() -> bool {
        true
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }

    pub fn set_enabled(_enabled: bool) -> anyhow::Result<()> {
        anyhow::bail!("autostart isn't implemented on this platform yet")
    }

    pub fn is_supported() -> bool {
        false
    }
}

pub use imp::{is_enabled, is_supported, set_enabled};
