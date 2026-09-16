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
    /// Find, open, and briefly flash the RPM LEDs on a supported wheel --
    /// use this to check the LED path in isolation, without the game or
    /// service running.
    TestLeds,
    /// Read-only: dump the raw HID report descriptor for every interface of
    /// a given vendor/product id, so report shapes can be checked before
    /// writing anything to the device.
    DumpDescriptor {
        #[arg(long, value_parser = parse_hex_u16)]
        vid: u16,
        #[arg(long, value_parser = parse_hex_u16)]
        pid: u16,
    },
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
        Command::TestLeds => test_leds()?,
        Command::DumpDescriptor { vid, pid } => dump_descriptor(vid, pid)?,
        Command::CaptureForza { bind, count } => capture_forza(&bind, count).await?,
    }

    Ok(())
}

fn list_hid_devices() -> anyhow::Result<()> {
    let api = hidapi::HidApi::new()?;
    println!(
        "{:<8} {:<8} {:<8} {:<8} {:<30} PATH",
        "VID", "PID", "USAGEPG", "USAGE", "PRODUCT"
    );
    for dev in api.device_list() {
        println!(
            "{:#06x}  {:#06x}  {:#06x}  {:#06x}  {:<30} {}",
            dev.vendor_id(),
            dev.product_id(),
            dev.usage_page(),
            dev.usage(),
            dev.product_string().unwrap_or("<unknown>"),
            dev.path().to_string_lossy()
        );
    }
    Ok(())
}

fn test_leds() -> anyhow::Result<()> {
    let proto = sld_output_logitech_hid::test_leds()?;
    println!("wheel found and LEDs flashed successfully using protocol '{proto}'");
    Ok(())
}

fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u16::from_str_radix(s, 16).map_err(|e| e.to_string())
}

/// Read-only: no writes to the device, just fetches and hex-dumps each
/// matching interface's report descriptor.
fn dump_descriptor(vid: u16, pid: u16) -> anyhow::Result<()> {
    let api = hidapi::HidApi::new()?;
    let mut buf = [0u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
    for dev_info in api.device_list() {
        if dev_info.vendor_id() != vid || dev_info.product_id() != pid {
            continue;
        }
        println!(
            "--- usage_page={:#06x} usage={:#06x} path={}",
            dev_info.usage_page(),
            dev_info.usage(),
            dev_info.path().to_string_lossy()
        );
        let dev = match dev_info.open_device(&api) {
            Ok(d) => d,
            Err(e) => {
                println!("    open failed: {e}");
                continue;
            }
        };
        match dev.get_report_descriptor(&mut buf) {
            Ok(n) => {
                for chunk in buf[..n].chunks(16) {
                    let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                    println!("    {}", hex.join(" "));
                }
            }
            Err(e) => println!("    get_report_descriptor failed: {e}"),
        }
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
                match &pkt.extras {
                    sld_source_forza::packet::TitleExtras::Horizon {
                        car_group,
                        smashable_vel_diff,
                        smashable_mass,
                    } => {
                        println!(
                            "      horizon extras: car_group={car_group} smashable_vel_diff={smashable_vel_diff:.2} smashable_mass={smashable_mass:.1}"
                        );
                    }
                    sld_source_forza::packet::TitleExtras::Motorsport {
                        tire_wear,
                        track_ordinal,
                    } => {
                        println!(
                            "      motorsport extras: track_ordinal={track_ordinal} tire_wear fl={:.3} fr={:.3} rl={:.3} rr={:.3}",
                            tire_wear.front_left, tire_wear.front_right, tire_wear.rear_left, tire_wear.rear_right
                        );
                    }
                    sld_source_forza::packet::TitleExtras::None => {}
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
