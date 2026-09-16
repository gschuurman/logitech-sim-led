//! Turns config into concrete `TelemetrySource`/`OutputDevice` instances.
//! This is the **one and only** place in the whole workspace that needs to
//! know about every concrete source and output crate -- adding a new game
//! or output means adding a few lines here (and a config section in
//! `sld-core::config`), never touching `sld-core` itself or any other
//! source/output. See docs/adding-a-game-source.md and
//! docs/adding-an-output.md.

use sld_core::config::AppConfig;
use sld_core::traits::{OutputDevice, TelemetrySource};
use sld_output_logitech_hid::curve::ShiftLightCurve;
use sld_output_logitech_hid::device::LogitechLedOutput;
use sld_source_forza::ForzaSource;
use std::sync::{Arc, RwLock};

pub fn build_sources(cfg: &AppConfig) -> Vec<Box<dyn TelemetrySource>> {
    let mut sources: Vec<Box<dyn TelemetrySource>> = Vec::new();

    if let Some(forza_cfg) = &cfg.sources.forza {
        sources.push(Box::new(ForzaSource::new(forza_cfg.bind_addr.clone())));
    }

    // Future sources register here, e.g.:
    //   if let Some(c) = &cfg.sources.iracing { sources.push(Box::new(IracingSource::new(c))); }

    sources
}

/// Handles onto live-updatable state for outputs that support it, keyed by
/// the same names `web.rs`'s settings API uses -- currently just the LED
/// shift-light curve, updated in lockstep with the shared `AppConfig`
/// whenever settings are saved (see `web::settings_handlers`).
#[derive(Default, Clone)]
pub struct LiveOutputHandles {
    pub logitech_led_curve: Option<Arc<RwLock<ShiftLightCurve>>>,
}

pub fn build_outputs(cfg: &AppConfig) -> (Vec<Box<dyn OutputDevice>>, LiveOutputHandles) {
    let mut outputs: Vec<Box<dyn OutputDevice>> = Vec::new();
    let mut live = LiveOutputHandles::default();

    if let Some(led_cfg) = &cfg.outputs.logitech_led {
        if led_cfg.enabled {
            let curve = ShiftLightCurve {
                shift_point_pct: led_cfg.shift_point_pct,
                full_bar_pct: led_cfg.full_bar_pct,
                blink_at_redline: led_cfg.blink_at_redline,
            };
            let output = LogitechLedOutput::new(curve);
            live.logitech_led_curve = Some(output.curve_handle());
            outputs.push(Box::new(output));
        }
    }

    // Future outputs register here, e.g.:
    //   if let Some(c) = &cfg.outputs.some_output { if c.enabled { outputs.push(...) } }

    (outputs, live)
}
