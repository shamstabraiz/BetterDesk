//! System tray — branded context menu and event handling.
//!
//! The tray icon is always visible. Left-click toggles the main window.
//! Right-click shows the branded context menu.
//!
//! Menu layout:
//!   <Company Name> — Yomie
//!   ──────────────────────────────
//!   ❓  Request Help
//!   💬  Chat
//!   ──────────────────────────────
//!   ⚙  Settings
//!   👤  Operator Mode
//!   ──────────────────────────────
//!   ✕  Exit
//!
//! Branding (company name, accent color) is fetched from the server on
//! startup and cached locally. NO logo/image in the menu — text only.

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

// ---------------------------------------------------------------------------
//  Branding model
// ---------------------------------------------------------------------------

/// Server-provided branding — text + colors only, no images.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branding {
    /// Company name displayed in the tray menu header.
    pub company_name: String,
    /// Primary accent color (hex, e.g. "#3b82f6").
    pub accent_color: String,
    /// Support phone / email shown in the help dialog.
    pub support_contact: String,
}

impl Default for Branding {
    fn default() -> Self {
        Self {
            company_name: "Yomie".into(),
            accent_color: "#3b82f6".into(),
            support_contact: String::new(),
        }
    }
}

/// Global branding state managed by Tauri.
pub struct BrandingState(pub Mutex<Branding>);

// ---------------------------------------------------------------------------
//  Public helpers
// ---------------------------------------------------------------------------

/// Build and register the system tray icon. Call once inside `setup`.
pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // Initialize branding state with defaults.
    app.manage(BrandingState(Mutex::new(Branding::default())));

    let branding = Branding::default();
    let menu = build_tray_menu(app, &branding)?;

    // Load tray icon from embedded PNG.
    let icon_bytes = include_bytes!("../icons/32x32.png");
    let icon = tauri::image::Image::from_bytes(icon_bytes)
        .unwrap_or_else(|_| {
            let px: Vec<u8> = (0..16)
                .flat_map(|_| [30u8, 80u8, 160u8, 255u8])
                .collect();
            tauri::image::Image::new_owned(px, 4, 4)
        });

    TrayIconBuilder::with_id("yomie-tray")
        .tooltip("Yomie — right-click for menu")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu_event(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    debug!("Tray icon created");
    Ok(())
}

/// Fetch branding from server and update the tray menu.
pub async fn fetch_and_apply_branding<R: Runtime>(app: &AppHandle<R>, base_url: &str) {
    let url = format!("{}/api/bd/branding", base_url);
    debug!("Fetching branding from {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<Branding>().await {
                Ok(branding) => {
                    info!(
                        "Branding loaded: company={}, accent={}",
                        branding.company_name, branding.accent_color
                    );

                    // Cache branding to disk
                    if let Err(e) = cache_branding(&branding) {
                        warn!("Failed to cache branding: {}", e);
                    }

                    // Update state
                    if let Some(state) = app.try_state::<BrandingState>() {
                        *state.0.lock().unwrap() = branding.clone();
                    }

                    // Rebuild tray menu
                    if let Err(e) = rebuild_tray_menu(app, &branding) {
                        warn!("Failed to rebuild tray menu: {}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to parse branding: {}", e);
                    load_cached_branding(app);
                }
            }
        }
        Ok(resp) => {
            debug!("Branding endpoint returned {}", resp.status());
            load_cached_branding(app);
        }
        Err(e) => {
            debug!("Branding fetch failed: {}", e);
            load_cached_branding(app);
        }
    }
}

/// Navigate the main window to a specific SolidJS route, showing it if hidden.
pub fn navigate_to<R: Runtime>(app: &AppHandle<R>, route: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        let js = format!(
            "if(window.__navigate){{window.__navigate('{}')}}",
            route.replace('\'', "\\'")
        );
        let _ = window.eval(&js);
    } else {
        warn!("navigate_to: main window not found");
    }
}

// ---------------------------------------------------------------------------
//  Menu construction
// ---------------------------------------------------------------------------

fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    branding: &Branding,
) -> tauri::Result<tauri::menu::Menu<R>> {
    // Header: company name — Yomie (disabled, text-only branding)
    let header_text = if branding.company_name.is_empty() || branding.company_name == "Yomie" {
        "Yomie".to_string()
    } else {
        format!("{} — Yomie", branding.company_name)
    };

    let item_header = MenuItemBuilder::with_id("header", &header_text)
        .enabled(false)
        .build(app)?;

    // Device ID (disabled info line) — cached to avoid repeated WMI calls
    static CACHED_DEVICE_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let device_id_text = {
        let id = CACHED_DEVICE_ID.get_or_init(|| {
            crate::identity::get_or_create_device_id().unwrap_or_else(|_| "...".into())
        });
        format!("ID: {}", id)
    };
    let item_device_id = MenuItemBuilder::with_id("device_id", &device_id_text)
        .enabled(false)
        .build(app)?;

    // Help request
    let item_help = MenuItemBuilder::with_id("help_request", "❓  Request Help")
        .build(app)?;

    // Chat
    let item_chat = MenuItemBuilder::with_id("chat", "💬  Chat")
        .build(app)?;

    // Settings
    let item_settings = MenuItemBuilder::with_id("settings", "⚙  Settings")
        .build(app)?;

    // Operator mode
    let item_operator = MenuItemBuilder::with_id("operator_mode", "👤  Operator Mode")
        .build(app)?;

    // Show window
    let item_show = MenuItemBuilder::with_id("show", "🖥  Show Window")
        .build(app)?;

    // Exit
    let item_exit = MenuItemBuilder::with_id("exit", "✕  Exit")
        .build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&item_header)
        .item(&item_device_id)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&item_help)
        .item(&item_chat)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&item_settings)
        .item(&item_operator)
        .item(&item_show)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&item_exit)
        .build()?;

    Ok(menu)
}

fn rebuild_tray_menu<R: Runtime>(app: &AppHandle<R>, branding: &Branding) -> tauri::Result<()> {
    let menu = build_tray_menu(app, branding)?;
    if let Some(tray) = app.tray_by_id("yomie-tray") {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
//  Event handler
// ---------------------------------------------------------------------------

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    debug!("Tray menu event: {}", id);
    match id {
        "help_request" => {
            // Fire help request through IPC — frontend opens a small dialog.
            navigate_to(app, "/help-request");
        }
        "chat" => {
            // Open the dedicated chat window directly (not via main window).
            if let Some(chat_win) = app.get_webview_window("chat") {
                let _ = chat_win.show();
                let _ = chat_win.set_focus();
            } else {
                warn!("Chat window not found — falling back to main window route");
                navigate_to(app, "/chat");
            }
        }
        "settings" => navigate_to(app, "/settings"),
        "operator_mode" => navigate_to(app, "/operator"),
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "exit" => {
            app.exit(0);
        }
        _ => {}
    }
}

fn toggle_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                let _ = window.hide();
            }
            _ => {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    }
}

// ---------------------------------------------------------------------------
//  Branding cache (local disk)
// ---------------------------------------------------------------------------

fn branding_cache_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("com", "Yomie", "Yomie")
        .map(|d| d.config_dir().join("branding_cache.json"))
}

fn cache_branding(branding: &Branding) -> anyhow::Result<()> {
    if let Some(path) = branding_cache_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(branding)?;
        std::fs::write(path, data)?;
    }
    Ok(())
}

fn load_cached_branding<R: Runtime>(app: &AppHandle<R>) {
    if let Some(path) = branding_cache_path() {
        if path.exists() {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(branding) = serde_json::from_str::<Branding>(&data) {
                    debug!("Loaded cached branding: {}", branding.company_name);
                    if let Some(state) = app.try_state::<BrandingState>() {
                        *state.0.lock().unwrap() = branding.clone();
                    }
                    let _ = rebuild_tray_menu(app, &branding);
                }
            }
        }
    }
}
