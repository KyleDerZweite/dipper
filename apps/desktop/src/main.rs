mod paths;
use anyhow::Result;
use async_trait::async_trait;
use dipper_core::{
    capture::Capture,
    dictation::{Dictation, DictationSnapshot},
    journal::Journal,
    settings::Settings,
    target::{DeliveryStatus, DispatchStage, Insertion, InsertionOutcome, TargetPermit},
};
use dipper_desktop::{DesktopAdapter, DesktopEvent};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{
    Emitter, Manager, State,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

struct AppState {
    _session_lock: std::fs::File,
    dictation: Arc<Dictation>,
    desktop: Option<Arc<DesktopAdapter>>,
    desktop_error: Option<String>,
    shutting_down: AtomicBool,
    exit_ready: AtomicBool,
}
struct UnavailableInsertion(String);
#[async_trait]
impl Insertion for UnavailableInsertion {
    fn reserve(&self) -> Result<TargetPermit> {
        anyhow::bail!(self.0.clone())
    }
    async fn arm(&self, _permit: TargetPermit) -> Result<TargetPermit> {
        anyhow::bail!(self.0.clone())
    }
    async fn dispatch(&self, _permit: &TargetPermit, _text: &str) -> InsertionOutcome {
        InsertionOutcome {
            stage: DispatchStage::Unchanged,
            status: DeliveryStatus::Suppressed,
            detail: self.0.clone(),
        }
    }
    async fn copy_explicit(&self, _text: &str) -> Result<()> {
        anyhow::bail!(self.0.clone())
    }
}
fn desktop(state: &AppState) -> Result<&Arc<DesktopAdapter>, String> {
    state.desktop.as_ref().ok_or_else(|| {
        state
            .desktop_error
            .clone()
            .unwrap_or_else(|| "Desktop adapter unavailable".into())
    })
}
#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> DictationSnapshot {
    state.dictation.snapshot()
}
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.dictation.settings()
}
#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    state
        .dictation
        .save_settings(settings)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn list_inputs() -> Result<serde_json::Value, String> {
    let catalog = tauri::async_runtime::spawn_blocking(Capture::inputs)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(
        serde_json::json!({"default_name":catalog.default, "devices":catalog.devices.into_iter().map(|d| serde_json::json!({"node_name":d.name,"description":d.description})).collect::<Vec<_>>()}),
    )
}
#[tauri::command]
async fn configure_hold(state: State<'_, AppState>, preferred: String) -> Result<String, String> {
    if state.dictation.snapshot().busy {
        return Err("Wait until dictation and cleanup finish before changing the Hold key.".into());
    }
    desktop(&state)?
        .setup_hold(&preferred)
        .await
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn effective_binding(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(match &state.desktop {
        Some(desktop) => desktop.effective_binding().await,
        None => None,
    })
}
#[tauri::command]
async fn open_shortcut_settings(state: State<'_, AppState>) -> Result<(), String> {
    if state.dictation.snapshot().busy {
        return Err("Wait until dictation finishes before opening shortcut settings.".into());
    }
    desktop(&state)?
        .open_shortcut_settings()
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn cancel_dictation(state: State<'_, AppState>) {
    state.dictation.cancel();
}
#[derive(Serialize)]
struct JournalRow {
    id: uuid::Uuid,
    started_at: String,
    state: String,
    raw_text: Option<String>,
    detail: String,
}
#[tauri::command]
async fn recent_recordings(state: State<'_, AppState>) -> Result<Vec<JournalRow>, String> {
    let journal = state.dictation.journal();
    let recordings = tauri::async_runtime::spawn_blocking(move || journal.recent(30))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(recordings
        .into_iter()
        .map(|recording| {
            let text = recording
                .attempts
                .iter()
                .rev()
                .find_map(|attempt| attempt.raw_text.clone());
            let last_attempt = recording.attempts.last();
            let last_insertion = recording.insertions.last();
            let mut outcomes = vec![format!("Audio {}", recording.state)];
            if let Some(attempt) = last_attempt {
                outcomes.push(format!("Recognition {}", attempt.state.replace('_', " ")));
            }
            if let Some(insertion) = last_insertion {
                outcomes.push(format!("Insertion {}", insertion.state));
            }
            let detail = recording
                .error
                .clone()
                .or_else(|| {
                    last_attempt.and_then(|attempt| {
                        attempt
                            .details
                            .get("error")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                })
                .or_else(|| {
                    last_attempt.and_then(|attempt| {
                        attempt.diagnostic_error.clone().or_else(|| {
                            attempt
                                .details
                                .get("diagnostic_error")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_owned)
                        })
                    })
                })
                .or_else(|| {
                    last_insertion.and_then(|insertion| {
                        insertion
                            .details
                            .get("detail")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                })
                .unwrap_or_else(|| {
                    format!(
                        "{} frames available, {} synchronized. Audio {}.",
                        recording.available_frames, recording.synced_frames, recording.availability
                    )
                });
            JournalRow {
                id: recording.recording_id,
                started_at: recording.started_at,
                state: outcomes.join(" · "),
                raw_text: text,
                detail,
            }
        })
        .collect())
}
#[tauri::command]
async fn retry_recovery(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.dictation.clone();
    let report = tauri::async_runtime::spawn_blocking(move || engine.reconcile_journal())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if report.unpersisted {
        return Err(report
            .error
            .unwrap_or_else(|| "Recovery remains unpersisted".into()));
    }
    Ok(())
}
#[tauri::command]
async fn copy_transcript(state: State<'_, AppState>, recording_id: String) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;
    let journal = state.dictation.journal();
    let recording = tauri::async_runtime::spawn_blocking(move || journal.recording(id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .ok_or("Recording not found")?;
    let text = recording
        .attempts
        .iter()
        .rev()
        .find_map(|attempt| attempt.raw_text.clone())
        .ok_or("No transcript retained")?;
    desktop(&state)?
        .copy_explicit(&text)
        .await
        .map_err(|e| e.to_string())
}
// Downsample the canonical embedded logo once. Alpha-weighted box averaging
// keeps thin strokes readable without adding an image runtime dependency.
fn small_logo(source: &tauri::image::Image<'_>) -> tauri::image::Image<'static> {
    let mut pixels = vec![0u8; 32 * 32 * 4];
    let width = source.width() as usize;
    let height = source.height() as usize;
    for y in 0..32 {
        for x in 0..32 {
            let mut sums = [0u64; 4];
            let mut count = 0;
            for sy in y * height / 32..((y + 1) * height / 32).max(y * height / 32 + 1).min(height)
            {
                for sx in x * width / 32..((x + 1) * width / 32).max(x * width / 32 + 1).min(width)
                {
                    let rgba = &source.rgba()[(sy * width + sx) * 4..][..4];
                    for channel in 0..3 {
                        sums[channel] += u64::from(rgba[channel]) * u64::from(rgba[3]);
                    }
                    sums[3] += u64::from(rgba[3]);
                    count += 1;
                }
            }
            let offset = (y * 32 + x) * 4;
            for channel in 0..3 {
                pixels[offset + channel] = sums[channel].checked_div(sums[3]).unwrap_or(0) as u8;
            }
            pixels[offset + 3] = sums[3].checked_div(count).unwrap_or(0) as u8;
        }
    }
    tauri::image::Image::new_owned(pixels, 32, 32)
}
fn tray_icon(
    logo: &tauri::image::Image<'_>,
    recording: bool,
    busy: bool,
    error: bool,
) -> tauri::image::Image<'static> {
    let mut pixels = logo.rgba().to_vec();
    let color = if recording {
        Some([226, 87, 87])
    } else if error {
        Some([222, 168, 65])
    } else if busy {
        Some([168, 206, 183])
    } else {
        None
    };
    if let Some(color) = color {
        for y in 20..32 {
            for x in 20..32 {
                let dx = x as i32 - 26;
                let dy = y as i32 - 26;
                if dx * dx + dy * dy <= 25 {
                    let offset = (y * 32 + x) * 4;
                    pixels[offset..offset + 3].copy_from_slice(&color);
                    pixels[offset + 3] = 255;
                }
            }
        }
    }
    tauri::image::Image::new_owned(pixels, 32, 32)
}
fn show_window(app: &tauri::AppHandle) {
    if app
        .try_state::<AppState>()
        .is_some_and(|state| state.dictation.snapshot().busy)
    {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
fn request_quit(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    if state.shutting_down.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        state.dictation.shutdown().await;
        if let Some(desktop) = &state.desktop
            && let Err(error) = desktop.stop().await
        {
            eprintln!("Dipper desktop cleanup: {error:#}");
        }
        state.exit_ready.store(true, Ordering::SeqCst);
        app.exit(0);
    });
}
fn main() -> Result<()> {
    unsafe {
        libc::umask(0o077);
    }
    // GTK can contact portals during startup. Make the host desktop identity
    // available before GTK initializes, including on the very first run.
    let paths = paths::AppPaths::resolve()?;
    let session_lock = paths.lock_session()?;
    paths.private_directories()?;
    paths.ensure_desktop_identity()?;
    let settings = Settings::load(&paths.settings, &paths.data)?;
    let journal = Arc::new(Journal::open(&paths.data, &paths.state)?);
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_snapshot, get_settings, save_settings, list_inputs, configure_hold, effective_binding, open_shortcut_settings, cancel_dictation, recent_recordings, retry_recovery, copy_transcript])
        .setup(move |app| {
            let brand = app.default_window_icon().ok_or_else(|| anyhow::anyhow!("Embedded Dipper logo missing"))?.clone();
            let tray_logo = small_logo(&brand);
            let recovery = journal.recovery_report();
            let native = tauri::async_runtime::block_on(DesktopAdapter::new(paths.clipboard_helper));
            let (native, desktop_error): (Option<Arc<DesktopAdapter>>, Option<String>) = match native {
                Ok(native) => (Some(Arc::new(native)), None),
                Err(error) => (None, Some(format!("{error:#}"))),
            };
            let insertion: Arc<dyn Insertion> = match &native {
                Some(native) => native.clone(),
                None => Arc::new(UnavailableInsertion(desktop_error.clone().unwrap_or_default())),
            };
            if let Some(native) = &native
                && let Err(error) = tauri::async_runtime::block_on(native.restore_hold()) { eprintln!("Dipper shortcut restore: {error:#}"); }
            let dictation = Dictation::new(journal, insertion, paths.worker, paths.settings, settings);
            if let Some(error) = &desktop_error { dictation.session_lost(error); }
            if recovery.unpersisted { dictation.session_lost("Journal recovery could not persist. Existing data was preserved; check available storage."); }
            app.manage(AppState {
                _session_lock: session_lock, dictation: dictation.clone(), desktop: native.clone(), desktop_error, shutting_down: AtomicBool::new(false), exit_ready: AtomicBool::new(false) });
            let status = MenuItem::with_id(app, "status", "Dipper: Ready", false, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "Settings and Journal", true, None::<&str>)?;
            let cancel = MenuItem::with_id(app, "cancel", "Cancel dictation", false, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Dipper", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&status, &settings_item, &cancel, &quit])?;
            TrayIconBuilder::with_id("dipper").icon(tray_icon(&tray_logo, false, false, false)).tooltip("Dipper: Ready").menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_window(app),
                    "cancel" => app.state::<AppState>().dictation.cancel(),
                    "quit" => request_quit(app),
                    _ => (),
                }).build(app)?;
            if let Some(window) = app.get_webview_window("main") {
                window.set_icon(brand)?;
                let handle = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event { api.prevent_close(); let _ = handle.hide(); }
                });
            }
            let mut snapshot = dictation.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let value = snapshot.borrow().clone();
                    let _ = handle.emit("dictation-state", &value);
                    let _ = status.set_text(format!("Dipper: {}", value.phase));
                    let _ = cancel.set_enabled(value.busy);
                    let _ = settings_item.set_enabled(!value.busy);
                    if let Some(tray) = handle.tray_by_id("dipper") {
                        let _ = tray.set_tooltip(Some(format!("Dipper: {}. {}", value.phase, value.detail)));
                        let _ = tray.set_icon(Some(tray_icon(&tray_logo, value.recording, value.busy, value.error)));
                    }
                    if snapshot.changed().await.is_err() { break; }
                }
            });
            if let Some(native) = native {
                let mut events = native.subscribe();
                let engine = dictation.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        match events.recv().await {
                            Ok(DesktopEvent::HoldPressed) => { if let Err(error) = engine.begin() { eprintln!("Dipper admission: {error:#}"); } }
                            Ok(DesktopEvent::HoldReleased) => engine.release(),
                            Ok(DesktopEvent::SessionLost(reason)) => engine.session_lost(&reason),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => engine.session_lost("Desktop event queue overflowed."),
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
            }
            if std::env::args().any(|arg| arg == "--show") || !dictation.settings().model_path.is_file() { show_window(app.handle()); }
            Ok(())
        })
        .build(tauri::generate_context!())?;
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event
            && !app.state::<AppState>().exit_ready.load(Ordering::SeqCst)
        {
            api.prevent_exit();
            request_quit(app);
        }
    });
    Ok(())
}
