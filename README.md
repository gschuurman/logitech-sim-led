# logitech-sim-led

[![CI](https://github.com/gschuurman/logitech-sim-led/actions/workflows/ci.yml/badge.svg)](https://github.com/gschuurman/logitech-sim-led/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Cross-platform background service that ingests racing-game telemetry and
drives the RPM shift-light LEDs on a Logitech G-series wheel, with a
foundation laid for a second, auxiliary telemetry display (e.g. an ESP32).

Built game-agnostic and output-agnostic from the ground up: telemetry
sources (Forza today) and outputs (wheel LEDs, aux display) are independent
plugins wired together over a shared event bus. See
**[docs/architecture.md](docs/architecture.md)** for the full design and
**[docs/](docs/)** for protocol references and extension guides.

## Status

This is a fresh scaffold, not a finished product. The core architecture,
Forza Sled telemetry (RPM/idle/redline), and the G27/G29 LED protocol are
solid. Some pieces are explicitly foundation-only or unverified -- see the
table in [docs/architecture.md](docs/architecture.md#whats-genuinely-done-vs-foundation-only)
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
  sld-outputs/aux-display/       second-display output (foundation)
  sld-service/                  the daemon (binary: sld-service)
  sld-cli/                      dev/debug tooling (binary: sld-cli)
firmware/esp32-aux-display/    matching ESP32-side stub (PlatformIO)
config/default.toml            documented example config
docs/                          architecture + protocol docs
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
  builds `sld-service`/`sld-cli` release binaries for Linux
  (x86_64), Windows (x86_64), and macOS (x86_64 + aarch64), packages each
  with `config/`, `docs/`, `README.md`, and `LICENSE`, and publishes them to
  a GitHub Release.

## License

[MIT](LICENSE)
