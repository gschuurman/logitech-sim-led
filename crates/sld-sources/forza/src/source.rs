use crate::packet;
use async_trait::async_trait;
use sld_core::shutdown::ShutdownSignal;
use sld_core::traits::{TelemetrySource, TelemetryTx};
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

/// Listens for Forza's "Data Out" UDP packets and republishes them as
/// normalized [`sld_core::telemetry::TelemetryFrame`]s.
///
/// Point the game's Settings > HUD and Gameplay > Data Out "IP Address"/
/// "Port" at the machine and port this is bound to.
pub struct ForzaSource {
    bind_addr: String,
    game_label: String,
}

impl ForzaSource {
    pub fn new(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            game_label: "forza".to_string(),
        }
    }

    /// Tag frames from this source with a specific title, e.g.
    /// "forza_horizon_5", "forza_horizon_6", "forza_motorsport". Useful once
    /// per-title quirks (e.g. the tire-wear extension) need distinguishing
    /// downstream. Defaults to the generic "forza".
    pub fn with_game_label(mut self, label: impl Into<String>) -> Self {
        self.game_label = label.into();
        self
    }
}

#[async_trait]
impl TelemetrySource for ForzaSource {
    fn id(&self) -> &str {
        "forza"
    }

    async fn run(&mut self, tx: TelemetryTx, mut shutdown: ShutdownSignal) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(&self.bind_addr).await?;
        info!(bind_addr = %self.bind_addr, "forza source listening for Data Out UDP packets");

        let mut buf = [0u8; 1500];
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    info!("forza source shutting down");
                    return Ok(());
                }
                result = socket.recv_from(&mut buf) => {
                    let (len, _peer) = match result {
                        Ok(v) => v,
                        Err(e) => {
                            warn!(error = %e, "forza source: recv error");
                            continue;
                        }
                    };

                    match packet::parse(&buf[..len]) {
                        Some(pkt) => {
                            if !pkt.sled.is_race_on {
                                continue;
                            }
                            let frame = pkt.to_telemetry_frame(&self.game_label);
                            // A send error just means no receivers are
                            // currently subscribed (e.g. no outputs
                            // enabled) -- not fatal, so we ignore it.
                            let _ = tx.send(frame);
                        }
                        None => {
                            debug!(len, "forza source: packet too short to parse as Data Out");
                        }
                    }
                }
            }
        }
    }
}
