//! Developer/debug utilities. Not part of the running service -- this is
//! for bring-up: finding your wheel's vendor/product id, and capturing raw
//! Forza packets to verify/calibrate the parser in `sld-source-forza`
//! against your actual game/title/version (see
//! docs/telemetry-protocol-forza.md).

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "sld-cli",
    about = "Developer/debug utilities for logitech-sim-led"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected HID devices (use this to find your wheel's vendor/product id).
    ListHidDevices,
    /// Listen for raw Forza "Data Out" UDP packets and print their length,
    /// a hex preview, and the parsed Sled/Dash fields.
    CaptureForza {
        #[arg(long, default_value = "0.0.0.0:5300")]
        bind: String,
        #[arg(long, default_value_t = 10)]
        count: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let cli = Cli::parse();

    match cli.command {
        Command::ListHidDevices => list_hid_devices()?,
        Command::CaptureForza { bind, count } => capture_forza(&bind, count).await?,
    }

    Ok(())
}

fn list_hid_devices() -> anyhow::Result<()> {
    let api = hidapi::HidApi::new()?;
    println!("{:<8} {:<8} {:<30} PATH", "VID", "PID", "PRODUCT");
    for dev in api.device_list() {
        println!(
            "{:#06x}  {:#06x}  {:<30} {}",
            dev.vendor_id(),
            dev.product_id(),
            dev.product_string().unwrap_or("<unknown>"),
            dev.path().to_string_lossy()
        );
    }
    Ok(())
}

async fn capture_forza(bind: &str, count: usize) -> anyhow::Result<()> {
    let socket = tokio::net::UdpSocket::bind(bind).await?;
    println!("listening on {bind} for {count} packets... (point the game's Data Out setting here)");

    let mut buf = [0u8; 1500];
    for i in 0..count {
        let (len, peer) = socket.recv_from(&mut buf).await?;
        let preview_len = len.min(64);
        println!(
            "[{i}] from {peer} len={len} bytes  first {preview_len} bytes: {}",
            hex_preview(&buf[..preview_len])
        );

        match sld_source_forza::packet::parse(&buf[..len]) {
            Some(pkt) => {
                println!(
                    "      sled: is_race_on={} rpm={:.0}/{:.0} (idle {:.0})",
                    pkt.sled.is_race_on,
                    pkt.sled.current_engine_rpm,
                    pkt.sled.engine_max_rpm,
                    pkt.sled.engine_idle_rpm
                );
                if let Some(dash) = &pkt.dash {
                    println!(
                        "      dash: speed={:.1} m/s gear={} fuel={:.2}",
                        dash.speed_mps, dash.gear, dash.fuel
                    );
                }
                if let Some(tw) = &pkt.tire_wear {
                    println!(
                        "      tire_wear (UNVERIFIED offsets): fl={:.3} fr={:.3} rl={:.3} rr={:.3}",
                        tw.front_left, tw.front_right, tw.rear_left, tw.rear_right
                    );
                }
            }
            None => println!("      (too short to parse as Sled format)"),
        }
    }

    Ok(())
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
