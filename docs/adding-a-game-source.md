# Adding a new game/sim as a telemetry source

Using `sld-source-forza` as the template. Say you're adding iRacing (shared
memory) or ACC (also shared memory) or BeamNG (UDP, different schema than
Forza).

1. **New crate**: `crates/sld-sources/<name>/`, depending only on
   `sld-core` (plus whatever's needed to actually read the game's
   telemetry -- a UDP socket, a shared-memory crate, a named pipe, etc.).
   Add it to the root `Cargo.toml` workspace `members`.

2. **Implement `TelemetrySource`** (`sld_core::traits::TelemetrySource`):
   - `id(&self) -> &str` -- a stable short id, e.g. `"iracing"`.
   - `async fn run(&mut self, tx: TelemetryTx, shutdown: ShutdownSignal)` --
     loop reading from the game, and for each sample, build a
     `sld_core::telemetry::TelemetryFrame` and `tx.send(frame)`. `select!`
     against `shutdown.cancelled()` so the task exits promptly on shutdown
     (see `sld-sources/forza/src/source.rs` for the pattern with a UDP
     socket; a polling shared-memory source would `select!` a
     `tokio::time::interval` tick instead of a socket read).
   - Map whatever fields you can onto `TelemetryFrame`'s named fields
     (rpm, speed, gear, pedals, ...); put anything else in `extra` with a
     documented, `snake_case` key name (document the keys in a doc comment
     the way `sld-sources/forza/src/packet.rs` does).
   - A source erroring out doesn't take down the service -- `sld-service`
     logs the error and moves on -- so it's fine (even good) for `run` to
     return `Err` on an unrecoverable failure rather than trying to retry
     forever internally, *unless* the natural behavior is "game isn't
     running yet, keep waiting" (e.g. a shared-memory source should
     probably poll-and-retry rather than error out just because the game
     hasn't started).

3. **Config**: add an `Option<YourSourceConfig>` field to
   `sld_core::config::SourcesConfig`, with sane `#[serde(default = ...)]`
   values (see `ForzaSourceConfig` for the pattern), and document it in
   `config/default.toml`.

4. **Register it**: in `sld-service/src/registry.rs::build_sources`, add
   an `if let Some(cfg) = &cfg.sources.your_source { sources.push(...) }`
   block, and add the crate as a dependency of `sld-service`.

That's the whole surface. Nothing in `sld-core`, any existing source, or
any output needs to change -- the bus and `OutputDevice` side don't know or
care how many sources are publishing to them.

## If the game exposes richer data than `TelemetryFrame` has fields for

Prefer `extra` over adding a new named field to `TelemetryFrame` for
anything that's genuinely game-specific. Only promote something out of
`extra` into a real field once a second source would also populate it with
the same meaning and unit -- that's the signal it's actually common, not
Forza-specific dressed up as universal.
