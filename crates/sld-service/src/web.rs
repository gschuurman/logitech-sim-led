//! Minimal local status UI: one page showing live telemetry over a
//! websocket, served on localhost only. This is a foundation -- config
//! editing, plugin enable/disable, and an LED-curve preview are natural
//! next additions on top of this router; the websocket + broadcast-fanout
//! pattern here is what they'd build on.

use crate::registry::LiveOutputHandles;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sld_core::config::AppConfig;
use sld_core::shutdown::{ShutdownHandle, ShutdownSignal};
use sld_core::telemetry::TelemetryFrame;
use sld_core::traits::TelemetryRx;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<TelemetryFrame>,
    config: Arc<RwLock<AppConfig>>,
    live: LiveOutputHandles,
    shutdown_handle: ShutdownHandle,
}

pub async fn serve(
    bind_addr: &str,
    rx: TelemetryRx,
    mut shutdown: ShutdownSignal,
    config: Arc<RwLock<AppConfig>>,
    live: LiveOutputHandles,
    shutdown_handle: ShutdownHandle,
) -> anyhow::Result<()> {
    // The bus receiver we're handed can only be subscribed to once; re-fan
    // it out into a local broadcast channel so every websocket client that
    // connects gets its own subscription.
    let (local_tx, _local_rx) = broadcast::channel(64);
    let state = Arc::new(AppState {
        tx: local_tx.clone(),
        config,
        live,
        shutdown_handle,
    });

    let forward_shutdown = shutdown.clone();
    tokio::spawn(forward_frames(rx, local_tx, forward_shutdown));

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .route("/api/test-leds", post(test_leds_handler))
        .route("/api/settings", get(get_settings).post(post_settings))
        .route("/api/quit", post(quit_handler))
        .route("/favicon.png", get(favicon))
        .route("/logo.png", get(logo))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(%bind_addr, "web UI listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.cancelled().await;
        })
        .await?;

    Ok(())
}

async fn forward_frames(
    mut rx: TelemetryRx,
    local_tx: broadcast::Sender<TelemetryFrame>,
    mut shutdown: ShutdownSignal,
) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            frame = rx.recv() => match frame {
                Ok(f) => { let _ = local_tx.send(f); }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../assets/index.html"))
}

async fn favicon() -> impl IntoResponse {
    (
        [("content-type", "image/png")],
        include_bytes!("../assets/icons/icon-32.png").as_slice(),
    )
}

async fn logo() -> impl IntoResponse {
    (
        [("content-type", "image/png")],
        include_bytes!("../assets/icons/icon-128.png").as_slice(),
    )
}

/// Finds, opens, and briefly flashes a wheel's LEDs -- same code path as
/// `sld-cli test-leds` (see `sld-output-logitech-hid::device::test_leds`
/// and docs/led-hid-protocol.md for why this is the right way to poke at
/// the wheel rather than a hand-rolled write here). Runs on a blocking
/// thread since it does synchronous HID I/O plus a ~1.6s sleep to make the
/// flash actually visible.
async fn test_leds_handler() -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(sld_output_logitech_hid::test_leds).await;
    let body = match result {
        Ok(Ok(protocol)) => serde_json::json!({
            "ok": true,
            "message": format!("LEDs flashed (protocol '{protocol}')"),
        }),
        Ok(Err(e)) => serde_json::json!({ "ok": false, "message": e.to_string() }),
        Err(e) => serde_json::json!({ "ok": false, "message": format!("task panicked: {e}") }),
    };
    Json(body)
}

/// Same `ShutdownHandle` the GUI's tray "Quit" item and close-button (when
/// minimize-to-tray is off) use -- see gui.rs, which polls
/// `shutdown_signal.is_shutdown()` each tick specifically to notice a
/// shutdown triggered from here and exit the window/tray too, not just
/// the async service this axum server itself is part of.
async fn quit_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    tracing::info!("quit requested from dashboard");
    state.shutdown_handle.shutdown();
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Serialize)]
struct SettingsView {
    shift_point_pct: f32,
    full_bar_pct: f32,
    blink_at_redline: bool,
    minimize_to_tray_on_close: bool,
}

#[derive(Deserialize)]
struct SettingsUpdate {
    shift_point_pct: f32,
    full_bar_pct: f32,
    blink_at_redline: bool,
    minimize_to_tray_on_close: bool,
}

async fn get_settings(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cfg = state.config.read().unwrap();
    let led = cfg.outputs.logitech_led.clone().unwrap_or_default();
    Json(SettingsView {
        shift_point_pct: led.shift_point_pct,
        full_bar_pct: led.full_bar_pct,
        blink_at_redline: led.blink_at_redline,
        minimize_to_tray_on_close: cfg.app.minimize_to_tray_on_close,
    })
}

/// Applies a settings change immediately (the running LED output re-reads
/// its curve every ~33ms tick, and the GUI's close handler re-reads
/// `minimize_to_tray_on_close` on every close -- see device.rs/gui.rs)
/// and persists it to the per-user config file (see
/// `crate::config::save`), so it survives a restart too.
async fn post_settings(
    State(state): State<Arc<AppState>>,
    Json(update): Json<SettingsUpdate>,
) -> impl IntoResponse {
    let pct_range = 0.0..=1.0;
    if !pct_range.contains(&update.shift_point_pct) || !pct_range.contains(&update.full_bar_pct) {
        return Json(serde_json::json!({
            "ok": false,
            "message": "percentages must be between 0 and 100",
        }));
    }
    if update.full_bar_pct <= update.shift_point_pct {
        return Json(serde_json::json!({
            "ok": false,
            "message": "\"fully on\" percentage must be greater than \"start flashing\" percentage",
        }));
    }

    {
        let mut cfg = state.config.write().unwrap();
        let led = cfg
            .outputs
            .logitech_led
            .get_or_insert_with(Default::default);
        led.shift_point_pct = update.shift_point_pct;
        led.full_bar_pct = update.full_bar_pct;
        led.blink_at_redline = update.blink_at_redline;
        cfg.app.minimize_to_tray_on_close = update.minimize_to_tray_on_close;
    }

    if let Some(curve) = &state.live.logitech_led_curve {
        let mut curve = curve.write().unwrap();
        curve.shift_point_pct = update.shift_point_pct;
        curve.full_bar_pct = update.full_bar_pct;
        curve.blink_at_redline = update.blink_at_redline;
    }

    let save_result = crate::config::save(&state.config.read().unwrap());
    match save_result {
        Ok(path) => Json(serde_json::json!({
            "ok": true,
            "message": format!("Saved to {}", path.display()),
        })),
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "message": format!("Applied, but failed to save to disk: {e}"),
        })),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.tx.subscribe();
    while let Ok(frame) = rx.recv().await {
        let payload = match serde_json::to_string(&frame) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if socket.send(Message::Text(payload)).await.is_err() {
            break;
        }
    }
}
