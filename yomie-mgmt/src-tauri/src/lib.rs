//! Yomie MGMT Client — operator and admin remote management console.
//!
//! Architecture:
//! - `config`    — Persistent settings (server, keys, identity)
//! - `crypto`    — NaCl key exchange + secretbox stream encryption
//! - `protocol`  — RustDesk variable-length framing + protobuf codec
//! - `network`   — Signal (hbbs) and relay (hbbr) TCP/WS clients
//! - `identity`  — Device ID generation and management
//! - `capture`   — Cross-platform screen capture (DXGI / X11 / CGDisplay)
//! - `input`     — Cross-platform input simulation
//! - `clipboard` — Cross-platform clipboard sync
//! - `codec`     — Video encoding / decoding
//! - `service`   — Elevated privilege helpers (Windows Service / Linux root)
//! - `commands`  — Tauri IPC commands exposed to the frontend

pub mod cdap;
pub mod commands;
pub mod activity;
pub mod automation;
pub mod chat;
pub mod discovery;
pub mod file_transfer;
pub mod capture;
pub mod clipboard;
pub mod codec;
pub mod config;
pub mod crypto;
pub mod dataguard;
pub mod helpdesk;
pub mod identity;
pub mod input;
pub mod inventory;
pub mod logging;
pub mod management;
pub mod network;
pub mod protocol;
pub mod remote;
pub mod service;
pub mod tray;

pub mod proto {
    include!("proto/hbb.rs");
}

use log::info;
use std::sync::Mutex;
use std::time::Instant;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

/// Entry point — called from main.rs.
pub fn run() {
    let boot = Instant::now();

    // Default to DEBUG in --console mode, INFO otherwise.
    let is_console = std::env::args().any(|a| a == "--console");

    // Initialize dual logging: stderr + file in %APPDATA%/Yomie/Yomie/logs/
    logging::init(is_console);

    info!("Yomie MGMT v{} (pid={}) — boot", env!("CARGO_PKG_VERSION"), std::process::id());
    if is_console {
        info!("Console mode ACTIVE — all logs visible in terminal");
    }
    if let Some(path) = logging::log_file_path() {
        info!("Log file: {}", path.display());
    }

    let settings = config::Settings::load().unwrap_or_default();
    info!("Settings loaded in {:?}", boot.elapsed());

    // Central cancellation token — cancelled on app exit to stop all services.
    let shutdown_token = CancellationToken::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Another instance tried to start — show the existing window instead.
            info!("Second instance detected — bringing existing window to front");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(shutdown_token.clone())
        .manage(commands::AppState {
            settings: Mutex::new(settings),
            session: Mutex::new(None),
            registration: Mutex::new(None),
            bd_registration: Mutex::new(None),
            bd_relay: Mutex::new(None),
            inventory: Mutex::new(None),
            discovery: Mutex::new(None),
            chat_agent: Mutex::new(None),
            remote_agent: Mutex::new(None),
            cdap_agent: Mutex::new(None),
            activity: Mutex::new(commands::ActivityTracker::new()),
            session_manager: Mutex::new(None),
            read_notifs: Mutex::new(std::collections::HashSet::new()),
            dismissed_notifs: Mutex::new(std::collections::HashSet::new()),
            http_client: reqwest::Client::builder()
                .cookie_store(true)
                .timeout(std::time::Duration::from_secs(15))
                .danger_accept_invalid_certs(false)
                .build()
                .expect("Failed to build HTTP client"),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_device_id,
            commands::connect_to_peer,
            commands::disconnect,
            commands::authenticate,
            commands::send_key_event,
            commands::send_mouse_event,
            commands::simulate_local_key,
            commands::simulate_local_mouse,
            commands::simulate_local_text,
            commands::get_connection_state,
            commands::start_registration,
            commands::stop_registration,
            commands::get_registration_status,
            commands::get_inventory_status,
            commands::collect_inventory_now,
            commands::start_lan_discovery,
            commands::stop_lan_discovery,
            commands::get_discovered_servers,
            commands::get_discovery_status,
            commands::discover_mdns_servers,
            commands::request_server_registration,
            commands::poll_registration_status,
            commands::apply_server_config,
            // Server connection test
            commands::test_server_connection,
            commands::auto_connect_server,
            // Admin gate
            commands::is_admin,
            commands::elevate_and_apply_config,
            commands::elevate_restart,
            commands::org_login,
            commands::get_system_locale,
            // Chat ecosystem
            commands::get_chat_status,
            commands::send_chat_message,
            commands::mark_chat_read,
            commands::get_chat_contacts,
            commands::get_chat_groups,
            commands::load_chat_conversation,
            commands::create_chat_group,
            commands::open_chat_window,
            commands::close_chat_window,
            commands::reconnect_chat,
            // Remote desktop
            commands::get_remote_status,
            commands::start_remote_viewer,
            // Relay remote session (Phase 43)
            commands::start_remote_session,
            commands::stop_remote_session,
            commands::send_remote_input,
            commands::toggle_clipboard_sync,
            commands::send_special_key,
            commands::switch_display,
            commands::set_quality,
            commands::toggle_recording,
            commands::get_session_quality,
            // Device management
            commands::get_device_info_cmd,
            commands::lock_screen_cmd,
            commands::logoff_user_cmd,
            commands::restart_system_cmd,
            commands::shutdown_system_cmd,
            commands::abort_shutdown_cmd,
            commands::run_predefined_cmd,
            // CDAP
            commands::cdap_connect,
            commands::cdap_disconnect,
            commands::cdap_status,
            commands::cdap_get_config,
            commands::cdap_save_config,
            // Tray branding
            commands::get_branding,
            commands::request_help,
            // Operator mode
            commands::operator_login,
            commands::operator_login_2fa,
            commands::operator_get_devices,
            // API proxy (session-based, cookie jar)
            commands::api_proxy,
            commands::api_clear_session,
            commands::set_access_token,
            commands::operator_get_help_requests,
            commands::operator_accept_help_request,
            commands::operator_record_session_event,
            commands::operator_get_device_groups,
            commands::operator_configure_device,
            commands::operator_install_module,
            // Device actions (Phase 44)
            commands::operator_send_device_action,
            commands::operator_wake_on_lan,
            commands::operator_get_session_history,
            commands::operator_automation_get_rules,
            commands::operator_automation_save_rule,
            commands::operator_automation_delete_rule,
            commands::operator_automation_get_alerts,
            commands::operator_automation_ack_alert,
            commands::operator_automation_get_commands,
            commands::operator_automation_create_command,
            commands::operator_dataguard_get_policies,
            commands::operator_dataguard_save_policy,
            commands::operator_dataguard_delete_policy,
            commands::operator_dataguard_get_events,
            commands::operator_dataguard_get_stats,
            // Server management (MGMT Client)
            commands::server_get_health,
            commands::server_get_clients,
            commands::server_get_operators,
            commands::server_get_audit,
            commands::server_get_api_keys,
            commands::server_disconnect_client,
            commands::server_ban_client,
            commands::server_revoke_api_key,
            // Notification center (MGMT Client)
            commands::get_notifications,
            commands::mark_notification_read,
            commands::mark_all_notifications_read,
            commands::dismiss_notification,
            // File transfer
            commands::browse_local_files,
            commands::open_file_native,
            // Activity tracking
            commands::get_activity_log,
            // Logging bridge
            commands::write_log,
            commands::open_log_file,
            commands::get_log_path,
        ])
        .setup(move |app| {
            // System tray — always visible, redesigned with branding.
            if let Err(e) = tray::setup_tray(app.handle()) {
                log::warn!("Failed to create system tray: {}", e);
            }

            // Show main window on startup.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }

            // Enable autostart on first run — the plugin is already registered
            // but not enabled by default. We enable it once and persist a
            // sentinel so we don't touch it again (user can disable in settings).
            {
                use tauri_plugin_autostart::ManagerExt;
                let sentinel = config::Settings::config_path()
                    .map(|p| p.with_file_name(".autostart_enabled"));
                if let Ok(path) = sentinel {
                    if !path.exists() {
                        let mgr = app.autolaunch();
                        if let Err(e) = mgr.enable() {
                            log::warn!("Failed to enable autostart: {}", e);
                        } else {
                            info!("Autostart enabled (first run)");
                        }
                        let _ = std::fs::write(&path, "1");
                    }
                }
            }

            // Tokio runtime is now active — safe to spawn async tasks.
            let state = app.state::<commands::AppState>();
            let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
            info!("Tauri setup started at {:?} since boot", boot.elapsed());

            let has_server = !settings.server_address.is_empty()
                || !settings.console_url.is_empty();

            if has_server {
                match identity::get_or_create_device_id() {
                    Ok(device_id) => {
                        info!("Device ID resolved in {:?}: {}", boot.elapsed(), device_id);
                        if settings.native_protocol {
                            // Yomie native — HTTP registration + WebSocket relay
                            info!(
                                "Auto-starting BD native registration with {} as {}",
                                settings.bd_api_url(),
                                device_id
                            );
                            let service =
                                network::bd_registration::BdRegistrationService::start(
                                    &settings, &device_id,
                                );
                            *state.bd_registration.lock().unwrap_or_else(|e| e.into_inner()) = Some(service);

                            // Also start UDP signal keepalive — sends RegisterPeer
                            // heartbeats to the signal server so the device appears
                            // online in the in-memory peer map. Required for web
                            // remote and RustDesk-compatible relay connections.
                            info!(
                                "Auto-starting signal keepalive to {} as {}",
                                settings.hbbs_address(),
                                device_id
                            );
                            let sig_service =
                                network::registration::RegistrationService::start(
                                    &settings, &device_id,
                                );
                            *state.registration.lock().unwrap_or_else(|e| e.into_inner()) = Some(sig_service);

                            // Delay secondary services (inventory, chat, remote, CDAP)
                            // to avoid thundering herd on startup. They start 8 seconds
                            // after boot to give registration time to succeed first.
                            let settings_cl = settings.clone();
                            let device_id_cl = device_id.clone();
                            let app_h = app.handle().clone();

                            tauri::async_runtime::spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_secs(8)).await;

                                let state = app_h.state::<commands::AppState>();

                                // Inventory collector
                                let inv = inventory::InventoryCollector::start(
                                    &settings_cl, &device_id_cl,
                                );
                                if let Ok(mut lock) = state.inventory.lock() {
                                    *lock = Some(inv);
                                }

                                // Chat agent
                                let ws_scheme = settings_cl.server_ws_scheme();
                                let ws_host = settings_cl.server_ws_host();
                                let chat_ws = format!(
                                    "{}://{}/ws/chat/{}",
                                    ws_scheme, ws_host, device_id_cl
                                );
                                let chat = chat::ChatService::start(
                                    app_h.clone(),
                                    chat_ws,
                                    device_id_cl.clone(),
                                );
                                if let Ok(mut lock) = state.chat_agent.lock() {
                                    *lock = Some(chat);
                                }

                                // Remote desktop agent
                                let remote_ws = format!(
                                    "{}://{}/ws/remote-agent/{}",
                                    ws_scheme, ws_host, device_id_cl
                                );
                                let remote = remote::RemoteAgent::start(
                                    app_h.clone(),
                                    remote_ws,
                                    device_id_cl.clone(),
                                );
                                if let Ok(mut lock) = state.remote_agent.lock() {
                                    *lock = Some(remote);
                                }

                                // CDAP agent — disabled for desktop clients.
                                // Desktop clients use the management WebSocket on
                                // the Go API port (21114) instead. CDAP is reserved
                                // for IoT devices, bridges, and standalone agents.
                                // The management WS is started automatically by
                                // BdRegistrationService after enrollment succeeds.

                                // Fetch branding from server
                                tray::fetch_and_apply_branding(&app_h, &settings_cl.bd_api_url()).await;
                            });
                        } else {
                            // Legacy RustDesk-compatible UDP registration
                            info!(
                                "Auto-starting RustDesk registration with {} as {}",
                                settings.hbbs_address(),
                                device_id
                            );
                            let service =
                                network::registration::RegistrationService::start(
                                    &settings, &device_id,
                                );
                            *state.registration.lock().unwrap_or_else(|e| e.into_inner()) = Some(service);
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to get device ID, skipping auto-registration: {}",
                            e
                        );
                    }
                }
            } else {
                info!("Server address not configured — waiting for user setup via Server Setup page");
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Intercept window close → hide to tray instead of quitting.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Yomie");

    // App loop exited — signal all services to stop.
    shutdown_token.cancel();
    info!("Yomie shutdown complete");
}
