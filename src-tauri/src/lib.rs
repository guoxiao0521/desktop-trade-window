mod models;
mod providers;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WindowEvent,
};

use models::{resolve_symbol, Candle, Quote};
use providers::{fetch_kline_with_fallback, fetch_quote_with_fallback};

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_market")]
    pub market: String,
    #[serde(default = "default_last_symbols")]
    pub last_symbols: HashMap<String, String>,
    #[serde(default = "default_period")]
    pub period: String,
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub always_on_top: bool,
    /// Outer window position in physical pixels (from last drag).
    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
    /// Inner window size in physical pixels (from last resize).
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
}

fn default_market() -> String {
    "US".into()
}

fn default_period() -> String {
    "1m".into()
}

fn default_color_scheme() -> String {
    "green-up".into()
}

fn default_theme() -> String {
    "dark".into()
}

fn default_last_symbols() -> HashMap<String, String> {
    HashMap::from([
        ("US".into(), "AAPL".into()),
        ("HK".into(), "00700".into()),
        ("KR".into(), "005930".into()),
    ])
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            market: default_market(),
            last_symbols: default_last_symbols(),
            period: default_period(),
            color_scheme: default_color_scheme(),
            theme: default_theme(),
            always_on_top: false,
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join(SETTINGS_FILE))
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<Option<AppSettings>, String> {
    let path = settings_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("read settings: {e}"))?;
    match serde_json::from_str::<AppSettings>(&text) {
        Ok(settings) => Ok(Some(settings)),
        Err(e) => {
            eprintln!("Failed to parse settings ({e}), using defaults");
            Ok(Some(AppSettings::default()))
        }
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
    }
    let text = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("serialize settings: {e}"))?;
    fs::write(&path, text).map_err(|e| format!("write settings: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn fetch_kline(
    symbol: String,
    period: String,
    market: Option<String>,
) -> Result<Vec<Candle>, String> {
    let resolved = resolve_symbol(&symbol, market.as_deref())?;
    let interval = match period.as_str() {
        "5m" | "m5" => "5m",
        _ => "1m",
    };
    fetch_kline_with_fallback(&resolved, interval).await
}

#[tauri::command]
async fn fetch_quote(symbol: String, market: Option<String>) -> Result<Quote, String> {
    let resolved = resolve_symbol(&symbol, market.as_deref())?;
    fetch_quote_with_fallback(&resolved).await
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Stock Widget")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => toggle_main_window(app),
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let _tray = builder.build(app)?;
    Ok(())
}

fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(true) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            fetch_kline,
            fetch_quote,
            load_settings,
            save_settings
        ])
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Close button / Alt+F4 → hide to tray instead of quitting
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
