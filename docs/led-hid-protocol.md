# Logitech wheel RPM LED HID protocol

Why raw HID instead of Logitech's official SDK: the official Logitech
Gaming SDK (`LogitechSteeringWheel.dll` / G HUB SDK) only works on Windows
and requires G HUB running. Talking directly to the wheel over USB HID
works on Windows, Linux, and macOS via `hidapi`, with no dependency on any
Logitech software being installed (including G HUB -- confirmed working
with G HUB left running) -- which is what "cross-platform" means here in
practice.

## Source

Verified against the actively-maintained Linux kernel driver
[`berarma/new-lg4ff`](https://github.com/berarma/new-lg4ff) (`hid-lg4ff.c`
/ `hid-ids.h`) -- a driver that actually runs force feedback + LEDs on
this hardware today, which is stronger evidence than a one-off packet
capture. This replaced an earlier version of this doc that leaned on
vaguer "community reverse-engineering" citations.

## Device identification

USB vendor id for all Logitech wheels: `0x046d`.

| Wheel | Product id | Status |
|---|---|---|
| G27 | `0xc29b` | Supported -- `ClassicLedProtocol` |
| G29 | `0xc24f` | Supported -- `ClassicLedProtocol` |
| G923, native/PC mode | `0xc266` | Supported -- `ClassicLedProtocol` |
| G923, Xbox-compatible mode | `0xc26e` | Supported -- `ClassicLedProtocol`, but see [caveat](#whats-still-inferred-not-driver-confirmed) |
| G923, PlayStation-compatible mode | `0xc267` | Supported -- auto-switched to native mode first, see below |
| G920 | `0xc262` | **Not supported** -- no confirmed LED command, see below |
| Driving Force GT | `0xc29a` | **Not supported** -- no confirmed LED command, see below |

Run `cargo run -p sld-cli -- list-hid-devices` to confirm your wheel's
actual vendor/product id and compare against this table. Run `cargo run
-p sld-cli -- test-leds` to actually find, open, and flash a wheel's LEDs
in isolation, without the game or service running.

## A vendor/product id is not enough -- match the HID usage too

The wheels here enumerate as **several separate HID interfaces** under
the same vendor/product id -- e.g. the G923 in native mode shows up as
four: a vendor-specific config interface, an extra-buttons interface
(x2), and the actual joystick/FFB collection. Only that last one accepts
the LED output report; writing it to any of the others *opens*
successfully but the `write()` call itself fails (Windows:
`ERROR_INVALID_PARAMETER`/`0x57`, "the parameter is incorrect" -- because
the report doesn't match that interface's declared output-report length).

`hidapi`'s device listing exposes `usage_page()`/`usage()` per interface;
the joystick/FFB collection is HID usage page `0x0001` (Generic Desktop),
usage `0x0004` (Joystick). `WheelLedProtocol::usage_page()`/`usage()` (and
`ModeSwitch::usage_page`/`usage`) encode this, and `device.rs` filters on
both fields, not just vendor/product id, when picking which interface
handle to open. Which of the several same-vendor-product-id interfaces
the OS lists *first* isn't guaranteed, so without this filter the code
could open any of them -- including one that opens fine but silently
can't take the LED command.

## `0xf8` is not a HID Report ID -- the joystick collection has none

This is the one that briefly sent the wheel's force-feedback motor to
full-right instead of lighting an LED, so it's worth spelling out
precisely. Reading the joystick/FFB interface's raw report descriptor
(`sld-cli dump-descriptor --vid 0x046d --pid <product-id>`, read-only, no
writes) shows exactly one Output item:

```
06 01 ff 09 02 15 00 26 ff 00 35 00 46 ff 00 75 08 95 10 91 02
-- Usage Page (vendor 0xff01), Usage 2, 16x 8-bit fields, Output, Var, Abs
```

There is no `85 xx` ("Report ID") item anywhere in this descriptor. That
16-byte field is the *entire* Output report, opaque vendor data -- there's
no HID-level structure distinguishing "LED command" from "force effect
command" the way a Report ID would. `new-lg4ff`'s `lg4ff_set_leds()`
writes `cmd = [0xf8, 0x12, led_bits, 0, 0, 0, 0]` into `entry->report`'s
*first 7 data bytes*, not its Report ID field -- confirmed by reading
`lg4ff_send_cmd()` itself: it never touches `entry->report->id`, unlike
the sibling `lg4ff_send_cmd_with_id()` used elsewhere in the driver for
genuinely-numbered reports. So `0xf8` is a **firmware-level command-class
byte living inside the opaque payload**, not a protocol-level Report ID.

`hidapi`'s `write()`/`send_output_report()` calling convention still wants
`data[0]` to be a Report ID (`0x0` for a device with a single unnumbered
report, per hidapi's own docs). Passing `[0xf8, 0x12, led_bits, 0,0,0,0]`
directly makes hidapi consume the `0xf8` as that placeholder and drop it,
shifting every real byte one position left into the wheel's raw
force-feedback payload -- which is exactly what spun the wheel. The fix:
always prepend an explicit `0x00` placeholder first, so the *real* first
payload byte (`0xf8`) lands at payload position 0 as intended:

```
[0x00, 0xf8, 0x12, led_bits, 0x00, 0x00, 0x00, 0x00]
 ^report-id           ^--- what new-lg4ff actually sends on the wire
 placeholder,
 not sent
```

`hidapi` zero-pads the rest up to the interface's declared 17-byte total
(1 placeholder + 16 payload) automatically -- confirmed by `write()`
reporting 17 bytes written for a shorter input. `device.rs`'s
`write_wheel_frame()` does this prepend for every wheel write (LED
command, mode-switch command).

One dead end for anyone revisiting this: `hid-lg4ff.c` sends via
`hid_hw_request(..., HID_REQ_SET_REPORT)`, which reads like "use the USB
Control endpoint" and hidapi's `send_output_report()` is the literal
match for that (`HidD_SetOutputReport` on Windows) -- but that call fails
outright against this wheel (`HidD_SetOutputReport` error, no wheel
motion). Plain `write()` (interrupt OUT endpoint, which this wheel has
for its continuous FFB stream) is what actually works, matching what
Linux's `usbhid` layer picks for an Output report when an interrupt OUT
endpoint exists, regardless of the `HID_REQ_SET_REPORT` naming.

## The command: `ClassicLedProtocol`

HID output report to light the 5-LED RPM bar:

```
bytes: [0xf8, 0x12, led_bits, 0x00, 0x00, 0x00, 0x00]
```

- `0xf8` -- report id used for this class of "extended" wheel commands.
- `0x12` -- sub-command for "set RPM LEDs".
- `led_bits` -- bitmask, bit 0 = leftmost LED, up to bit 4 (5 LEDs total),
  so valid range is `0x00`-`0x1f`.

`new-lg4ff`'s `lg4ff_set_leds()` sends exactly this, and the driver
registers it (`has_leds = 1`) only for product ids G27, G29, and G923 in
its **native/PC mode** (`0xc266`) -- not for G920 or DFGT (see below), and
not for G923 in PlayStation mode without switching it first (see next
section).

## G923 PlayStation mode: switch to native mode first

A G923 connected in PlayStation-compatible mode enumerates as product id
`0xc267`. The driver doesn't send LED commands to that id directly --
instead, on seeing it, it sends a one-shot "switch to native mode, with
detach" command, after which the wheel disconnects and re-enumerates as
`0xc266` (native mode), where the normal command above then works.

That switch command (`lg4ff_switch_from_ps_mode` /
`lg4ff_mode_switch_30_g923` in the driver source) is notable for using a
**different report id**: `0x30`, not the usual `0xf8`.

```
bytes: [0x30, 0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00]
       ^report id  ^--- 7-byte payload, same as sent to other wheel modes
```

`sld-output-logitech-hid` implements this in `device.rs`:
1. If a `0xc267` device is found (and nothing matched directly), send the
   switch command to it.
2. Close that handle -- the wheel is about to detach.
3. Poll (every 250ms, up to 5s) for a `0xc266` device to reappear.
4. Open that and proceed with the normal LED command.

If the wheel doesn't reappear within 5 seconds, this is logged as a
warning and LED output for that run is disabled -- it doesn't crash the
service or retry indefinitely.

## What's still inferred, not driver-confirmed

- **G923 Xbox-compatible mode** (`0xc26e`,
  `USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL` in the driver's `hid-ids.h`) is
  *defined* in that header but never referenced anywhere else in
  `hid-lg4ff.c` -- no LED registration, no mode-switch handling. This
  project currently treats it as already-native (grouped into
  `ClassicLedProtocol`'s matches) on the reasoning that Xbox controllers
  don't need PS4-style mode negotiation on a PC the way the PlayStation
  variant does. **That's an inference, not something the driver source
  confirms.** If you own a G923 for Xbox and it enumerates as `0xc26e` on
  your PC, `sld-cli list-hid-devices` will tell you, and testing whether
  the LEDs actually respond would turn this from inference into fact (or
  disprove it, in which case it needs the same mode-switch treatment as
  the PlayStation edition, or a different command entirely).
- **G920** (`0xc262`) is explicitly *excluded* from `has_leds` in the
  driver. This doesn't necessarily mean the hardware lacks shift-light
  LEDs -- it may just mean the driver hasn't implemented it -- but there's
  no evidence here for what command would work, so it's kept out of
  `all_known_protocols` entirely rather than guessing. `sld-cli
  list-hid-devices` plus the running service will report "found a G920,
  but its LED protocol isn't implemented yet" rather than silently doing
  nothing, if you have one connected.
- **Driving Force GT** (`0xc29a`) is similarly excluded from `has_leds` in
  this driver, despite an earlier version of this project grouping it with
  G27/G29 on weaker evidence. Same treatment as G920.

## Adding another wheel family, or firming up an inference above

Implement `WheelLedProtocol` (`crate::protocol`) for it and add it to
`all_known_protocols()` (for a direct match) or `known_mode_switches()`
(for a wheel that needs a switch command first, following the G923 PS
pattern). Nothing else in `sld-output-logitech-hid` needs to change --
`device.rs`'s `find_and_open_wheel` already tries direct matches, then
known mode-switches, then falls back to a "recognized but unsupported"
diagnostic via `known_unsupported_wheels()`.
