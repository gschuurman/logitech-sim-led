# Installers

Every tagged release (`v*.*.*`) publishes, alongside the plain
`.tar.gz`/`.zip` archives, a native installer per platform. Source for all
three lives under `packaging/`; the build steps are in
`.github/workflows/release.yml`.

**Status note**: these were written without access to a Windows or macOS
machine to test on (this project was built in a Linux-only environment) --
the underlying commands (`wix build`, `pkgbuild`, `dpkg-deb`) are correct
as documented, but the *first* tag that runs them for real is the actual
test. If a packaging job fails, it doesn't block the release -- the plain
archives still get published; see `publish-release`'s comment in
`release.yml`.

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

## Why native installers instead of just the archives

The plain `.tar.gz`/`.zip` archives (from the `build` job) are still
published and are the simplest option if you just want the binaries
without touching system package management. The installers exist for a
real "double-click to install, uninstall the normal way for your OS"
experience -- particularly the Windows MSI and Linux deb, which both give
proper OS-native uninstall; the macOS pkg is the one platform where that
isn't fully achievable (see above), so it ships its own uninstall script
instead.
