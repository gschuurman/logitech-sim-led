//! Foundation for driving a second, auxiliary telemetry display -- e.g. an
//! ESP32 with its own screen sitting next to the wheel. This crate is
//! deliberately the least "finished" part of the workspace: it establishes
//! the seam (transport trait + wire schema + `OutputDevice` impl) so a real
//! display can be built against a stable contract, without blocking that
//! work on picking exact hardware today.
//!
//! - `protocol`: the normalized, newline-delimited JSON wire schema.
//! - `transport`: how bytes get to the display (serial today; swap in
//!   WiFi/BLE/USB-CDC-at-a-different-baud later without touching anything
//!   else).
//! - `device`: the `OutputDevice` impl, rate-limited independently of the
//!   telemetry source's own update rate.
//!
//! See docs/aux-display-protocol.md and firmware/esp32-aux-display for the
//! matching microcontroller-side stub.

pub mod device;
pub mod protocol;
pub mod transport;

pub use device::AuxDisplayOutput;
