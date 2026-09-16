//! `OutputDevice` implementation that drives the RPM shift-light LEDs on a
//! Logitech G-series wheel over raw USB HID -- no dependency on Logitech G
//! HUB being installed, which is what makes this work cross-platform
//! (Windows, Linux, macOS all have working `hidapi` backends).
//!
//! - `protocol`: per-wheel-family HID report encoding.
//! - `curve`: RPM -> LED bar state, independent of any specific wheel.
//! - `device`: ties them together, does the actual (blocking) HID I/O.
//!
//! See docs/led-hid-protocol.md for protocol provenance/confidence per
//! wheel model.

pub mod curve;
pub mod device;
pub mod protocol;

pub use device::LogitechLedOutput;
