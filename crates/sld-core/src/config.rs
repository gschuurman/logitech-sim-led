//! Shared config schema, loaded from TOML by `sld-service`. Lives in
//! `sld-core` (rather than the service crate) so source/output crates could
//! eventually own and validate their own config sub-sections without a
//! dependency inversion. Every field has a default so an empty/missing file
//! -- or a missing section within it -- degrades gracefully instead of
//! failing to parse; see config/default.toml for the documented example.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub sources: SourcesConfig,
    #[serde(default)]
    pub outputs: OutputsConfig,
    #[serde(default = "default_bus_capacity")]
    pub bus_capacity: usize,
}

fn default_bus_capacity() -> usize {
    crate::bus::DEFAULT_CAPACITY
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            sources: SourcesConfig::default(),
            outputs: OutputsConfig::default(),
            bus_capacity: default_bus_capacity(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourcesConfig {
    #[serde(default)]
    pub forza: Option<ForzaSourceConfig>,
    // Future sources (iRacing, ACC, BeamNG, ...) get their own optional
    // section here -- see docs/adding-a-game-source.md.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForzaSourceConfig {
    /// UDP address to bind and listen on for Forza's "Data Out" feature.
    #[serde(default = "default_forza_bind")]
    pub bind_addr: String,
}

fn default_forza_bind() -> String {
    "0.0.0.0:5300".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputsConfig {
    #[serde(default)]
    pub logitech_led: Option<LogitechLedConfig>,
    // Future outputs (a second display, an OBS overlay, ...) get their own
    // optional section here -- see docs/adding-an-output.md.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogitechLedConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Fraction (0.0-1.0) of the idle->max RPM range at which the first LED
    /// turns on.
    #[serde(default = "default_shift_pct")]
    pub shift_point_pct: f32,
    #[serde(default = "default_blink")]
    pub blink_at_redline: bool,
}

fn default_true() -> bool {
    true
}
fn default_shift_pct() -> f32 {
    0.85
}
fn default_blink() -> bool {
    true
}
