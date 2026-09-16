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
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

pub struct LogitechLedOutput {
    curve: ShiftLightCurve,
    update_interval: Duration,
}

impl LogitechLedOutput {
    pub fn new(curve: ShiftLightCurve) -> Self {
        Self {
            curve,
            update_interval: Duration::from_millis(33), // ~30 Hz
        }
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

        let curve = self.curve;
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
                        let state = curve.evaluate(f.rpm, f.rpm_idle, f.rpm_max);
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

fn hid_writer_loop(rx: std::sync::mpsc::Receiver<LedBarState>) {
    let mut api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            tracing::error!(error = %e, "logitech_led: failed to initialize hidapi");
            return;
        }
    };

    let Some((device, proto)) = find_and_open_wheel(&mut api) else {
        return;
    };

    run_led_writer(device, proto.as_ref(), rx);
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
                && d.product_id() == switch.matches_product_id)
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
        if let Err(e) = dev.write(&switch.switch_report) {
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
            });
            if let Some(dev_info) = found {
                if let Some(proto) = protocols
                    .iter()
                    .find(|p| p.matches(dev_info.vendor_id(), dev_info.product_id()))
                {
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

fn run_led_writer(
    device: hidapi::HidDevice,
    proto: &dyn WheelLedProtocol,
    rx: std::sync::mpsc::Receiver<LedBarState>,
) {
    let mut blink_on = false;
    let mut last_blink_toggle = Instant::now();
    let mut current_state = LedBarState::default();

    // Block waiting for the next desired state, but with a short timeout so
    // an in-progress blink keeps animating even without fresh telemetry.
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(state) => current_state = state,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
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

        if let Err(e) = device.write(&proto.encode_leds(bits)) {
            tracing::warn!(error = %e, "logitech_led: failed to write HID report");
        }
    }

    let _ = device.write(&proto.encode_leds(0));
}
