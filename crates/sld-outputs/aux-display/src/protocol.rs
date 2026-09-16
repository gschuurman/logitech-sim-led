//! Wire protocol for the auxiliary display. Kept intentionally simple: one
//! JSON object per line (newline-delimited JSON) so it's trivial to parse
//! on a microcontroller with ArduinoJson, and easy to inspect/debug with
//! any serial terminal. See docs/aux-display-protocol.md for the schema
//! reference and firmware/esp32-aux-display for a parser that consumes it.
//!
//! This is intentionally a small subset of `TelemetryFrame`, not the whole
//! thing -- the aux display doesn't need (and shouldn't have to parse)
//! every field every source might ever populate.

use serde::{Deserialize, Serialize};
use sld_core::telemetry::TelemetryFrame;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxFrame {
    pub rpm: f32,
    pub rpm_max: f32,
    pub speed_kph: f32,
    pub gear: i8,
    pub fuel: Option<f32>,
    pub lap: Option<u32>,
    pub position: Option<u32>,
}

impl From<&TelemetryFrame> for AuxFrame {
    fn from(f: &TelemetryFrame) -> Self {
        Self {
            rpm: f.rpm,
            rpm_max: f.rpm_max,
            speed_kph: f.speed_mps * 3.6,
            gear: f.gear,
            fuel: f.fuel,
            lap: f.lap,
            position: f.position,
        }
    }
}

impl AuxFrame {
    /// Serialize as a single line of JSON terminated with `\n`, ready to
    /// write directly to a serial port or socket.
    pub fn to_line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_default();
        s.push('\n');
        s
    }
}
