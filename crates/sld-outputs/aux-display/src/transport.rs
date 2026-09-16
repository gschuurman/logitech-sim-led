//! How `AuxFrame` lines actually get to the display. Serial is the only
//! real transport today (matching a USB-attached ESP32 acting as a CDC
//! device); a WiFi/TCP or BLE transport can be added later as another
//! `AuxTransport` impl without touching `device.rs` or the protocol.

use async_trait::async_trait;

#[async_trait]
pub trait AuxTransport: Send + Sync {
    async fn send_line(&mut self, line: &str) -> anyhow::Result<()>;
}

pub struct SerialTransport {
    port: tokio_serial::SerialStream,
}

impl SerialTransport {
    pub fn open(path: &str, baud_rate: u32) -> anyhow::Result<Self> {
        use tokio_serial::SerialPortBuilderExt;
        let port = tokio_serial::new(path, baud_rate).open_native_async()?;
        Ok(Self { port })
    }
}

#[async_trait]
impl AuxTransport for SerialTransport {
    async fn send_line(&mut self, line: &str) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        self.port.write_all(line.as_bytes()).await?;
        Ok(())
    }
}

/// No-op-ish transport used when no aux display is configured or connected
/// -- logs frames instead of sending them. Lets the output run (and be
/// tested) without hardware attached.
pub struct LoggingTransport;

#[async_trait]
impl AuxTransport for LoggingTransport {
    async fn send_line(&mut self, line: &str) -> anyhow::Result<()> {
        tracing::debug!(line = line.trim_end(), "aux-display (logging transport)");
        Ok(())
    }
}
