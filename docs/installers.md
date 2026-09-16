# Installers

Every tagged release (`v*.*.*`) publishes, alongside the plain
`.tar.gz`/`.zip` archives, a native installer per platform. Source for all
four lives under `packaging/`; the build steps are in
`.github/workflows/release.yml`.

**Status note**: these were written without access to a Windows or macOS
machine to test on (this project was built in a Linux-only environment).
The Windows MSI and macOS pkg/Linux deb/SteamOS flatpak jobs were
validated for real against `v0.1.1` -- one real bug turned up (WiX
`<Package>` attributes needed PascalCase, not the camelCase originally
written) and was fixed the same day; see the `v0.1.1` release notes and
commit history for the exact failure. If a packaging job fails on a future
release, it doesn't block that release -- the plain archives still get
published; see `publish-release`'s comment in `release.yml`.

**Where each installer's config file ends up, and how `sld-service` finds
it**: `sld-service` checks, in order: the `SLD_CONFIG_PATH` environment
variable if set, then `config/default.toml` relative to the current
directory (the tarball/dev layout), then
`$XDG_CONFIG_HOME/logitech-sim-led/default.toml` (falling back to
`~/.config/...` -- this is what the Flatpak's sandboxed config grant
maps to), then the OS system location the `.deb`/`.pkg` installers use.
The first one that exists wins; see `candidate_config_paths` in
`crates/sld-service/src/config.rs`.

## Windows: `.msi`

Built with [WiX v5](https://wixtoolset.org/) from
`packaging/windows/product.wxs`.

- **Installs to**: `C:\Program Files\logitech-sim-led\` (`sld-service.exe`,
  `sld-cli.exe`, `README.md`, `LICENSE`, `config\default.toml`).
- **Start Menu**: a "logitech-sim-led" shortcut that runs `sld-service.exe`.
- **Uninstall**: native -- Settings > Apps > "logitech-sim-led" > Uninstall
  (or Control Panel > Programs and Features). This is the actual point of
  using an MSI: Windows tracks everything it installed and removes it
  cleanly, no separate script needed.
- **Not included yet**: adding the install folder to `PATH` (so `sld-cli`
  works from any terminal). Needs the WiX Util extension
  (`util:Environment`), left out of this first cut to keep it to core WiX
  only -- run `sld-cli` via its full path in
  `C:\Program Files\logitech-sim-led\` until that's added, or add it to
  your own `PATH` manually.

## macOS: `.pkg`

Built with `pkgbuild` (no extra tooling needed -- it ships with Xcode
command line tools, which macOS runners already have) from
`packaging/macos/`. One `.pkg` per architecture
(`aarch64-apple-darwin`/`x86_64-apple-darwin` -- Apple Silicon and Intel).

- **Installs to**: `/usr/local/bin/` (`sld-service`, `sld-cli`),
  `/usr/local/etc/logitech-sim-led/` (`default.toml`), and
  `/usr/local/share/logitech-sim-led/uninstall.sh`.
- **Uninstall**: macOS `.pkg` installers have no OS-level "Add/Remove
  Programs" equivalent for command-line tools like this (that's an Apple
  platform limitation, not something cut short here) -- run:
  ```
  sudo /usr/local/share/logitech-sim-led/uninstall.sh
  ```
  which removes the binaries and config and forgets the package receipt
  (`pkgutil --forget`).

## Linux: `.deb`

Built with `dpkg-deb` from a tree assembled in
`.github/workflows/release.yml` using `packaging/linux/control.template`.
`amd64` only for now.

- **Install**: `sudo dpkg -i logitech-sim-led-*.deb` (or `sudo apt install
  ./logitech-sim-led-*.deb` to also resolve the `libudev1` dependency
  automatically if it's somehow missing).
- **Installs to**: `/usr/bin/` (`sld-service`, `sld-cli`),
  `/etc/logitech-sim-led/default.toml`,
  `/usr/share/doc/logitech-sim-led/` (README/LICENSE).
- **Uninstall**: native -- `sudo apt remove logitech-sim-led` or
  `sudo dpkg -r logitech-sim-led`. Same mechanism as any other
  apt-installed package; nothing project-specific.

## SteamOS (and other Linux desktops): `.flatpak`

Built with `flatpak-builder` from `packaging/flatpak/`. SteamOS's root
filesystem is read-only and dm-verity-protected, and gets reset on every
OS update -- the `.deb` above genuinely doesn't apply there, even though
it's also Linux, because anything written into `/usr` or `/etc` outside a
Flatpak/`/home` gets wiped on the next update. Flatpak (installing under
`/var/lib/flatpak` or `~/.var/app`, both on the persistent partition) is
Valve's own recommended path for third-party SteamOS software, and it's
what Discover (the SteamOS software center) natively installs.

Unlike the other three installers, this one builds the Rust binaries
genuinely from source *inside* the Flatpak sandbox (using the
`org.freedesktop.Sdk.Extension.rust-stable` SDK extension), rather than
copying in binaries built elsewhere -- that keeps them linked against the
same glibc they'll actually run against, which is the idiomatic way to do
this and avoids any ABI-mismatch risk.

- **Install**: double-click the `.flatpak` file in Discover or a file
  manager, or `flatpak install --user logitech-sim-led-*.flatpak`. Not
  published to Flathub (that requires their own review process, out of
  scope here) -- it's an unsigned, self-distributed bundle, which
  `flatpak install` will let through with a warning rather than blocking.
- **Sandbox permissions requested**: `--share=network` (UDP telemetry in,
  local web UI) and `--device=all` (raw HID access to the wheel -- Flatpak
  has no finer-grained portal for hidraw devices specifically, so this is
  the coarsest permission Flatpak offers, requested because there's no
  narrower option).
- **Config**: lives at `~/.var/app/io.github.gschuurman.logitech_sim_led/config/logitech-sim-led/default.toml`
  inside the sandbox (mapped from the host's
  `~/.config/logitech-sim-led/default.toml` via the app's
  `--filesystem=xdg-config/logitech-sim-led:create` grant) -- create that
  file yourself if you want to override the bundled defaults.
- **Uninstall**: native -- `flatpak uninstall io.github.gschuurman.logitech_sim_led`,
  or remove it from Discover's "Installed" tab.
- **Running it**: launch from Discover/your app menu (opens a terminal
  showing live logs, since it's a background service rather than a
  windowed app), or from a Desktop Mode terminal:
  `flatpak run io.github.gschuurman.logitech_sim_led`. It keeps running in
  the background across a switch to Gaming Mode -- that's a different
  Plasma session/compositor, not a logout -- so you can start it once from
  Desktop Mode and then launch your game normally.

## Why native installers instead of just the archives

The plain `.tar.gz`/`.zip` archives (from the `build` job) are still
published and are the simplest option if you just want the binaries
without touching system package management. The installers exist for a
real "double-click to install, uninstall the normal way for your OS"
experience -- particularly the Windows MSI and Linux deb, which both give
proper OS-native uninstall; the macOS pkg is the one platform where that
isn't fully achievable (see above), so it ships its own uninstall script
instead.
