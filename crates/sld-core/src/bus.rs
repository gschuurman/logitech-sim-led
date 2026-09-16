//! The telemetry event bus: a single `tokio::sync::broadcast` channel that
//! every enabled source publishes onto and every enabled output subscribes
//! to independently. Fan-out is what gives us "n games -> 1..m outputs"
//! without sources and outputs knowing about each other.

use crate::traits::{TelemetryRx, TelemetryTx};
use tokio::sync::broadcast;

/// Default channel capacity: frames buffered per-subscriber before a slow
/// subscriber starts lagging (see `broadcast::error::RecvError::Lagged`).
/// At ~60 frames/sec this is a few seconds of headroom.
pub const DEFAULT_CAPACITY: usize = 256;

pub fn new_bus(capacity: usize) -> (TelemetryTx, TelemetryRx) {
    broadcast::channel(capacity)
}
