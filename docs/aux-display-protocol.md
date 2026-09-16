# Auxiliary display wire protocol

`outputs.aux_display` in config sends one line of JSON per update to
whatever transport is configured (serial today -- see
`crates/sld-outputs/aux-display/src/transport.rs`). Newline-delimited JSON
was chosen specifically because it's trivial to parse on a microcontroller
(one `deserializeJson` call per line, no framing/length-prefix logic
needed) and easy to eyeball with any serial terminal while debugging.

## Schema (`AuxFrame`)

```json
{"rpm":4213.5,"rpm_max":7200.0,"speed_kph":142.3,"gear":4,"fuel":0.62,"lap":3,"position":2}
```

| Field | Type | Notes |
|---|---|---|
| `rpm` | number | Current engine RPM. |
| `rpm_max` | number | Redline RPM for the current car. |
| `speed_kph` | number | Converted from the internal m/s representation. |
| `gear` | integer | Raw gear number, same convention as `TelemetryFrame::gear` (see docs/telemetry-protocol-forza.md for Forza's specific convention). |
| `fuel` | number or `null` | 0.0-1.0, `null` if the source doesn't report it. |
| `lap` | integer or `null` | `null` if the source doesn't report it. |
| `position` | integer or `null` | Race position, `null` if not applicable/available. |

Defined in `crates/sld-outputs/aux-display/src/protocol.rs`; the firmware
stub's parser (`firmware/esp32-aux-display/src/main.cpp`) mirrors this
exact field set.

## Rate

Independent of how fast the telemetry source publishes -- `AuxDisplayOutput`
down-samples to `outputs.aux_display.update_rate_hz` (default 10 Hz), since
a display doesn't need (and a slow microcontroller link may not want)
frames at the source's native rate.

## Transport

Serial only today (`SerialTransport`, USB CDC at 115200 baud, matching the
firmware stub's `Serial.begin(115200)`). `AuxTransport` is a small async
trait (`send_line(&mut self, line: &str)`), so adding WiFi/TCP or BLE is a
new impl in `transport.rs` -- `device.rs` and the wire schema don't change.

## Extending the schema

Add fields to `AuxFrame` and its `From<&TelemetryFrame>` impl in
`protocol.rs`, then mirror them in the firmware's `AuxFrame` struct and
`parseLine()`. Keep it a deliberate subset of `TelemetryFrame` -- pull in
only what an actual display will render, not everything a source happens
to expose.
