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
use crate::protocol;
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
    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            tracing::error!(error = %e, "logitech_led: failed to initialize hidapi");
            return;
        }
    };

    let protocols = protocol::all_known_protocols();
    let mut found: Option<(hidapi::HidDevice, &'static str)> = None;

    'search: for dev_info in api.device_list() {
        for proto in &protocols {
            if proto.matches(dev_info.vendor_id(), dev_info.product_id()) {
                match dev_info.open_device(&api) {
                    Ok(dev) => {
                        tracing::info!(
                            protocol = proto.id(),
                            "opened Logitech wheel for LED output"
                        );
                        found = Some((dev, proto.id()));
                        break 'search;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, protocol = proto.id(), "found matching wheel but failed to open it");
                    }
                }
            }
        }
    }

    let Some((device, proto_id)) = found else {
        tracing::warn!(
            "logitech_led: no supported wheel found; run `sld-cli list-hid-devices` to check vendor/product ids"
        );
        return;
    };

    let proto = protocols
        .into_iter()
        .find(|p| p.id() == proto_id)
        .expect("proto_id came from this exact list");

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
