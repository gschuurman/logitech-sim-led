# Architecture

## Goals

1. Ingest telemetry from Forza Horizon 5/6 and Forza Motorsport today,
   without hard-coding Forza into the core of the app.
2. Drive the RPM shift-light LEDs on a Logitech G-series wheel from that
   telemetry.
3. **Modularity**: adding another game (iRacing, ACC, BeamNG, ...) or another
   output (a second display, an OBS overlay, ...) should be additive -- a
   new crate plus a config section -- never a change to existing sources,
   outputs, or the core.
4. Cross-platform (Windows/Linux/macOS), and specifically **not** dependent
   on Logitech G HUB being installed -- LED output talks to the wheel
   directly over USB HID.

## Shape: ports and adapters

The whole app is one idea, applied twice:

```
 TelemetrySource impls  --publish-->  [ bus ]  --subscribe-->  OutputDevice impls
  (one per game)                  broadcast channel              (one per output)
```

`sld-core` defines the two ports and the normalized data that crosses them:

- **`TelemetrySource`** (inbound port) -- ingests from one game and
  publishes normalized `TelemetryFrame`s onto the bus. One impl per game.
- **`OutputDevice`** (outbound port) -- subscribes to the bus and drives one
  physical/virtual output. One impl per output.
- **`TelemetryFrame`** -- the normalized model both sides agree on. Common
  fields (rpm, speed, gear, pedals, ...) are named; anything game-specific
  goes in an `extra: HashMap<String, f32>` bag so adding a field for one
  game never forces a change on every other source/output.
- **bus** -- a `tokio::sync::broadcast` channel. Every source publishes onto
  the same sender; every output gets its own receiver via `subscribe()`, so
  N sources fan in and M outputs fan out with no source knowing about any
  output and vice versa.

Neither side depends on the other. Nothing in `sld-source-forza` knows
Logitech wheels exist; nothing in `sld-output-logitech-hid` knows Forza
exists. The only place that knows about *concrete* sources and outputs is
`sld-service::registry`, which reads config and instantiates whichever ones
are enabled -- see docs/adding-a-game-source.md and
docs/adding-an-output.md.

This is a deliberately small, static form of a plugin system: today,
"plugins" are Rust crates registered in one `match`-like function in
`registry.rs`. It gets you the actual goal (adding a game or an output is
additive, not invasive) without the complexity of dynamic loading. If a
true runtime plugin system (drop a `.dll`/`.so`/WASM module in a folder,
have it show up without a rebuild) becomes a real requirement later, it
slots in *underneath* `registry.rs` -- the `TelemetrySource`/`OutputDevice`
trait boundary doesn't change, only how implementations of them get
discovered does.

## Crate layout

```
crates/
  sld-core/                    domain types + traits + bus + config schema
  sld-sources/
    forza/                     Forza Horizon 5/6 + Motorsport UDP source
    (future: iracing/, acc/, beamng/, ...)
  sld-outputs/
    logitech-hid/               RPM LED output over raw USB HID
    (future: a second display, an OBS overlay, ...)
  sld-service/                  the daemon: config, wiring, web UI, tray scaffold
  sld-cli/                      dev/debug tooling (HID list, packet capture)
docs/                          this file and friends
config/
  default.toml                 documented example config
```

Dependency direction is strictly inward: `sld-sources/*` and
`sld-outputs/*` depend on `sld-core`; `sld-service` depends on all of them
to wire things together; nothing depends on `sld-service`.

## Data flow (Forza -> LEDs, concretely)

1. Forza's in-game "Data Out" setting sends UDP packets to a configured
   address/port.
2. `ForzaSource::run` (`sld-sources/forza/src/source.rs`) receives them,
   hands each payload to `packet::parse`, and -- if the game reports a
   session as running -- converts the result to a `TelemetryFrame` and
   publishes it on the bus.
3. `LogitechLedOutput::run` (`sld-outputs/logitech-hid/src/device.rs`)
   receives frames independently, evaluates `ShiftLightCurve::evaluate`
   (pure function of rpm/idle/max -> which of 5 LEDs should be lit, and
   whether to blink) at a fixed ~30 Hz tick, and hands the latest desired
   state to a dedicated OS thread that owns the blocking HID device and
   writes the actual report.

A second output (a display, an overlay, ...) would subscribe to the same
bus independently, at whatever rate it needs -- see
docs/adding-an-output.md.

A slow or misbehaving output can't stall another output or a source: each
has its own `broadcast::Receiver`, and a receiver that falls behind just
skips ahead (`RecvError::Lagged`) instead of blocking the sender.

## Why this stack

Chosen with the user, given the alternatives were a) a C# app built on
Logitech's official SDK (simplest, but the LED driver is Windows+G-HUB
only) and b) Python (fastest to prototype, weaker fit for a long-running
background service):

- **Rust core, raw USB HID via `hidapi`** for the LED driver, instead of
  Logitech's official Gaming SDK. This is what makes "cross-platform"
  literal -- `hidapi` has working backends on Windows, Linux, and macOS, and
  none of them require G HUB (or any Logitech software) to be installed.
  The tradeoff, made explicit rather than hidden, is that the HID report
  format is community-reverse-engineered rather than officially documented
  -- see docs/led-hid-protocol.md for exactly what's verified vs. not, per
  wheel model.
- **`tokio`** async runtime for the service, since it's fundamentally I/O
  fan-in/fan-out (many UDP packets in, several device writes out) with
  cooperative shutdown -- a natural fit for `select!`/channels.
- **Headless service + local web UI, with a tray-icon scaffold** as the app
  shape: this is meant to run in the background while you play, not be a
  foreground app you interact with. The web UI (`sld-service/src/web.rs`,
  on by default) gives a live status view without needing native GUI work;
  the tray icon (`sld-service/src/tray.rs`) is scaffolded but not wired in
  yet because it needs a main-thread native event loop that doesn't mesh
  trivially with `#[tokio::main]` -- see that file's doc comment for
  exactly what's left.

## What's genuinely done vs. foundation-only

Being direct about confidence levels -- the LED path is verified against
official documentation; the HID output side is still community
reverse-engineering:

| Piece | Status |
|---|---|
| Core traits, bus, shutdown, config | Solid -- this is the actual architecture, not a stub. |
| Forza Sled parsing (RPM, idle, redline) | Verified against Forza's own Data Out documentation for both Horizon 6 and Motorsport; byte-identical across titles. This is all the LED output needs. |
| Forza Dash parsing (speed, gear, pedals, fuel, lap, tire wear/temps) | Verified against the same official docs, including the real structural difference between Horizon's and Motorsport's layouts (see docs/telemetry-protocol-forza.md) -- covered by unit tests in `sld-sources/forza/src/packet.rs`. `Gear`'s Reverse/Neutral convention specifically is *not* defined by Forza's docs, so it's exposed as a raw value. |
| G27/G29/G923 LED HID protocol | Verified against the actively-maintained `berarma/new-lg4ff` Linux driver source, including the G923 PlayStation-mode-switch handshake -- see docs/led-hid-protocol.md. G923 Xbox-mode is an inference, not driver-confirmed. |
| G920/Driving Force GT LED HID protocol | Not implemented -- the reference driver doesn't register LED support for these either, so rather than guess, the service now reports "found this wheel, but its LED protocol isn't implemented" instead of silently doing nothing. |
| Web UI | Working minimal live-telemetry page. |
| Tray icon | Scaffolded, not wired into `main()` -- see tray.rs. |

A second output (e.g. an auxiliary/second display) isn't built yet -- see
docs/adding-an-output.md for the shape it would take when it's needed.

## Extending

- New game -> docs/adding-a-game-source.md
- New output -> docs/adding-an-output.md
- Forza wire format details -> docs/telemetry-protocol-forza.md
- LED HID protocol details -> docs/led-hid-protocol.md

## Near-term roadmap (not built yet)

- Verify against real hardware: the G923 Xbox-mode inference, and whether
  G920/DFGT support can be added at all (needs a packet capture, since the
  reference driver has none).
- Wire the tray icon into `main()` (see tray.rs for the exact restructuring
  needed).
- Add a config section + editor in the web UI instead of hand-editing TOML.
- A second real game source (proves the modularity claim beyond one
  example) -- iRacing and ACC both expose shared-memory telemetry, which is
  a different `TelemetrySource` shape (polling shared memory instead of a
  UDP socket) and a good test that the trait boundary holds up.
- A second output, once there's a concrete need for one -- see
  docs/adding-an-output.md for what that involves.
