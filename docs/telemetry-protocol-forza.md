# Forza "Data Out" telemetry protocol

Byte layout below is verified against Forza's own documentation, not
community reverse-engineering:

- Forza Horizon 6: <https://support.forza.net/hc/en-us/articles/51744149102611-Forza-Horizon-6-Data-Out-Documentation>
  (that page states Forza Horizon 5's Car Dash payload is byte-for-byte
  identical to Horizon 6's)
- Forza Motorsport (2023): <https://support.forza.net/hc/en-us/articles/21742934024211-Forza-Motorsport-Data-Out-Documentation>

## Enabling it in-game

Settings > HUD and Gameplay (Forza Motorsport: Settings > Gameplay & HUD >
"UDP Race Telemetry"):

- Data Out: On
- Data Out IP Address: the machine running `sld-service`
- Data Out IP Port: must match `sources.forza.bind_addr` in
  `config/default.toml` (default `5300`). Forza Motorsport's docs
  specifically warn against using ports 5200-5300 for this, since the game
  binds its own outgoing socket somewhere in that range -- if you hit
  issues on the default port, try a different one on both ends.
- Forza Motorsport only: Data Out Packet Format -- "Sled" or "Dash". Pick
  Dash to get speed/gear/pedals/fuel/lap data, not just RPM.
- Forza Horizon 5/6: no format choice -- the game always sends one fixed
  packet (see below).

## The two formats are genuinely different, not one format plus an extension

This is the one thing earlier notes here got wrong: it's not "one Dash
layout, maybe with some extra fields tacked on the end." The Position/
Speed/.../Gear block sits at a **different byte offset** depending on the
title:

- **Forza Motorsport**: Sled (232 bytes) is optionally followed immediately
  by the Position/Speed/.../Gear block, then `TireWearFrontLeft..RearRight`
  and `TrackOrdinal` -- Dash packets are >= 331 bytes total.
- **Forza Horizon 5/6**: Sled (232 bytes) is *always* followed by 3 fields
  Motorsport doesn't have -- `CarGroup` (u32), `SmashableVelDiff` (f32),
  `SmashableMass` (f32) -- and *then* the same Position/Speed/.../Gear
  block. Horizon does not send `TireWear*`/`TrackOrdinal`. Total packet
  size is fixed at 324 bytes.

`sld-source-forza::packet::parse` tells them apart by length (Motorsport's
331+ is well clear of Horizon's fixed 324) and returns which one it saw via
`TitleExtras::Horizon { .. }` / `TitleExtras::Motorsport { .. }`.

**The Sled block itself -- including `CurrentEngineRpm`, `EngineIdleRpm`,
`EngineMaxRpm`, which is all the LED shift-light output needs -- is
byte-identical between titles and sits before where the two formats
diverge.** The LED feature works the same regardless of which title/format
you're running.

## Byte layout

```
Sled (0..232), identical across all titles/formats:
  0   S32   IsRaceOn            (1 = race on, 0 = in menus/stopped)
  4   U32   TimestampMS
  8   F32   EngineMaxRpm
  12  F32   EngineIdleRpm
  16  F32   CurrentEngineRpm
  20  F32   AccelerationX/Y/Z (3x, not currently mapped to TelemetryFrame)
  32  F32   VelocityX
  36  F32   VelocityY
  40  F32   VelocityZ
  44..212   angular velocity, yaw/pitch/roll, per-wheel suspension/tire/slip
            physics (not currently mapped -- add to `extra` if you need
            them; see the note on WheelInPuddle* below if you do)
  212 S32   CarOrdinal
  216 S32   CarClass            (0 = D .. 7 = X class)
  220 S32   CarPerformanceIndex (100..999)
  224 S32   DrivetrainType      (0 = FWD, 1 = RWD, 2 = AWD)
  228 S32   NumCylinders

Forza Horizon 5/6 only, 232..244 (pushes everything below down by 12
bytes relative to Motorsport):
  232 U32   CarGroup
  236 F32   SmashableVelDiff
  240 F32   SmashableMass

Shared Dash tail, 79 bytes -- starts at 232 for Motorsport, 244 for Horizon:
  +0  F32   PositionX
  +4  F32   PositionY
  +8  F32   PositionZ
  +12 F32   Speed (m/s)
  +16 F32   Power (watts)
  +20 F32   Torque (Nm)
  +24 F32   TireTempFrontLeft
  +28 F32   TireTempFrontRight
  +32 F32   TireTempRearLeft
  +36 F32   TireTempRearRight
  +40 F32   Boost
  +44 F32   Fuel (0.0-1.0)
  +48 F32   DistanceTraveled
  +52 F32   BestLap
  +56 F32   LastLap
  +60 F32   CurrentLap
  +64 F32   CurrentRaceTime
  +68 U16   LapNumber
  +70 U8    RacePosition
  +71 U8    Accel (0-255)
  +72 U8    Brake (0-255)
  +73 U8    Clutch (0-255)
  +74 U8    HandBrake (0-255)
  +75 U8    Gear
  +76 S8    Steer (-127 full left .. 127 full right)
  +77 S8    NormalizedDrivingLine
  +78 S8    NormalizedAIBrakeDifference

Forza Motorsport only, right after the tail above (i.e. at absolute
offset 311):
  311 F32   TireWearFrontLeft
  315 F32   TireWearFrontRight
  319 F32   TireWearRearLeft
  323 F32   TireWearRearRight
  327 S32   TrackOrdinal
```

## Things worth knowing that aren't in the byte table

- **`Gear`'s Reverse/Neutral convention is not documented by Forza.** Both
  official pages just say "// Current gear \n U8 Gear;" with no stated
  mapping. `TelemetryFrame::gear` is deliberately the raw byte -- don't
  assume a transform (e.g. "0 = Reverse") without checking against your
  own captures.
- **`WheelInPuddle*` differs in type between titles**, despite occupying
  the same 4-byte slot: Forza Horizon's docs declare it `S32` (a 0/1 flag),
  Forza Motorsport's declare the equivalent field `F32 WheelInPuddleDepth*`
  (a continuous 0.0-1.0 depth). This parser doesn't currently read that
  field at all, so it's not a live bug -- just a trap if you extend
  `SledData` to include it later: read it as the right type per title, not
  one type for both.
- **Exact total byte count for Motorsport's Dash format isn't stated** in
  its docs (unlike Horizon's, which explicitly says 324). This parser's
  `FM_DASH_MIN_LEN` (331) is a computed lower bound, checked with `>=`, so
  it doesn't care whether the real wire format adds trailing alignment
  padding or not.

## How to verify/capture for your own setup

```
cargo run -p sld-cli -- capture-forza --bind 0.0.0.0:5300 --count 20
```

Prints every packet's length, a raw hex preview, and the parsed
Sled/Dash/title-specific fields -- useful for confirming Gear's actual
convention on your title, or just double-checking data is arriving at all.
