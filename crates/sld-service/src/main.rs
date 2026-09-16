//! Entrypoint. Two shapes, both wiring up the same `run_service`:
//!
//! - `gui` feature (on by default): a native tray app, no console window
//!   -- see `gui.rs`.
//! - without it: a plain console/service-manager-friendly process that
//!   runs until Ctrl-C, for environments where a window and tray icon
//!   make no sense (e.g. under systemd). Build with
//!   `--no-default-features --features web` for this.

// No console window in GUI builds on Windows (a visible terminal behind a
// tray app looks unfinished, and there's no console to log to anyway --
// see gui.rs's init_tracing). Console builds keep the normal console
// subsystem so `tracing_subscriber::fmt()` output is actually visible.
#![cfg_attr(
    all(feature = "gui", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(feature = "gui")]
mod autostart;
mod config;
#[cfg(feature = "gui")]
mod gui;
mod registry;
#[cfg(feature = "web")]
mod web;

use sld_core::bus;
use sld_core::config::AppConfig;
use sld_core::shutdown::ShutdownSignal;
use std::sync::{Arc, RwLock};

const WEB_BIND_ADDR: &str = "127.0.0.1:5301";

/// Wires up every enabled source and output through the shared bus, and
/// runs until `shutdown` is cancelled. Shared by the plain console
/// entrypoint (which cancels it on Ctrl-C) and the GUI's background tokio
/// thread (which cancels it from the tray "Quit" item).
///
/// `shared_cfg` is loaded by the caller (not here) specifically so it can
/// also be handed to `gui::run()`'s close-button handler and to
/// `web.rs`'s settings API -- one shared, live-mutable source of truth
/// for config, not a snapshot each of those would otherwise load
/// separately and drift out of sync.
async fn run_service(
    shared_cfg: Arc<RwLock<AppConfig>>,
    mut shutdown: ShutdownSignal,
) -> anyhow::Result<()> {
    let (bus_capacity, sources, outputs, live_outputs) = {
        let cfg = shared_cfg.read().unwrap();
        let (outputs, live_outputs) = registry::build_outputs(&cfg);
        (
            cfg.bus_capacity,
            registry::build_sources(&cfg),
            outputs,
            live_outputs,
        )
    };
    let (bus_tx, _bus_rx) = bus::new_bus(bus_capacity);

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
        let shutdown = shutdown.clone();
        tasks.spawn(async move {
            let id = source.id().to_string();
            if let Err(e) = source.run(tx, shutdown).await {
                tracing::error!(source = %id, error = %e, "telemetry source exited with error");
            }
        });
    }

    for mut output in outputs {
        let rx = bus_tx.subscribe();
        let shutdown = shutdown.clone();
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
        let shutdown = shutdown.clone();
        let shared_cfg = Arc::clone(&shared_cfg);
        tasks.spawn(async move {
            if let Err(e) = web::serve(WEB_BIND_ADDR, rx, shutdown, shared_cfg, live_outputs).await
            {
                tracing::error!(error = %e, "web UI server exited with error");
            }
        });
    }
    #[cfg(not(feature = "web"))]
    let _ = live_outputs;

    shutdown.cancelled().await;
    tracing::info!("shutting down");

    while tasks.join_next().await.is_some() {}

    Ok(())
}

#[cfg(feature = "gui")]
fn main() -> anyhow::Result<()> {
    gui::run()
}

#[cfg(not(feature = "gui"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use sld_core::shutdown::ShutdownHandle;
    use tracing_subscriber::EnvFilter;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let shared_cfg = Arc::new(RwLock::new(config::load_default_config()?));
    let (shutdown_handle, shutdown_signal) = ShutdownHandle::new();
    let service = tokio::spawn(run_service(shared_cfg, shutdown_signal));

    tokio::signal::ctrl_c().await?;
    tracing::info!("ctrl-c received, shutting down");
    shutdown_handle.shutdown();

    service.await??;
    Ok(())
}
