# Forza "Data Out" telemetry protocol

## Enabling it in-game

Settings > HUD and Gameplay > Data Out:

- Data Out: On
- IP Address: the machine running `sld-service`
- Port: must match `sources.forza.bind_addr` in `config/default.toml`
  (default `5300`)

This applies to Forza Horizon 5, (reportedly) Forza Horizon 6, and Forza
Motorsport -- they share the same "Data Out" feature and packet lineage.

## Packet formats

Three formats exist, distinguished by size. `sld-source-forza` reads
whichever fields are present based on the packet's length:

| Format | Size | Contains |
|---|---|---|
| Sled | 232 bytes | RPM (current/idle/max), velocity, suspension/tire physics, car id. **No** speed/gear/pedals/fuel. |
| Dash / Car Dash | 311 bytes | Everything in Sled, plus speed, power/torque, tire temps, fuel, lap/race info, pedal inputs, gear, steering. |
| Race | larger, includes multi-car data | Not parsed by this crate yet. |

Set the game to send Dash (or Car Dash) format to get gear/pedals/fuel --
Sled alone is enough for the LED shift-light (it only needs RPM/idle/max)
but not much else.

## Byte layout implemented in `packet.rs`

All fields little-endian. This layout has been consistent across Forza
titles' Data Out feature for years and is the same one shared across the
open-source Forza-telemetry community (multiple independent
implementations agree on it):

```
Sled (0..232):
  0   i32   IsRaceOn
  4   u32   TimestampMS
  8   f32   EngineMaxRpm
  12  f32   EngineIdleRpm
  16  f32   CurrentEngineRpm
  20  f32   AccelerationX/Y/Z (3x f32, not currently mapped to TelemetryFrame)
  32  f32   VelocityX
  36  f32   VelocityY
  40  f32   VelocityZ
  44..212   angular velocity, yaw/pitch/roll, per-wheel suspension/tire/slip
            physics (not currently mapped -- add to `extra` if you need them)
  212 i32   CarOrdinal
  216 i32   CarClass
  220 i32   CarPerformanceIndex
  224 i32   DrivetrainType
  228 i32   NumCylinders

Dash extension (232..311), only present if packet >= 311 bytes:
  232..244  PositionX/Y/Z (not currently mapped)
  244 f32   Speed (m/s)
  248 f32   Power (watts)
  252 f32   Torque (Nm)
  256 f32   TireTempFrontLeft
  260 f32   TireTempFrontRight
  264 f32   TireTempRearLeft
  268 f32   TireTempRearRight
  272 f32   Boost
  276 f32   Fuel (0.0-1.0)
  280 f32   DistanceTraveled
  284 f32   BestLap
  288 f32   LastLap
  292 f32   CurrentLap
  296 f32   CurrentRaceTime
  300 u16   LapNumber
  302 u8    RacePosition
  303 u8    Accel (0-255)
  304 u8    Brake (0-255)
  305 u8    Clutch (0-255)
  306 u8    HandBrake (0-255)
  307 u8    Gear
  308 i8    Steer
  309 i8    NormalizedDrivingLine
  310 i8    NormalizedAIBrakeDifference
```

## What's unverified

- **Tire wear extension**: some Forza titles (Forza Horizon 5 is commonly
  cited) are reported to append 4 trailing `f32` tire-wear fields after
  byte 311. `packet.rs` reads them speculatively when the packet is >= 327
  bytes, but **this offset has not been confirmed against a live capture**
  in this environment. Treat `extra["tire_wear_*"]` as unreliable until you
  verify it.
- **`Gear` semantics**: widely described as "0 = Reverse, 1+ = forward
  gears" but conventions have been described inconsistently across
  community docs (e.g. whether Neutral gets its own value or reads as 0
  alongside Reverse). `TelemetryFrame::gear` is deliberately just the raw
  byte with a note in its own doc comment -- don't assume a transform (like
  `gear - 1`) without checking against your title.

## How to verify/calibrate for your title

```
cargo run -p sld-cli -- capture-forza --bind 0.0.0.0:5300 --count 20
```

Point the game's Data Out at the machine/port you ran this on, drive
around, and compare the printed hex/parsed values against what the game's
HUD shows (RPM, speed, gear, fuel). If something's off, the raw hex preview
is there to re-derive the correct offset.
