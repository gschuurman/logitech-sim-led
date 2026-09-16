//! `sld-core` defines the ports every plugin builds against: the normalized
//! [`telemetry::TelemetryFrame`], the [`traits::TelemetrySource`] /
//! [`traits::OutputDevice`] traits, the shared event bus, shutdown
//! signaling, and the config schema.
//!
//! This crate has (deliberately) no knowledge of Forza, Logitech wheels, or
//! anything else concrete -- every game integration and every output device
//! lives in its own crate and depends *inward* on this one. That's the seam
//! that makes "connect another game" or "drive another output" additive
//! work instead of surgery. See docs/architecture.md.

pub mod bus;
pub mod config;
pub mod shutdown;
pub mod telemetry;
pub mod traits;
