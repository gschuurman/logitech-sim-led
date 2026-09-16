# Logitech wheel RPM LED HID protocol

Why raw HID instead of Logitech's official SDK: the official Logitech
Gaming SDK (`LogitechSteeringWheel.dll` / G HUB SDK) only works on Windows
and requires G HUB running. Talking directly to the wheel over USB HID
works on Windows, Linux, and macOS via `hidapi`, with no dependency on any
Logitech software being installed -- which is what "cross-platform" means
here in practice.

## Device identification

USB vendor id for all Logitech wheels: `0x046d`.

| Wheel | Product id | `WheelLedProtocol` impl |
|---|---|---|
| G27 | `0xc29b` | `G29Protocol` |
| G29 | `0xc24f` | `G29Protocol` |
| Driving Force GT | `0xc29a` | `G29Protocol` |
| G920 (Xbox) | `0xc262` | `G920Protocol` (stub) |
| G923 (PlayStation) | `0xc266` | `G920Protocol` (stub) |
| G923 (Xbox) | `0xc267` | `G920Protocol` (stub) |

Run `cargo run -p sld-cli -- list-hid-devices` to confirm your wheel's
actual vendor/product id and compare against this table.

## G27 / G29 / Driving Force GT: `G29Protocol`

HID output report to light the 5-LED RPM bar:

```
bytes: [0xf8, 0x12, led_bits, 0x00, 0x00, 0x00, 0x00]
```

- `0xf8` -- report id used for this class of "extended" wheel commands.
- `0x12` -- sub-command for "set RPM LEDs".
- `led_bits` -- bitmask, bit 0 = leftmost LED, up to bit 4 (5 LEDs total),
  so valid range is `0x00`-`0x1f`.

This matches the command implemented by the Linux kernel's `hid-lg4ff`
driver and used by userspace tools such as `oversteer` -- it's about as
well-established as community reverse-engineering of this hardware gets.

## G920 / G923: `G920Protocol` -- **unverified stub**

These wheels sit on a different USB profile (they're built around the
Xbox/PlayStation controller bus rather than the older Driving Force
lineage), and the LED command is not as consistently documented across
community sources. `G920Protocol::encode_leds` currently just mirrors the
G29 command byte-for-byte as a starting point -- it may not actually light
the LEDs correctly on real G920/G923 hardware.

To fill this in properly:

1. Get access to real G920/G923 hardware.
2. Capture the actual output report G HUB sends when the wheel's RPM LEDs
   light up -- e.g. with Wireshark + USBPcap on Windows while G HUB is
   running and a game with LED support is active, or by checking whether
   `hid-lg4ff` has grown support for these models by the time you read
   this (it's evolved over time).
3. Replace the body of `G920Protocol::encode_leds` with the real bytes, and
   remove the "unverified" language from its doc comment.

## Adding another wheel family

Implement `WheelLedProtocol` (`crate::protocol`) for it and add it to
`all_known_protocols()`. Nothing else in `sld-output-logitech-hid` needs to
change -- `device.rs` already iterates all registered protocols and picks
whichever one's `matches()` returns true for the first connected device.
