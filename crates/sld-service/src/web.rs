//! Minimal local status UI: one page showing live telemetry over a
//! websocket, served on localhost only. This is a foundation -- config
//! editing, plugin enable/disable, and an LED-curve preview are natural
//! next additions on top of this router; the websocket + broadcast-fanout
//! pattern here is what they'd build on.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use sld_core::shutdown::ShutdownSignal;
use sld_core::telemetry::TelemetryFrame;
use sld_core::traits::TelemetryRx;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<TelemetryFrame>,
}

pub async fn serve(
    bind_addr: &str,
    rx: TelemetryRx,
    mut shutdown: ShutdownSignal,
) -> anyhow::Result<()> {
    // The bus receiver we're handed can only be subscribed to once; re-fan
    // it out into a local broadcast channel so every websocket client that
    // connects gets its own subscription.
    let (local_tx, _local_rx) = broadcast::channel(64);
    let state = Arc::new(AppState {
        tx: local_tx.clone(),
    });

    let forward_shutdown = shutdown.clone();
    tokio::spawn(forward_frames(rx, local_tx, forward_shutdown));

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .route("/api/test-leds", post(test_leds_handler))
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
