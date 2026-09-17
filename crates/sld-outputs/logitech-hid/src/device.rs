//! `OutputDevice` implementation tying [`crate::curve`] and
//! [`crate::protocol`] to an actual HID device.
//!
//! `hidapi::HidDevice` does blocking I/O and (depending on platform/version)
//! isn't safely usable across an async yield point, so all device I/O runs
//! on its own dedicated OS thread. The async `run` loop only computes the
//! desired LED state and hands the *latest* one to that thread over a
//! `sync_channel(1)` -- if the writer thread is momentarily behind, older
//! intermediate states are simply dropped rather than queued, which is the
//! right behavior for something that only cares about "what should the
//! LEDs show right now".

use crate::curve::{LedBarState, ShiftLightCurve};
use crate::protocol::{self, WheelLedProtocol};
use async_trait::async_trait;
use sld_core::shutdown::ShutdownSignal;
use sld_core::telemetry::TelemetryFrame;
use sld_core::traits::{OutputDevice, TelemetryRx};
use std::sync::mpsc::{sync_channel, RecvTimeoutError};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

pub struct LogitechLedOutput {
    curve: Arc<RwLock<ShiftLightCurve>>,
    update_interval: Duration,
}

impl LogitechLedOutput {
    pub fn new(curve: ShiftLightCurve) -> Self {
        Self {
            curve: Arc::new(RwLock::new(curve)),
            update_interval: Duration::from_millis(33), // ~30 Hz
        }
    }

    /// A shared handle to the curve this output evaluates against every
    /// tick -- updating it through this handle (e.g. from a settings API)
    /// takes effect immediately, no restart needed. Keeps this crate
    /// unaware of `sld-core::config::AppConfig`/settings persistence
    /// entirely; `sld-service`'s registry.rs is the one place that wires
    /// config into concrete sources/outputs, this is just the live knob
    /// it hands out.
    pub fn curve_handle(&self) -> Arc<RwLock<ShiftLightCurve>> {
        Arc::clone(&self.curve)
    }
}

#[async_trait]
impl OutputDevice for LogitechLedOutput {
    fn id(&self) -> &str {
        "logitech_led"
    }

    async fn run(
        &mut self,
        mut rx: TelemetryRx,
        mut shutdown: ShutdownSignal,
    ) -> anyhow::Result<()> {
        let (state_tx, state_rx) = sync_channel::<LedBarState>(1);
        let hid_thread = std::thread::spawn(move || hid_writer_loop(state_rx));

        let mut ticker = tokio::time::interval(self.update_interval);
        let mut latest: Option<TelemetryFrame> = None;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                frame = rx.recv() => {
                    match frame {
                        Ok(f) => latest = Some(f),
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = ticker.tick() => {
                    if let Some(f) = &latest {
                        // Re-read every tick (cheap, uncontended lock at
                        // ~30 Hz) rather than snapshotting once outside the
                        // loop, so a settings change takes effect on the
                        // very next tick.
                        let curve = *self.curve.read().unwrap();
                        let state = curve.evaluate(f.rpm, f.rpm_max);
                        // Best-effort: if the writer thread hasn't drained
                        // the previous state yet, drop this one -- it'll be
                        // superseded by the next tick anyway.
                        let _ = state_tx.try_send(state);
                    }
                }
            }
        }

        drop(state_tx);
        let _ = hid_thread.join();
        Ok(())
    }
}

/// Writes a wheel command frame (as documented in `protocol.rs`, e.g.
/// `[0xf8, 0x12, led_bits, ...]`) to the HID device.
///
/// Two things had to line up here, both confirmed against the wheel's raw
/// report descriptor and against `new-lg4ff`'s actual driver source (not
/// just this crate's paraphrase of it) -- see docs/led-hid-protocol.md:
///
/// 1. **Framing.** The joystick/FFB collection these commands target
///    declares its Output report with *no Report ID field* (no `85 xx`
///    item). `hidapi` still requires callers to put something in
///    `data[0]` -- its docs say devices with a single unnumbered report
///    must set that byte to `0x0` -- so `frame`'s own leading byte
///    (`0xf8`, the report id the driver documents) must not go there
///    directly, or it gets consumed as that placeholder and every real
///    byte shifts left by one, turning a "set LEDs" command into an
///    unrelated, unvalidated write on the wheel's raw force-feedback
///    channel. Confirmed the hard way: this exact bug briefly commanded
///    the wheel to full-right force instead of lighting an LED.
/// 2. **Transfer type.** Tried routing this through `send_output_report()`
///    (hidapi's Control-endpoint Set_Report, matching `hid-lg4ff.c`'s
///    `HID_REQ_SET_REPORT` call) on the theory that it'd be a more faithful
///    match for the driver -- but Windows' `HidD_SetOutputReport` outright
///    rejected it (`hidapi error: HidD_SetOutputReport`, no wheel motion).
///    Falling back to `write()`, which per hidapi's own docs prefers the
///    interrupt OUT endpoint when the device has one (this wheel does, for
///    its continuous FFB stream) -- Linux's `usbhid` layer picks the same
///    transport for Output reports when an interrupt OUT endpoint exists,
///    so this is the closer match in practice, despite the driver's
///    `HID_REQ_SET_REPORT` naming suggesting otherwise.
fn write_wheel_frame(device: &hidapi::HidDevice, frame: &[u8]) -> hidapi::HidResult<usize> {
    let mut buf = Vec::with_capacity(frame.len() + 1);
    buf.push(0x00);
    buf.extend_from_slice(frame);
    device.write(&buf)
}

/// Standalone diagnostic: finds and opens a wheel exactly like the real
/// output device does, then flashes all 5 LEDs on briefly. Returns the
/// protocol id used on success, or the underlying error otherwise -- useful
/// for `sld-cli test-leds` to narrow down "found the wheel but couldn't
/// write to it" vs. "never found it" without needing the full service/game
/// running.
pub fn test_leds() -> anyhow::Result<&'static str> {
    let mut api = hidapi::HidApi::new()?;
    let Some((device, proto)) = find_and_open_wheel(&mut api) else {
        anyhow::bail!("no supported wheel found or opened -- see warnings above for details");
    };

    write_wheel_frame(&device, &proto.encode_leds(0x1f))?;
    std::thread::sleep(Duration::from_millis(800));
    write_wheel_frame(&device, &proto.encode_leds(0x00))?;
    Ok(proto.id())
}

/// Wheels aren't always present under their native product id the moment
/// this thread starts -- the service may start before the wheel is powered
/// on, or before whatever puts it in "sim" mode has run. Retrying
/// indefinitely (rather than giving up after one failed scan, as this used
/// to) means a wheel that shows up later still gets picked up, instead of
/// silently never lighting an LED for the rest of the process's life while
/// telemetry keeps flowing to everything else on the bus.
fn hid_writer_loop(rx: std::sync::mpsc::Receiver<LedBarState>) {
    let mut api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            tracing::error!(error = %e, "logitech_led: failed to initialize hidapi");
            return;
        }
    };

    loop {
        if let Err(e) = api.refresh_devices() {
            tracing::warn!(error = %e, "logitech_led: failed to refresh HID device list");
        }

        if let Some((device, proto)) = find_and_open_wheel(&mut api) {
            let shutting_down = run_led_writer(device, proto.as_ref(), &rx);
            if shutting_down {
                return;
            }
            // Device was lost mid-run (write failures) -- loop back around
            // and try to find it again.
            continue;
        }

        // No wheel found this pass -- wait a bit before rescanning, but
        // keep draining the channel while we wait so shutdown (channel
        // disconnect) is noticed promptly instead of only after the sleep.
        match rx.recv_timeout(Duration::from_secs(2)) {
            Err(RecvTimeoutError::Disconnected) => return,
            _ => {}
        }
    }
}

/// Locates a supported wheel and returns an opened device + the protocol to
/// drive it with. Handles three cases, in order:
///
/// 1. A wheel already presenting under a product id
///    [`protocol::ClassicLedProtocol`] recognizes -- opened directly.
/// 2. A wheel in a known compatibility mode (currently: G923 in
///    PlayStation mode) -- sends the mode-switch command, waits for it to
///    re-enumerate under its native product id, then opens that.
/// 3. A recognized-but-unsupported Logitech wheel (e.g. G920) -- logged
///    with a specific message so it's clearly not just "not found".
///
/// Returns `None` (having already logged why) if no wheel could be opened.
fn find_and_open_wheel(
    api: &mut hidapi::HidApi,
) -> Option<(hidapi::HidDevice, Box<dyn WheelLedProtocol>)> {
    let protocols = protocol::all_known_protocols();

    if let Some(found) = try_open_direct_match(api, &protocols) {
        return Some(found);
    }

    if let Some(found) = try_switch_and_reopen(api, &protocols) {
        return Some(found);
    }

    for dev_info in api.device_list() {
        if dev_info.vendor_id() != protocol::LOGITECH_VENDOR_ID {
            continue;
        }
        if let Some((_, name)) = protocol::known_unsupported_wheels()
            .into_iter()
            .find(|(pid, _)| *pid == dev_info.product_id())
        {
            tracing::warn!(
                wheel = name,
                product_id = format!("{:#06x}", dev_info.product_id()),
                "logitech_led: found a {name}, but its LED protocol isn't implemented yet \
                 (see docs/led-hid-protocol.md)"
            );
            return None;
        }
    }

    tracing::warn!(
        "logitech_led: no supported wheel found; run `sld-cli list-hid-devices` to check vendor/product ids"
    );
    None
}

fn try_open_direct_match(
    api: &hidapi::HidApi,
    protocols: &[Box<dyn WheelLedProtocol>],
) -> Option<(hidapi::HidDevice, Box<dyn WheelLedProtocol>)> {
    for dev_info in api.device_list() {
        for proto in protocols {
            if !proto.matches(dev_info.vendor_id(), dev_info.product_id()) {
                continue;
            }
            if dev_info.usage_page() != proto.usage_page() || dev_info.usage() != proto.usage() {
                continue;
            }
            match dev_info.open_device(api) {
                Ok(dev) => {
                    tracing::info!(
                        protocol = proto.id(),
                        "opened Logitech wheel for LED output"
                    );
                    return Some((dev, clone_boxed(proto.as_ref())));
                }
                Err(e) => {
                    tracing::warn!(error = %e, protocol = proto.id(), "found matching wheel but failed to open it");
                }
            }
        }
    }
    None
}

fn try_switch_and_reopen(
    api: &mut hidapi::HidApi,
    protocols: &[Box<dyn WheelLedProtocol>],
) -> Option<(hidapi::HidDevice, Box<dyn WheelLedProtocol>)> {
    for switch in protocol::known_mode_switches() {
        let path = api.device_list().find_map(|d| {
            (d.vendor_id() == protocol::LOGITECH_VENDOR_ID
                && d.product_id() == switch.matches_product_id
                && d.usage_page() == switch.usage_page
                && d.usage() == switch.usage)
                .then(|| d.path().to_owned())
        });
        let Some(path) = path else { continue };

        tracing::info!(
            wheel = switch.id,
            "found wheel in a compatibility mode; sending mode-switch command"
        );
        let dev = match api.open_path(&path) {
            Ok(dev) => dev,
            Err(e) => {
                tracing::warn!(error = %e, wheel = switch.id, "failed to open wheel to switch its mode");
                continue;
            }
        };
        if let Err(e) = write_wheel_frame(&dev, &switch.switch_report) {
            tracing::warn!(error = %e, wheel = switch.id, "failed to send mode-switch command");
            continue;
        }
        // The wheel detaches and re-enumerates under a new product id
        // after this -- drop our handle to the old one and poll for it.
        drop(dev);

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(250));
            if api.refresh_devices().is_err() {
                continue;
            }
            let found = api.device_list().find(|d| {
                d.vendor_id() == protocol::LOGITECH_VENDOR_ID
                    && d.product_id() == switch.expected_product_id_after_switch
                    && d.usage_page() == switch.usage_page
                    && d.usage() == switch.usage
            });
            if let Some(dev_info) = found {
                if let Some(proto) = protocols.iter().find(|p| {
                    p.matches(dev_info.vendor_id(), dev_info.product_id())
                        && p.usage_page() == dev_info.usage_page()
                        && p.usage() == dev_info.usage()
                }) {
                    match dev_info.open_device(api) {
                        Ok(dev) => {
                            tracing::info!(
                                wheel = switch.id,
                                protocol = proto.id(),
                                "wheel switched mode successfully, opened for LED output"
                            );
                            return Some((dev, clone_boxed(proto.as_ref())));
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, wheel = switch.id, "wheel re-enumerated but failed to open");
                        }
                    }
                }
            }
        }
        tracing::warn!(
            wheel = switch.id,
            "wheel did not re-enumerate in native mode within 5s after mode-switch command"
        );
    }
    None
}

/// `all_known_protocols()` returns owned `Box<dyn WheelLedProtocol>`s with
/// no state, so a fresh instance found by matching `id()` is equivalent to
/// cloning the trait object we're already holding a `&` to.
fn clone_boxed(proto: &dyn WheelLedProtocol) -> Box<dyn WheelLedProtocol> {
    protocol::all_known_protocols()
        .into_iter()
        .find(|p| p.id() == proto.id())
        .expect("id came from this exact list")
}

/// Runs until the channel disconnects (service shutting down) or writes to
/// the device start failing repeatedly (wheel unplugged/powered off).
/// Returns `true` for the former, `false` for the latter, so
/// `hid_writer_loop` knows whether to rescan for the wheel or stop for
/// good.
fn run_led_writer(
    device: hidapi::HidDevice,
    proto: &dyn WheelLedProtocol,
    rx: &std::sync::mpsc::Receiver<LedBarState>,
) -> bool {
    let mut blink_on = false;
    let mut last_blink_toggle = Instant::now();
    let mut current_state = LedBarState::default();
    let mut consecutive_write_failures = 0u32;

    // Block waiting for the next desired state, but with a short timeout so
    // an in-progress blink keeps animating even without fresh telemetry.
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(state) => current_state = state,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let _ = write_wheel_frame(&device, &proto.encode_leds(0));
                return true;
            }
        }

        let bits = if current_state.blink {
            if last_blink_toggle.elapsed() > Duration::from_millis(100) {
                blink_on = !blink_on;
                last_blink_toggle = Instant::now();
            }
            if blink_on {
                current_state.bits
            } else {
                0
            }
        } else {
            current_state.bits
        };

        match write_wheel_frame(&device, &proto.encode_leds(bits)) {
            Ok(_) => consecutive_write_failures = 0,
            Err(e) => {
                consecutive_write_failures += 1;
                tracing::warn!(error = %e, "logitech_led: failed to write HID report");
                if consecutive_write_failures >= 5 {
                    tracing::warn!(
                        "logitech_led: wheel appears to have disconnected, will rescan"
                    );
                    return false;
                }
            }
        }
    }
}
