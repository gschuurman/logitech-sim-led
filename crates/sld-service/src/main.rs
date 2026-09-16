//! The daemon: loads config, wires up every enabled source and output
//! through the shared bus, and runs until Ctrl-C (or the tray "Quit" item,
//! once that scaffold is finished -- see tray.rs).

mod config;
mod registry;
#[cfg(feature = "tray")]
mod tray;
#[cfg(feature = "web")]
mod web;

use sld_core::bus;
use sld_core::shutdown::ShutdownHandle;
use tracing_subscriber::EnvFilter;

const CONFIG_PATH: &str = "config/default.toml";
const WEB_BIND_ADDR: &str = "127.0.0.1:5301";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = config::load_or_default(CONFIG_PATH)?;
    let (bus_tx, _bus_rx) = bus::new_bus(cfg.bus_capacity);
    let (shutdown_handle, shutdown_signal) = ShutdownHandle::new();

    let sources = registry::build_sources(&cfg);
    let outputs = registry::build_outputs(&cfg);

    tracing::info!(
        sources = sources.len(),
        outputs = outputs.len(),
        "starting logitech-sim-led service"
    );
    if sources.is_empty() {
        tracing::warn!("no telemetry sources enabled -- see config/default.toml");
    }
    if outputs.is_empty() {
        tracing::warn!("no output devices enabled -- see config/default.toml");
    }

    let mut tasks = tokio::task::JoinSet::new();

    for mut source in sources {
        let tx = bus_tx.clone();
        let shutdown = shutdown_signal.clone();
        tasks.spawn(async move {
            let id = source.id().to_string();
            if let Err(e) = source.run(tx, shutdown).await {
                tracing::error!(source = %id, error = %e, "telemetry source exited with error");
            }
        });
    }

    for mut output in outputs {
        let rx = bus_tx.subscribe();
        let shutdown = shutdown_signal.clone();
        tasks.spawn(async move {
            let id = output.id().to_string();
            if let Err(e) = output.run(rx, shutdown).await {
                tracing::error!(output = %id, error = %e, "output device exited with error");
            }
        });
    }

    #[cfg(feature = "web")]
    {
        let rx = bus_tx.subscribe();
        let shutdown = shutdown_signal.clone();
        tasks.spawn(async move {
            if let Err(e) = web::serve(WEB_BIND_ADDR, rx, shutdown).await {
                tracing::error!(error = %e, "web UI server exited with error");
            }
        });
    }

    tokio::signal::ctrl_c().await?;
    tracing::info!("ctrl-c received, shutting down");
    shutdown_handle.shutdown();

    while tasks.join_next().await.is_some() {}

    Ok(())
}
