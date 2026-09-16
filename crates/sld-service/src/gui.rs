//! Native app entrypoint: no console window, a system tray icon, and a
//! window embedding the same dashboard `web.rs` already serves (over an
//! actual `wry` webview, not "go open your browser") -- so the telemetry
//! grid and the LED test button are the one UI, just presented natively
//! instead of as a bare background process.
//!
//! `tray-icon`/`wry` need to pump their event loop on the main OS thread,
//! which doesn't mesh with `#[tokio::main]` owning that thread instead:
//! the async service (telemetry sources, LED output, the axum server
//! behind the webview) runs on a `tokio::runtime::Runtime` driven from a
//! background thread, and this module owns the main thread's `tao`
//! `EventLoop` instead.

use crate::{autostart, config, run_service, WEB_BIND_ADDR};
use sld_core::shutdown::ShutdownHandle;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon as TrayIcon, TrayIconBuilder};

const ICON_BYTES: &[u8] = include_bytes!("../assets/icons/icon-256.png");

fn load_icon_rgba() -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(ICON_BYTES)?.into_rgba8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}

pub fn run() -> anyhow::Result<()> {
    init_tracing();

    // Shared with the background service thread (settings API reads/
    // writes it) and read directly below by the close-button handler, so
    // a settings change takes effect immediately without a restart.
    let shared_cfg = Arc::new(RwLock::new(config::load_default_config()?));

    let (shutdown_handle, shutdown_signal) = ShutdownHandle::new();

    // The async service (telemetry sources, LED output, the axum server
    // behind the webview) runs on its own thread/runtime -- this thread
    // is needed for tao's event loop below.
    let service_shutdown = shutdown_signal;
    let service_cfg = Arc::clone(&shared_cfg);
    let _service_thread = std::thread::Builder::new()
        .name("sld-service-async".into())
        .spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!(error = %e, "failed to start tokio runtime");
                    return;
                }
            };
            rt.block_on(async move {
                if let Err(e) = run_service(service_cfg, service_shutdown).await {
                    tracing::error!(error = %e, "service exited with error");
                }
            });
        })?;

    let event_loop: EventLoop<()> = EventLoop::new();

    let (rgba, icon_w, icon_h) = load_icon_rgba()?;
    let window_icon = tao::window::Icon::from_rgba(rgba.clone(), icon_w, icon_h).ok();
    let tray_icon_image = TrayIcon::from_rgba(rgba, icon_w, icon_h)?;

    let window = WindowBuilder::new()
        .with_title("logitech-sim-led")
        .with_inner_size(tao::dpi::LogicalSize::new(880.0, 680.0))
        .with_window_icon(window_icon)
        .build(&event_loop)?;

    // Give the axum server (spawned on the background runtime above) a
    // moment to actually be listening before pointing the webview at it.
    // A fixed sleep is not lovely, but it runs once at startup and web.rs
    // starts listening well within this window in practice; a retrying
    // webview navigation would need more wry API surface than is worth it
    // here.
    std::thread::sleep(Duration::from_millis(200));

    // WebView2's default data directory is next to the exe -- fine in a
    // dev build (target/debug or target/release), but a real install puts
    // the exe in Program Files, which a standard user token can't write
    // to. Without this, webview creation fails there specifically (silent
    // process exit, no window, exit code 1 -- confirmed the hard way: the
    // identical binary worked from target/release but not once installed
    // via the MSI to Program Files). `%LOCALAPPDATA%` is always writable
    // by the current user.
    let webview_data_dir = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("logitech-sim-led")
        .join("webview2");
    // Never read again after this, but must outlive the `WebViewBuilder`
    // it's borrowed by below (same "lives in this stack frame for the
    // process's effective lifetime" reasoning as `_webview`).
    let mut web_context = wry::WebContext::new(Some(webview_data_dir));

    // Never read again after this, but must stay alive for the app's
    // lifetime (dropping a `WebView` tears it down): it lives in this
    // stack frame for as long as `event_loop.run` below is executing,
    // which is effectively forever (see its diverging return type).
    let _webview = wry::WebViewBuilder::with_web_context(&mut web_context)
        .with_url(format!("http://{WEB_BIND_ADDR}"))
        .build(&window)?;

    let tray_menu = Menu::new();
    let show_item = MenuItem::new("Show Dashboard", true, None);
    let test_leds_item = MenuItem::new("Test LEDs", true, None);
    let autostart_item = CheckMenuItem::new(
        "Start with Windows",
        autostart::is_supported(),
        autostart::is_enabled(),
        None,
    );
    let quit_item = MenuItem::new("Quit", true, None);
    tray_menu.append(&show_item)?;
    tray_menu.append(&test_leds_item)?;
    if autostart::is_supported() {
        tray_menu.append(&autostart_item)?;
    }
    tray_menu.append(&PredefinedMenuItem::separator())?;
    tray_menu.append(&quit_item)?;

    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("logitech-sim-led")
        .with_icon(tray_icon_image)
        .build()?;

    let menu_rx = MenuEvent::receiver();
    let mut window_visible = true;

    event_loop.run(move |event, _elwt, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(150));

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Re-read live -- toggled from the dashboard's Settings
                // panel (web.rs's settings API mutates the same
                // `shared_cfg`), so a change takes effect on the very
                // next close, no restart needed.
                let minimize_to_tray = shared_cfg.read().unwrap().app.minimize_to_tray_on_close;
                if minimize_to_tray {
                    window.set_visible(false);
                    window_visible = false;
                } else {
                    tracing::info!("window closed with minimize-to-tray off, quitting");
                    shutdown_handle.shutdown();
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::NewEvents(StartCause::Init) => {
                tracing::info!("logitech-sim-led GUI started");
            }
            _ => {}
        }

        while let Ok(event) = menu_rx.try_recv() {
            if event.id == show_item.id() {
                window.set_visible(true);
                window.set_focus();
                window_visible = true;
            } else if event.id == test_leds_item.id() {
                // Runs even with the window hidden, unlike the dashboard's
                // own "Test LEDs" button (same underlying call -- see
                // web.rs's test_leds_handler) -- so the tray menu item
                // needs its own feedback path: a native dialog on a
                // throwaway thread, not blocking the event loop for the
                // ~1.6s the flash takes.
                std::thread::spawn(|| {
                    let result = sld_output_logitech_hid::test_leds();
                    let (level, text) = match result {
                        Ok(protocol) => (
                            rfd::MessageLevel::Info,
                            format!("LEDs flashed (protocol '{protocol}')"),
                        ),
                        Err(e) => (rfd::MessageLevel::Error, e.to_string()),
                    };
                    rfd::MessageDialog::new()
                        .set_title("logitech-sim-led")
                        .set_description(text)
                        .set_level(level)
                        .show();
                });
            } else if event.id == autostart_item.id() {
                let enable = autostart_item.is_checked();
                if let Err(e) = autostart::set_enabled(enable) {
                    tracing::error!(error = %e, "failed to change autostart setting");
                    autostart_item.set_checked(!enable);
                }
            } else if event.id == quit_item.id() {
                tracing::info!("quit requested from tray menu");
                shutdown_handle.shutdown();
                *control_flow = ControlFlow::Exit;
            }
        }

        let _ = window_visible;
    })
    // `EventLoop::run` never returns (it exits the process directly once
    // `control_flow` is set to `Exit`) -- `service_thread` is abandoned at
    // that point, same as any other GUI app quitting.
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // No console window in GUI mode (see main.rs's windows_subsystem
    // attribute), so stdout logging goes nowhere -- write to a rotating
    // file under the user's local app data instead. Falls back to stdout
    // (still useful when run from a dev terminal) if the log directory
    // can't be created.
    let log_dir = dirs::data_local_dir().map(|d| d.join("logitech-sim-led").join("logs"));

    match log_dir {
        Some(dir) if std::fs::create_dir_all(&dir).is_ok() => {
            let file_appender = tracing_appender::rolling::daily(&dir, "service.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            // Leaking the guard is deliberate: it must live for the whole
            // process lifetime to keep flushing, and this only runs once
            // at startup in a `fn main()` that otherwise never returns
            // control back here to hold onto it.
            std::mem::forget(guard);
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(non_blocking)
                .with_ansi(false)
                .init();
        }
        _ => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
    }
}
