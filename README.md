# logitech-sim-led

[![CI](https://github.com/gschuurman/logitech-sim-led/actions/workflows/ci.yml/badge.svg)](https://github.com/gschuurman/logitech-sim-led/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Cross-platform background service that ingests racing-game telemetry and
drives the RPM shift-light LEDs on a Logitech G-series wheel.

Built game-agnostic and output-agnostic from the ground up: telemetry
sources (Forza today) and outputs (wheel LEDs today) are independent
plugins wired together over a shared event bus, so adding another game or
another output (e.g. a second display) is additive. See
**[docs/user-guide.md](docs/user-guide.md)** for what it does and how to
use it, **[docs/architecture.md](docs/architecture.md)** for the full
design, and **[docs/](docs/)** for protocol references and extension
guides.

## Status

This is a fresh scaffold, not a finished product. The core architecture and
Forza telemetry parsing (both Horizon and Motorsport's Dash formats) are
verified against Forza's own Data Out documentation -- see
[docs/telemetry-protocol-forza.md](docs/telemetry-protocol-forza.md). The
LED protocol is verified against the `berarma/new-lg4ff` Linux driver
source for G27/G29/G923 (including the G923 PlayStation-mode switch); G920
and Driving Force GT have no confirmed LED command and aren't supported --
see the table in
[docs/architecture.md](docs/architecture.md#whats-genuinely-done-vs-foundation-only)
before assuming something works out of the box.

The workspace builds clean and passes `cargo test`/`clippy -D warnings`/
`fmt --check` (enforced by [CI](.github/workflows/ci.yml) on Linux, Windows,
and macOS) with default features. The `tray` feature is excluded from CI
because it needs system GTK dev packages on Linux -- see
[`crates/sld-service/src/tray.rs`](crates/sld-service/src/tray.rs).

## Layout

```
crates/
  sld-core/                    shared traits, telemetry model, bus, config
  sld-sources/forza/            Forza Horizon 5/6 + Motorsport UDP source
  sld-outputs/logitech-hid/      wheel RPM LED output (raw USB HID)
  sld-service/                  the daemon (binary: sld-service)
  sld-cli/                      dev/debug tooling (binary: sld-cli)
config/default.toml            documented example config
docs/                          user guide + architecture + protocol docs
```

## Building

Requires a recent Rust toolchain (1.75+ recommended, for `let else`).

```
cargo build --workspace
```

## Running

1. Copy/edit `config/default.toml` -- at minimum, decide which
   `sources.*` and `outputs.*` sections to enable.
2. Point Forza's Settings > HUD and Gameplay > Data Out at this machine's
   IP and the port in `sources.forza.bind_addr` (default `5300`).
3. Run the service:
   ```
   cargo run -p sld-service
   ```
4. Open <http://127.0.0.1:5301> for a minimal live-telemetry page (the
   `web` feature, on by default).

## Dev tools

```
cargo run -p sld-cli -- list-hid-devices        # find your wheel's vendor/product id
cargo run -p sld-cli -- capture-forza --count 20 # verify/calibrate Forza packet offsets
```

## CI / releases

- **CI** ([.github/workflows/ci.yml](.github/workflows/ci.yml)): on every
  push/PR to `main`, runs `cargo fmt --check`, `cargo clippy -D warnings`,
  and `cargo build`/`cargo test` across Linux, Windows, and macOS.
- **Release** ([.github/workflows/release.yml](.github/workflows/release.yml)):
  pushing a tag matching `v*.*.*` (e.g. `git tag v0.1.0 && git push origin v0.1.0`)
  builds plain `.tar.gz`/`.zip` archives for Linux (x86_64), Windows
  (x86_64), and macOS (x86_64 + aarch64), **and** native installers --
  a Windows `.msi`, macOS `.pkg` (per architecture), a Linux `.deb`, and a
  `.flatpak` for SteamOS (and other Flatpak-capable Linux desktops) --
  and publishes all of it to a GitHub Release. See
  [docs/installers.md](docs/installers.md) for what each installer does
  and how to uninstall.

## License

[MIT](LICENSE)
