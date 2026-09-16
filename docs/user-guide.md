# User Guide

This page is for people who want to **run** logitech-sim-led and light up
their wheel -- not write code. If you want to add a game, add an output, or
otherwise hack on it, see [architecture.md](architecture.md) instead.

## What is this?

A small background app that watches telemetry from your racing game while
you play and lights up the RPM shift-light LEDs on your Logitech wheel as
you approach redline -- the same kind of feature you'd get from Logitech's
own software, but working without G HUB installed, and built so it isn't
locked to one game.

## What it does today

- **Shift-light LEDs**: as engine RPM climbs toward redline, the 5 LEDs on
  your wheel light up one by one, then flash when you're at redline. When
  to start lighting up is configurable per your preference.
- **Live telemetry page**: a small page in your browser
  (`http://127.0.0.1:5301`) showing RPM, speed, and gear updating live while
  you drive -- useful to confirm everything's actually connected before you
  start caring about the LEDs.
- **Forza support**: Forza Horizon 5, Forza Horizon 6, and Forza Motorsport,
  via the games' built-in telemetry export feature (no mods, no memory
  reading).

## How it works, in plain terms

```
   Forza (in-game setting)  --sends telemetry over your network-->  logitech-sim-led
                                                                            |
                                                          reads current RPM, decides which
                                                          LEDs should be lit or blinking,
                                                          and tells your wheel over USB
```

Concretely: Forza has a setting that makes it broadcast your car's telemetry
(RPM, speed, gear, and more) over your local network as it plays. This app
listens for that, works out from the RPM how many of the wheel's 5 LEDs
should be lit (and whether they should be flashing), and writes that
directly to the wheel over USB -- it talks to the wheel's hardware
interface directly, so **you don't need Logitech G HUB installed or
running**.

## What you'll need

- A Logitech wheel with RPM shift-light LEDs. Supported today: **G27,
  G29, and G923** (native/PC mode, and PlayStation mode -- that one gets
  automatically switched into native mode on connect). G923 Xbox-mode
  should also work but is less certain -- see
  [led-hid-protocol.md](led-hid-protocol.md). **G920 and Driving Force GT
  are not supported** -- there's no confirmed LED command for them, so the
  service will tell you it found the wheel but can't drive its LEDs,
  rather than silently doing nothing.
- Forza Horizon 5, Forza Horizon 6, or Forza Motorsport on PC.
- A Windows, Linux, or macOS machine to run the service on -- normally the
  same PC the game is running on.

## Installing

**Prebuilt binaries**: once a release is published, download the archive
for your OS from the project's [GitHub Releases](https://github.com/gschuurman/logitech-sim-led/releases)
page, and unzip it somewhere.

**Build it yourself**: if there's no release yet for your platform, or you
want the latest code, see the "Building" section in the main
[README](../README.md) -- it's a standard `cargo build --workspace`.

## Setting it up

1. **Check your wheel is recognized.** From the folder you installed/built
   into:
   ```
   sld-cli list-hid-devices
   ```
   You should see a line with vendor id `0x046d` (Logitech) and a product
   id matching your wheel -- cross-check against the table in
   [led-hid-protocol.md](led-hid-protocol.md).

2. **Turn on Forza's telemetry export.** In-game: Settings > HUD and
   Gameplay > Data Out.
   - Data Out: **On**
   - IP Address: the IP of the machine running the service (`127.0.0.1` if
     it's the same PC as the game)
   - Port: `5300` (matches the default in `config/default.toml`)

3. **Check `config/default.toml`.** The defaults work for the common case
   (Forza on the same PC, LEDs on): the shipped file already has
   `sources.forza` and `outputs.logitech_led` enabled.

4. **Run it:**
   ```
   sld-service
   ```
   Leave this running in the background while you play.

5. **Confirm it's working.** Open `http://127.0.0.1:5301` in a browser --
   once you're in a race, you should see RPM/speed/gear updating live. Then
   check the wheel itself: the LEDs should start lighting up as you
   accelerate toward redline.

## Tuning the shift light

Two settings in `config/default.toml`, under `[outputs.logitech_led]`:

- **`shift_point_pct`** (default `0.85`): how far through the RPM range
  (from idle to redline) the *first* LED lights up. `0.85` means LEDs start
  appearing at 85% of the way to redline. Lower this (e.g. `0.7`) for an
  earlier warning, raise it (e.g. `0.95`) if you want them to appear right
  at the very end.
- **`blink_at_redline`** (default `true`): whether all 5 LEDs flash once
  you hit redline, instead of just staying solid. Turn this off if you find
  the flashing distracting.

Restart `sld-service` after changing config for it to take effect.

## Troubleshooting

**LEDs never light up:**
- Run `sld-cli list-hid-devices` -- if your wheel isn't listed at all,
  check the USB connection. If it's listed as a G920 or Driving Force GT,
  that's expected not to work yet -- see the note above. Check the
  service's own logs too: it logs a specific "found a G920/DFGT, but its
  LED protocol isn't implemented" message rather than a generic failure.
- If you have a G923 in PlayStation mode, the service sends a mode-switch
  command on startup and the wheel should briefly disconnect/reconnect --
  if that doesn't happen within a few seconds, the logs will say so.
- Check the live telemetry page (`http://127.0.0.1:5301`) -- if RPM isn't
  updating there either, the problem is upstream of the LEDs (see next
  point), not the wheel.

**The live telemetry page shows nothing / stays blank:**
- Double check Forza's Data Out IP/port matches where `sld-service` is
  running and listening (`sources.forza.bind_addr` in config, default
  `0.0.0.0:5300`).
- Make sure you're actually in a race/session -- the app ignores packets
  sent while the game is paused or in a menu.
- If the game and the service are on *different* machines, check firewalls
  aren't blocking inbound UDP on port 5300.
- For deeper debugging, run `sld-cli capture-forza` -- it prints every
  packet it receives, raw and parsed, which tells you whether packets are
  arriving at all.

**Gear/speed/fuel show up as blank or zero:**
- Forza Horizon 5/6 always send this data -- there's no format toggle, so
  if it's missing there, something upstream isn't working (see the
  previous points).
- Forza Motorsport specifically has a **Data Out Packet Format** setting
  that must be set to "Dash", not "Sled" -- "Sled" only sends RPM/physics
  data, not speed/gear/pedals/fuel.

## What you can do with it (beyond the basics)

- **Run it on a different machine than the game.** Since Forza sends
  telemetry over the network, `sld-service` doesn't have to run on the
  same PC -- point Forza's Data Out IP at another machine on your LAN
  (e.g. a small always-on box just for this) and run the service there.
- **Watch live telemetry from another device.** The web page at port 5301
  is just a normal web page -- open it from your phone or a second monitor
  on the same network (`http://<that machine's IP>:5301`) while you drive.
- **Point it at a different Forza title without reconfiguring much** --
  Horizon 5, Horizon 6, and Motorsport all use the same Data Out feature
  and port, so switching games generally just works.
- **Help fill in what's unverified.** If you own a G920 or G923 and are
  comfortable poking at USB traffic, confirming the real LED HID command
  for those wheels (see [led-hid-protocol.md](led-hid-protocol.md)) would
  turn that from a guess into something that actually works.
- **Add another output** -- a second display, an OBS overlay, logging to
  CSV, whatever else you can drive from the same telemetry -- see
  [adding-an-output.md](adding-an-output.md) for the shape that takes.

## What's foundation-only (not a finished feature)

To set expectations honestly:

- **G920 and Driving Force GT LEDs are not supported** -- no confirmed
  command exists for them (see [led-hid-protocol.md](led-hid-protocol.md)).
- **G923 Xbox-mode** is supported on the (unconfirmed) assumption that it
  behaves like native mode already -- flag it if your LEDs don't light up
  on that specific wheel/mode.
- **System tray icon** isn't wired up yet -- the app currently runs as a
  plain background process; the local web page is the way to see it's
  alive.

See the status table in [architecture.md](architecture.md#whats-genuinely-done-vs-foundation-only)
for the complete, up-to-date picture.
