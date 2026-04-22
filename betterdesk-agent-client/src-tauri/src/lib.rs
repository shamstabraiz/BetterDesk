//! BetterDesk Agent Client — lightweight endpoint device agent.
//!
//! Architecture:
//! - `config`          — Persistent settings (server address, device identity, preferences)
//! - `registration`    — Multi-step server validation and device registration flow
//! - `sysinfo_collect` — System information collection (hostname, OS, CPU, RAM, disk)
//! - `commands`        — Tauri IPC commands exposed to the frontend

pub mod commands;
pub mod config;
pub mod privileges;
pub mod registration;
pub mod sidecar;
pub mod sysinfo_collect;

use log::info;
use std::sync::Mutex;
use tauri::Manager;

/// Keeps the `TrayIcon` handle alive for the entire app lifetime.
///
/// In Tauri v2 `TrayIcon` is reference-counted — dropping all handles
/// unregisters the icon from the system tray.  We store one handle in
/// managed state so it is never dropped until the process exits.
#[allow(dead_code)]
struct TrayState(tauri::tray::TrayIcon<tauri::Wry>);

async fn resolve_sidecar_config_from_state(
    app: &tauri::AppHandle,
) -> Option<sidecar::SidecarConfig> {
    let mut config = {
        let state = app.try_state::<commands::AgentState>()?;
        let guard = state.config.lock().ok()?;
        if !guard.is_registered() {
            return None;
        }
        guard.clone()
    };

    registration::normalize_server_origin_best_effort(&mut config).await;

    if let Some(state) = app.try_state::<commands::AgentState>() {
        if let Ok(mut guard) = state.config.lock() {
            guard.server_address = config.server_address.clone();
        }
    }

    Some(config.to_sidecar_config())
}

/// Spawn a background task that sends `POST /api/heartbeat` every 12 seconds.
///
/// This keeps the device visible as ONLINE in the web panel even when the
/// CDAP sidecar is not running (e.g. binary not installed, auth not configured).
/// The task is idempotent — only one should run per app instance.
fn start_heartbeat_task(app: &tauri::AppHandle) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        use tokio::time::{interval, Duration, MissedTickBehavior};
        let mut ticker = interval(Duration::from_secs(12));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            // Read current registration state inside the lock — release before await.
            let snapshot = {
                let Some(state) = app_handle.try_state::<commands::AgentState>() else {
                    continue;
                };
                let Ok(guard) = state.config.lock() else { continue };
                if !guard.is_registered() {
                    continue;
                }
                (guard.server_address.clone(), guard.device_id.clone())
            };
            let (address, device_id) = snapshot;

            // Build URL with the same logic used in commands::format_api_url.
            let url = {
                let addr = address.trim();
                let with_scheme = if addr.starts_with("http://") || addr.starts_with("https://") {
                    addr.to_string()
                } else {
                    format!("http://{}", addr)
                };
                if let Ok(parsed) = url::Url::parse(&with_scheme) {
                    let host = parsed.host_str().unwrap_or("localhost");
                    let port = parsed.port().unwrap_or(21114);
                    let scheme = parsed.scheme();
                    format!("{}://{}:{}/api/heartbeat", scheme, host, port)
                } else {
                    format!("http://{}:21114/api/heartbeat", addr)
                }
            };

            let payload = serde_json::json!({ "id": device_id });
            if let Ok(client) = registration::build_http_client(8) {
                if let Err(e) = client.post(&url).json(&payload).send().await {
                    log::debug!("[heartbeat] Failed: {}", e);
                }
            }
        }
    });
}

/// Entry point — called from main.rs.
pub fn run() {
    // WebKitGTK Wayland workaround: prevent Gdk "Error 71 (Protocol error)
    // dispatching to Wayland display" crash on GNOME Wayland sessions.
    // GPU compositing in WebKit fails on some Wayland compositors without
    // XWayland fallback; disabling it keeps the app functional.
    // Must be set before GTK/GDK initializes (i.e. before tauri::Builder).
    #[cfg(target_os = "linux")]
    if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
        // SAFETY: called before any threads are spawned; no concurrent env access.
        unsafe { std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1") };
    }

    let is_console = std::env::args().any(|a| a == "--console");
    let default_level = if is_console { "debug" } else { "info" };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .format_timestamp_millis()
        .init();

    info!(
        "BetterDesk Agent v{} (pid={}) — boot",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    );

    let mut settings = config::AgentConfig::load().unwrap_or_default();
    if let Err(e) = settings.repair_legacy_registration_state() {
        log::warn!("Failed to repair legacy registration state: {}", e);
    }
    let is_registered = settings.is_registered();
    let auto_start = settings.auto_start_sidecar && is_registered;
    info!(
        "Config loaded — registered: {}, server: {:?}",
        is_registered,
        settings.server_address
    );

    let sidecar_manager = sidecar::SidecarManager::new();
    let sidecar_manager_clone = sidecar_manager.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            info!("Second instance detected — bringing existing window to front");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(commands::AgentState {
            config: Mutex::new(settings),
            chat_history: Mutex::new(Vec::new()),
            sidecar: sidecar_manager,
        })
        .invoke_handler(tauri::generate_handler![
            // Status & lifecycle
            commands::is_os_admin,
            commands::quit_app,
            commands::get_agent_status,
            commands::get_system_info,
            commands::get_installed_software,
            commands::get_system_services,
            commands::get_disk_partitions,
            commands::get_network_adapters,
            commands::reconnect_agent,
            commands::send_diagnostics,
            commands::get_agent_version,
            commands::copy_to_clipboard,
            // Registration flow
            commands::validate_server_step,
            commands::register_device,
            commands::poll_enrollment_status,
            commands::sync_initial_config,
            commands::discover_lan_servers,
            // Sidecar control
            commands::get_sidecar_status,
            commands::start_sidecar,
            commands::stop_sidecar,
            commands::restart_sidecar,
            commands::restart_agent_service,
            commands::answer_consent,
            // Chat
            commands::get_chat_history,
            commands::send_chat_message,
            // Help request
            commands::request_help,
            commands::cancel_help_request,
            // Settings
            commands::get_agent_settings,
            commands::save_agent_settings,
            commands::test_server_connection,
            commands::unregister_device,
            commands::authenticate_sudo,
            commands::log_frontend_event,
        ])
        .setup(move |app| {
            info!("Tauri setup complete");

            // Tray icon — always visible, minimal.
            // Keep the returned handle in managed state; dropping it would
            // unregister the icon from the system tray immediately.
            let tray = setup_tray(app.handle())?;
            app.manage(TrayState(tray));

            let is_autostart = std::env::args().any(|a| a == "--autostart");

            // On first run (device not registered yet): show the window so the
            // SetupWizard is immediately visible without requiring the user to
            // click the tray icon.
            if !is_registered && !is_autostart {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            }

            // Hide main window on startup if autostart mode.
            if is_autostart {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            // Auto-start Go sidecar if device is registered and setting is on.
            if auto_start {
                let app_handle = app.handle().clone();
                let sidecar_manager = sidecar_manager_clone.clone();
                tauri::async_runtime::spawn(async move {
                    info!("[sidecar] Auto-starting Go agent sidecar...");
                    let Some(sidecar_cfg) = resolve_sidecar_config_from_state(&app_handle).await else {
                        info!("[sidecar] Auto-start skipped: device not registered");
                        return;
                    };

                    if let Err(e) = sidecar_manager.start(&sidecar_cfg) {
                        // Non-fatal: sidecar binary may not be installed yet.
                        // User can start it manually from tray or settings.
                        log::warn!("[sidecar] Auto-start failed: {}", e);
                    } else {
                        // Start stdout reader for consent-request events.
                        sidecar_manager.start_stdout_reader(app_handle.clone());
                    }
                });
            }

            // Always start the standalone HTTP heartbeat — keeps the device
            // visible as ONLINE even when the CDAP sidecar is not running.
            start_heartbeat_task(app.handle());

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                use tauri::Emitter;

                api.prevent_close();
                info!("Close requested from window chrome");
                let _ = window.set_focus();
                let _ = window.app_handle().emit("request-quit", ());
            }
        })
        .run(tauri::generate_context!())
        .expect("Failed to start BetterDesk Agent");
}

/// System tray setup.
///
/// Menu layout:
/// - User items (always visible):    Show ID, Help request, Chat, Check connection
/// - Admin-gated items (always visible, checked on click): Settings, Quit agent
///
/// All items are always visible in the menu so that admin users who launched
/// the app without UAC elevation can still see and use Quit / Settings.
/// Admin membership is re-checked at click time for security.
fn setup_tray(
    app: &tauri::AppHandle,
) -> Result<tauri::tray::TrayIcon<tauri::Wry>, Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;

    let is_admin = privileges::is_os_admin();
    info!("Tray setup — OS admin: {}", is_admin);

    // Always-visible (user) items.
    let show_id = MenuItemBuilder::with_id("show_id", "Show device ID").build(app)?;
    let help = MenuItemBuilder::with_id("help_request", "Request help").build(app)?;
    let chat = MenuItemBuilder::with_id("chat", "Chat").build(app)?;
    let check = MenuItemBuilder::with_id("check_conn", "Check connection").build(app)?;

    // Sidecar control (visible to all — non-admin gets informational deny).
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sidecar_toggle = MenuItemBuilder::with_id("sidecar_toggle", "Restart CDAP agent").build(app)?;

    // Admin-gated items — always visible, privilege checked on click.
    let sep2 = PredefinedMenuItem::separator(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit agent").build(app)?;

    let builder = MenuBuilder::new(app)
        .item(&show_id)
        .item(&help)
        .item(&chat)
        .item(&check)
        .item(&sep1)
        .item(&sidecar_toggle)
        .item(&sep2)
        .item(&settings)
        .item(&quit);

    let menu = builder.build()?;

    // Load tray icon from the app bundle icons.
    let icon = app.default_window_icon().cloned();

    let mut tray_builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip(if is_admin {
            "BetterDesk Agent (admin)"
        } else {
            "BetterDesk Agent"
        });

    if let Some(icon) = icon {
        tray_builder = tray_builder.icon(icon);
    }

    let tray = tray_builder
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "show_id" => show_window(app, "/"),
                "help_request" => show_window(app, "/help"),
                "chat" => show_window(app, "/chat"),
                "check_conn" => show_window(app, "/?action=reconnect"),
                "sidecar_toggle" => {
                    // Restart the Go sidecar on demand (no admin required —
                    // sidecar is a user-space process, operator-side gating is
                    // already enforced by the server's RBAC).
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let Some(sc_cfg) = resolve_sidecar_config_from_state(&app_handle).await else {
                            info!("[tray] Sidecar toggle: device not registered");
                            return;
                        };

                        if let Some(state) = app_handle.try_state::<commands::AgentState>() {
                            state.sidecar.stop();
                            if let Err(e) = state.sidecar.start(&sc_cfg) {
                                log::warn!("[tray] Sidecar restart failed: {}", e);
                            } else {
                                state.sidecar.start_stdout_reader(app_handle.clone());
                                info!("[tray] Sidecar restarted");
                            }
                        }
                    });
                }
                // Settings — always accessible. A SudoAuthDialog in the frontend
                // gates the actual controls for non-admin users.
                "settings" => {
                    show_window(app, "/settings");
                }
                // Quit — show the main window then emit an event so the frontend
                // can present a confirmation / sudo-auth dialog before exiting.
                // This allows the user to cancel the quit if triggered by mistake.
                "quit" => {
                    use tauri::Emitter;
                    info!("Quit requested from tray");
                    show_window(app, "/");
                    let _ = app.emit("request-quit", ());
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // Single click or double click — show main window.
            match event {
                tauri::tray::TrayIconEvent::Click { .. }
                | tauri::tray::TrayIconEvent::DoubleClick { .. } => {
                    show_window(tray.app_handle(), "/");
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(tray)
}

/// Bring the main window to front and navigate to a given route.
///
/// The route is emitted as a `navigate` event; the SolidJS router listens
/// and performs client-side navigation. Falls back to showing the window
/// even if navigation fails (e.g. frontend not ready).
fn show_window(app: &tauri::AppHandle, route: &str) {
    use tauri::Emitter;

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = app.emit("navigate", route.to_string());
    }
}
