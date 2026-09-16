//! The two ports of the hexagon: [`TelemetrySource`] (inbound -- a game
//! feeding the bus) and [`OutputDevice`] (outbound -- something consuming
//! the bus). Every game integration implements only `TelemetrySource`;
//! every physical/virtual output implements only `OutputDevice`. Neither
//! side depends on the other, and `sld-service` is the only place that
//! wires concrete instances of both together (see
//! docs/adding-a-game-source.md and docs/adding-an-output.md).

use crate::shutdown::ShutdownSignal;
use crate::telemetry::TelemetryFrame;
use async_trait::async_trait;
use tokio::sync::broadcast;

pub type TelemetryTx = broadcast::Sender<TelemetryFrame>;
pub type TelemetryRx = broadcast::Receiver<TelemetryFrame>;

/// Something that ingests telemetry from a game/sim and publishes
/// normalized [`TelemetryFrame`]s onto the bus.
///
/// `run` should loop until `shutdown` resolves, publishing frames as they
/// arrive. Implementations own their own reconnect/retry logic -- a source
/// erroring out is logged by the caller but does not bring down the rest of
/// the service.
#[async_trait]
pub trait TelemetrySource: Send + Sync {
    /// Stable id used in logs and config, e.g. "forza".
    fn id(&self) -> &str;

    async fn run(&mut self, tx: TelemetryTx, shutdown: ShutdownSignal) -> anyhow::Result<()>;
}

/// Something that consumes normalized [`TelemetryFrame`]s and drives a
/// physical or virtual output (wheel LEDs, an aux display, an overlay,
/// logging to disk, ...).
///
/// `run` should loop until `shutdown` resolves. Each output gets its own
/// `broadcast::Receiver`, so a slow output can't block others or the
/// sources -- at worst it lags and drops frames (see
/// `tokio::sync::broadcast`'s `Lagged` semantics).
#[async_trait]
pub trait OutputDevice: Send + Sync {
    /// Stable id used in logs and config, e.g. "logitech_led".
    fn id(&self) -> &str;

    async fn run(&mut self, rx: TelemetryRx, shutdown: ShutdownSignal) -> anyhow::Result<()>;
}
