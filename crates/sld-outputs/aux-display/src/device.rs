use crate::protocol::AuxFrame;
use crate::transport::AuxTransport;
use async_trait::async_trait;
use sld_core::shutdown::ShutdownSignal;
use sld_core::telemetry::TelemetryFrame;
use sld_core::traits::{OutputDevice, TelemetryRx};
use tokio::sync::broadcast;

pub struct AuxDisplayOutput {
    transport: Box<dyn AuxTransport>,
    update_interval: std::time::Duration,
}

impl AuxDisplayOutput {
    pub fn new(transport: Box<dyn AuxTransport>, update_rate_hz: f32) -> Self {
        let hz = update_rate_hz.max(0.1);
        Self {
            transport,
            update_interval: std::time::Duration::from_secs_f32(1.0 / hz),
        }
    }
}

#[async_trait]
impl OutputDevice for AuxDisplayOutput {
    fn id(&self) -> &str {
        "aux_display"
    }

    async fn run(
        &mut self,
        mut rx: TelemetryRx,
        mut shutdown: ShutdownSignal,
    ) -> anyhow::Result<()> {
        let mut ticker = tokio::time::interval(self.update_interval);
        let mut latest: Option<TelemetryFrame> = None;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                frame = rx.recv() => {
                    match frame {
                        Ok(f) => latest = Some(f),
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
                _ = ticker.tick() => {
                    if let Some(f) = &latest {
                        let aux = AuxFrame::from(f);
                        if let Err(e) = self.transport.send_line(&aux.to_line()).await {
                            tracing::warn!(error = %e, "aux display: failed to send frame");
                        }
                    }
                }
            }
        }
    }
}
