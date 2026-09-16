//! Turns config into concrete `TelemetrySource`/`OutputDevice` instances.
//! This is the **one and only** place in the whole workspace that needs to
//! know about every concrete source and output crate -- adding a new game
//! or output means adding a few lines here (and a config section in
//! `sld-core::config`), never touching `sld-core` itself or any other
//! source/output. See docs/adding-a-game-source.md and
//! docs/adding-an-output.md.

use sld_core::config::AppConfig;
use sld_core::traits::{OutputDevice, TelemetrySource};
use sld_output_aux_display::device::AuxDisplayOutput;
use sld_output_aux_display::transport::{AuxTransport, LoggingTransport, SerialTransport};
use sld_output_logitech_hid::curve::ShiftLightCurve;
use sld_output_logitech_hid::device::LogitechLedOutput;
use sld_source_forza::ForzaSource;

pub fn build_sources(cfg: &AppConfig) -> Vec<Box<dyn TelemetrySource>> {
    let mut sources: Vec<Box<dyn TelemetrySource>> = Vec::new();

    if let Some(forza_cfg) = &cfg.sources.forza {
        sources.push(Box::new(ForzaSource::new(forza_cfg.bind_addr.clone())));
    }

    // Future sources register here, e.g.:
    //   if let Some(c) = &cfg.sources.iracing { sources.push(Box::new(IracingSource::new(c))); }

    sources
}

pub fn build_outputs(cfg: &AppConfig) -> Vec<Box<dyn OutputDevice>> {
    let mut outputs: Vec<Box<dyn OutputDevice>> = Vec::new();

    if let Some(led_cfg) = &cfg.outputs.logitech_led {
        if led_cfg.enabled {
            let curve = ShiftLightCurve {
                shift_point_pct: led_cfg.shift_point_pct,
                blink_at_redline: led_cfg.blink_at_redline,
            };
            outputs.push(Box::new(LogitechLedOutput::new(curve)));
        }
    }

    if let Some(aux_cfg) = &cfg.outputs.aux_display {
        if aux_cfg.enabled {
            let transport: Box<dyn AuxTransport> = match &aux_cfg.serial_port {
                Some(port) => match SerialTransport::open(port, 115_200) {
                    Ok(t) => Box::new(t),
                    Err(e) => {
                        tracing::warn!(
                            error = %e, port,
                            "aux_display: failed to open serial port, falling back to logging transport"
                        );
                        Box::new(LoggingTransport)
                    }
                },
                None => Box::new(LoggingTransport),
            };
            outputs.push(Box::new(AuxDisplayOutput::new(
                transport,
                aux_cfg.update_rate_hz,
            )));
        }
    }

    outputs
}
