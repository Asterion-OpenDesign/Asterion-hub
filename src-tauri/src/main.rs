// The Hub shell: a WebView (Vite + React) over the `hub-core` engine. This file only wires commands and
// events; every decision and every number the UI shows comes from the engine.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hub_core::engine::{Action, AppView, CatalogStatus, Engine, Event, HostView, HubConfig, TaskView};
use hub_core::error::HubError;
use hub_core::store::{HistoryEntry, Settings};
use hub_core::trust::RootSet;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager};

const HUB_CONFIG: &str = include_str!("../hub.config.json");
const ROOTS: &[u8] = include_bytes!("../../trust/roots.json");

#[derive(Deserialize)]
struct HubConfigFile {
    catalog_base_url: String,
    allowed_download_hosts: Vec<String>,
}

/// What a failed command looks like to the UI.
#[derive(Serialize)]
struct CmdError {
    code: String,
    detail: String,
    retryable: bool,
}

impl From<HubError> for CmdError {
    fn from(e: HubError) -> Self {
        CmdError { code: e.code().into(), detail: e.to_string(), retryable: e.retryable() }
    }
}

type CmdResult<T> = Result<T, CmdError>;

struct AppState {
    engine: Engine,
}

#[derive(Serialize)]
struct Bootstrap {
    host: HostView,
    settings: Settings,
    status: CatalogStatus,
    requires_newer_hub: Option<String>,
    /// Debug builds that were pointed at a local catalog say so, so nobody mistakes one for production.
    dev_catalog: Option<String>,
}

#[tauri::command]
fn bootstrap(state: tauri::State<AppState>) -> Bootstrap {
    Bootstrap {
        host: state.engine.host(),
        settings: state.engine.settings(),
        status: state.engine.status(),
        requires_newer_hub: state.engine.catalog_requires_newer_hub(),
        dev_catalog: dev_catalog_url(),
    }
}

#[tauri::command]
async fn refresh_catalog(state: tauri::State<'_, AppState>) -> CmdResult<CatalogStatus> {
    Ok(state.engine.refresh().await)
}

#[tauri::command]
fn catalog_status(state: tauri::State<AppState>) -> CatalogStatus {
    state.engine.status()
}

#[tauri::command]
fn list_apps(state: tauri::State<AppState>) -> Vec<AppView> {
    state.engine.apps()
}

#[tauri::command]
fn list_tasks(state: tauri::State<AppState>) -> Vec<TaskView> {
    state.engine.tasks()
}

// `async` on purpose: the task is spawned onto the runtime, which only exists for async commands.
#[tauri::command]
async fn start_task(state: tauri::State<'_, AppState>, app_id: String, action: Action) -> CmdResult<String> {
    state.engine.start(&app_id, action).map_err(Into::into)
}

#[tauri::command]
fn cancel_task(state: tauri::State<AppState>, task_id: String) -> bool {
    state.engine.cancel(&task_id)
}

#[tauri::command]
fn dismiss_task(state: tauri::State<AppState>, task_id: String) {
    state.engine.dismiss(&task_id)
}

#[tauri::command]
fn dismiss_finished_tasks(state: tauri::State<AppState>) {
    state.engine.dismiss_finished()
}

#[tauri::command]
fn open_app(state: tauri::State<AppState>, app_id: String) -> CmdResult<()> {
    state.engine.open(&app_id).map_err(Into::into)
}

#[tauri::command]
fn list_history(state: tauri::State<AppState>, limit: usize) -> Vec<HistoryEntry> {
    state.engine.history(limit.min(1000))
}

#[tauri::command]
fn clear_history(state: tauri::State<AppState>) -> CmdResult<()> {
    state.engine.clear_history().map_err(Into::into)
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Settings {
    state.engine.settings()
}

#[tauri::command]
fn save_settings(state: tauri::State<AppState>, settings: Settings) -> CmdResult<()> {
    state.engine.save_settings(settings).map_err(Into::into)
}

/// Opens a link in the system browser. Only https, and only the Hub's own docs site or a homepage that
/// appears in the signed catalog: the web layer cannot make the Hub open arbitrary addresses.
#[tauri::command]
fn open_link(state: tauri::State<AppState>, url: String) -> CmdResult<()> {
    let allowed = state.engine.apps().iter().any(|a| a.homepage.as_deref() == Some(url.as_str())) || url.starts_with("https://github.com/Asterion-OpenDesign/");
    if !allowed {
        return Err(HubError::UrlRefused("that link is not one the Hub may open".into()).into());
    }
    hub_core::platform::open_url(&url).map_err(Into::into)
}

#[tauri::command]
fn open_logs_folder(state: tauri::State<AppState>) -> CmdResult<()> {
    let dir = state.engine.logs_dir();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer.exe").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("/usr/bin/open").arg(&dir).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r: std::io::Result<std::process::Child> = Err(std::io::Error::other("unsupported"));
    r.map(|_| ()).map_err(|e| HubError::io("open logs folder", e).into())
}

#[cfg(debug_assertions)]
fn dev_catalog_url() -> Option<String> {
    std::env::var("ASTERION_HUB_DEV_CATALOG_URL").ok().filter(|s| !s.is_empty())
}
#[cfg(not(debug_assertions))]
fn dev_catalog_url() -> Option<String> {
    None
}

#[cfg(debug_assertions)]
fn dev_data_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("ASTERION_HUB_DEV_DATA_DIR").map(Into::into)
}
#[cfg(not(debug_assertions))]
fn dev_data_dir() -> Option<std::path::PathBuf> {
    None
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            let cfg: HubConfigFile = serde_json::from_str(HUB_CONFIG)?;
            let data_dir = dev_data_dir().unwrap_or(app.path().app_data_dir()?);
            let base = dev_catalog_url().unwrap_or(cfg.catalog_base_url);
            init_logging(&data_dir);

            let handle = app.handle().clone();
            let emit: hub_core::engine::Emitter = Arc::new(move |e: Event| {
                let _ = handle.emit("hub://event", &e);
            });
            let engine = Engine::new(
                HubConfig {
                    hub_version: semver::Version::parse(env!("CARGO_PKG_VERSION"))?,
                    catalog_base: url::Url::parse(&base)?,
                    roots: RootSet::from_json(ROOTS)?,
                    allowed_hosts: cfg.allowed_download_hosts,
                    allow_loopback_http: cfg!(debug_assertions),
                    data_dir,
                },
                emit,
            )?;
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "ASTERION Hub started");

            // Check for updates on start and, if the user chose a schedule, on that schedule while the
            // Hub is open. Nothing runs in the background once the Hub is closed: the applications
            // never depend on it.
            let e2 = engine.clone();
            tauri::async_runtime::spawn(async move {
                if e2.auto_check_due() {
                    e2.refresh().await;
                }
                loop {
                    tokio::time::sleep(Duration::from_secs(15 * 60)).await;
                    if e2.settings().auto_check.interval_hours().is_some() && e2.auto_check_due() {
                        e2.refresh().await;
                    }
                }
            });
            app.manage(AppState { engine });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(state) = window.try_state::<AppState>() {
                    // Closing while the installer runs would leave the user wondering what state the app is in.
                    if state.engine.has_active_install() {
                        api.prevent_close();
                        let _ = window.emit("hub://close-blocked", ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            refresh_catalog,
            catalog_status,
            list_apps,
            list_tasks,
            start_task,
            cancel_task,
            dismiss_task,
            dismiss_finished_tasks,
            open_app,
            list_history,
            clear_history,
            get_settings,
            save_settings,
            open_link,
            open_logs_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ASTERION Hub");
}

fn init_logging(data_dir: &std::path::Path) {
    let dir = data_dir.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let appender = tracing_appender::rolling::daily(dir, "hub.log");
    let _ = tracing_subscriber::fmt()
        .with_writer(appender)
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,hyper=warn,reqwest=warn".into()))
        .try_init();
}
