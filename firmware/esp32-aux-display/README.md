# esp32-aux-display (foundation stub)

This is **not** finished firmware. It's the minimum needed to prove the link
from `sld-service`'s `aux_display` output to a microcontroller works:

- `src/main.cpp` reads newline-delimited JSON frames over USB serial and
  parses them with ArduinoJson, printing the parsed values back over
  `Serial` (so you can verify the link with nothing but the PlatformIO
  serial monitor -- no display hardware required yet).
- The wire schema it parses is documented in
  `../../docs/aux-display-protocol.md`.

## Building

```
cd firmware/esp32-aux-display
pio run             # build
pio run -t upload   # flash
pio device monitor   # watch parsed frames
```

On the `sld-service` side, set `outputs.aux_display.serial_port` in
`config/default.toml` to the ESP32's serial port (e.g. `COM5` on Windows,
`/dev/ttyUSB0` on Linux) and `enabled = true`.

## Turning this into a real display

1. Pick hardware (a common choice: ESP32 dev board + a small SPI TFT, e.g.
   an ILI9341, driven by `TFT_eSPI` or `LVGL`).
2. Replace `renderFrame()`'s `Serial.printf` calls with actual draw calls.
3. If the display needs more telemetry than `AuxFrame` currently carries
   (tire temps, delta time, etc.), extend `AuxFrame` in
   `crates/sld-outputs/aux-display/src/protocol.rs` and mirror the new
   fields here -- see docs/aux-display-protocol.md for how the schema is
   meant to evolve.
4. If serial isn't the right transport for your setup (e.g. the ESP32 isn't
   USB-tethered to the PC), add a new `AuxTransport` impl (WiFi/TCP or BLE)
   in `crates/sld-outputs/aux-display/src/transport.rs` -- `device.rs`
   doesn't need to change.
