# Adding a new output device

Using `sld-output-logitech-hid` and `sld-output-aux-display` as templates.
Examples of things that fit this shape: a second wheel's LEDs, an OBS
overlay (WebSocket/browser-source), logging telemetry to disk/CSV, RGB
keyboard/mouse lighting, a haptic buzzer on shift.

1. **New crate**: `crates/sld-outputs/<name>/`, depending on `sld-core`
   plus whatever talks to the actual device (a HID/serial/network library,
   etc.). Add it to the root `Cargo.toml` workspace `members`.

2. **Implement `OutputDevice`** (`sld_core::traits::OutputDevice`):
   - `id(&self) -> &str` -- a stable short id, e.g. `"logitech_led"`.
   - `async fn run(&mut self, rx: TelemetryRx, shutdown: ShutdownSignal)` --
     loop on `rx.recv()` (handling `RecvError::Lagged` by just continuing --
     it means you fell behind, not that anything broke) and drive your
     device. `select!` against `shutdown.cancelled()`.
   - If the device does **blocking** I/O (most HID/serial libraries do),
     don't block the async task directly -- follow the pattern in
     `sld-output-logitech-hid/src/device.rs`: hand the *latest* desired
     state to a dedicated OS thread over a `sync_channel(1)`, and let that
     thread own the blocking device handle. This keeps a slow/blocking
     device from stalling the tokio runtime, and naturally coalesces
     updates (the thread always acts on the newest state, never a queue of
     stale ones).
   - If the device only needs occasional/rate-limited updates (like the aux
     display, which doesn't need every frame at the source's native rate),
     down-sample with a `tokio::time::interval` the way
     `sld-output-aux-display/src/device.rs` does, rather than acting on
     every single bus message.

3. **Config**: add an `Option<YourOutputConfig>` field to
   `sld_core::config::OutputsConfig` (see `LogitechLedConfig` for the
   pattern), and document it in `config/default.toml`.

4. **Register it**: in `sld-service/src/registry.rs::build_outputs`, add an
   `if let Some(cfg) = &cfg.outputs.your_output { if cfg.enabled { outputs.push(...) } }`
   block, and add the crate as a dependency of `sld-service`.

Same as sources: nothing in `sld-core`, any existing output, or any source
needs to change. Every output gets its own `broadcast::Receiver`, so a
slow or crashing output can't affect any other output or any source.
