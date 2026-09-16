//! `TelemetrySource` implementation for Forza Horizon 5/6 and Forza
//! Motorsport, via the games' built-in "Data Out" UDP telemetry feature
//! (Settings > HUD and Gameplay > Data Out). All three titles share the
//! same wire format lineage, so one parser covers all of them; see
//! `packet` for the byte layout and docs/telemetry-protocol-forza.md for
//! how to point a game at this source and how to verify offsets for your
//! specific title/version.

pub mod packet;
pub mod source;

pub use source::ForzaSource;
