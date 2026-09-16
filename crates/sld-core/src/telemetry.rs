//! The normalized telemetry model. Every `TelemetrySource` translates its
//! game-specific packet/shared-memory format into one of these; every
//! `OutputDevice` consumes only this type and never needs to know which
//! game produced it.
//!
//! Fields that aren't universal across sims (tire temps, boost, wear, ...)
//! don't get a dedicated struct field -- they go in `extra` so adding a
//! field for one game never forces a breaking change on every other source
//! and output. Promote a field out of `extra` into a named field only once
//! two or more sources agree on its meaning and unit.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Id of the `TelemetrySource` that produced this frame, e.g. "forza".
    pub source: String,

    /// Specific title/variant, e.g. "forza_horizon_5", "forza_motorsport".
    /// Optional because not every source can (or needs to) distinguish.
    pub game: Option<String>,

    /// Wall-clock capture time, milliseconds since Unix epoch.
    pub timestamp_ms: u64,

    /// Whether a session/race is actively running (vs. paused, in a menu,
    /// or otherwise producing telemetry that shouldn't drive outputs).
    pub is_running: bool,

    pub rpm: f32,
    pub rpm_idle: f32,
    pub rpm_max: f32,

    pub speed_mps: f32,

    /// Raw gear number as reported by the source. Convention varies by
    /// game -- see that source's doc comment / docs/ page. For Forza:
    /// 0 = Reverse, 1+ = forward gears (unverified below the Sled block;
    /// confirm with `sld-cli capture-forza` against your title/version).
    pub gear: i8,

    /// 0.0 - 1.0
    pub throttle: f32,
    /// 0.0 - 1.0
    pub brake: f32,
    /// 0.0 - 1.0
    pub clutch: f32,
    /// -1.0 (full left) - 1.0 (full right)
    pub steer: f32,

    /// 0.0 - 1.0, when the source reports it.
    pub fuel: Option<f32>,
    pub lap: Option<u32>,
    pub lap_time_s: Option<f32>,
    pub position: Option<u32>,

    /// Game-specific fields that don't (yet) have a common home: tire
    /// temps/wear, boost, power/torque, car ids, etc. Keys are
    /// `snake_case` and documented by the source that populates them.
    pub extra: HashMap<String, f32>,
}
