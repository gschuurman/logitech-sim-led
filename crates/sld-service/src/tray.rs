//! **Experimental scaffold, not wired into `main()` yet.**
//!
//! `tray-icon` (and the `tao` event loop it needs to pump menu/click
//! events) has to run on the main OS thread, which doesn't mesh directly
//! with `#[tokio::main]` owning that thread instead. Finishing this means
//! restructuring `main()` roughly as:
//!
//!   1. Build the tokio `Runtime` explicitly (not via the `#[tokio::main]`
//!      attribute) and spawn today's `main()` body onto it from a
//!      background thread.
//!   2. Run `tao::event_loop::EventLoop::run` on the main thread, and in
//!      its callback, handle `tray_icon::menu::MenuEvent` (e.g. forward a
//!      "Quit" click to `ShutdownHandle::shutdown()` via a channel/`Arc`
//!      shared with the background thread).
//!
//! The API surface below (`MenuItem::new`, `Menu::append`,
//! `TrayIconBuilder`) has not been verified against a pinned `tray-icon`
//! version -- check docs.rs for the version in Cargo.toml before enabling
//! the `tray` feature and building on this.
//!
//! Build requirement on Linux: `tray-icon`/`tao` pull in GTK bindings
//! (`gdk-sys`, `pango-sys`, ...) that need system dev packages, e.g. on
//! Debian/Ubuntu: `apt install libgtk-3-dev`. This is confirmed by trying
//! `cargo check -p sld-service --features tray` without them installed --
//! it fails at the `gdk-sys`/`pango-sys` build scripts with a `pkg-config`
//! error, not a Rust error. CI intentionally builds default features only
//! for this reason (see .github/workflows/ci.yml).

use tray_icon::menu::{Menu, MenuItem};
use tray_icon::TrayIconBuilder;

pub struct TrayScaffold {
    pub menu: Menu,
    pub quit_item: MenuItem,
}

impl TrayScaffold {
    pub fn build() -> anyhow::Result<Self> {
        let menu = Menu::new();
        let quit_item = MenuItem::new("Quit", true, None);
        menu.append(&quit_item)?;

        let _tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_tooltip("logitech-sim-led")
            .build()?;

        Ok(Self { menu, quit_item })
    }
}
