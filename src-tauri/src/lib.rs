mod clipboard_watch;
mod db;
mod focus_restore;
mod settings;

use focus_restore::PreviousFrontApp;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub(crate) type SharedDb = Arc<Mutex<Connection>>;
pub(crate) type SkipOwnPasteboardWrites = Arc<AtomicBool>;

const MENU_OPEN_PICKER: &str = "open-picker";

fn lock_db(store: &SharedDb) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
    store
        .lock()
        .map_err(|_| "clipboard database mutex poisoned".to_string())
}

fn notify_clips_updated<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit("clips-updated", serde_json::Value::Null);
}

fn show_picker_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    focus_restore::capture_previous_front_app(app);
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();
    notify_clips_updated(app);
}

fn toggle_picker_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let visible = win.is_visible().unwrap_or(true);
    if visible {
        let _ = hide_picker_window(app, false);
    } else {
        show_picker_window(app);
    }
}

fn present_picker_window<R: Runtime>(app: &AppHandle<R>) {
    show_picker_window(app);
}

fn hide_picker_window<R: Runtime>(app: &AppHandle<R>, paste: bool) -> Result<(), String> {
    let Some(win) = app.get_webview_window("main") else {
        return Ok(());
    };
    win.hide().map_err(|e| e.to_string())?;
    focus_restore::restore_previous_front_app(app, paste);
    Ok(())
}

/// Menu bar tray + configurable OS-wide picker shortcut.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn install_desktop_shell<R: Runtime>(handle: &AppHandle<R>) -> Result<(), String> {
    settings::install_picker_hotkey_handler(handle, toggle_picker_window)?;

    let tray_menu = tauri::menu::Menu::with_items(
        handle,
        &[
            &tauri::menu::MenuItem::with_id(
                handle,
                MENU_OPEN_PICKER,
                "Open picker…",
                true,
                None::<&str>,
            )
            .map_err(|e| e.to_string())?,
            &tauri::menu::PredefinedMenuItem::separator(handle).map_err(|e| e.to_string())?,
            &tauri::menu::PredefinedMenuItem::quit(handle, Some("Quit ESTM")).map_err(|e| e.to_string())?,
        ],
    )
    .map_err(|e| e.to_string())?;

    let png = include_bytes!("../icons/32x32.png");
    let icon = tauri::image::Image::from_bytes(png).map_err(|e| e.to_string())?;

    tauri::tray::TrayIconBuilder::new()
        .tooltip("ESTM")
        .menu(&tray_menu)
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .show_menu_on_left_click(true)
        .on_menu_event(move |app_h, menu_event| {
            if menu_event.id() == MENU_OPEN_PICKER {
                present_picker_window(app_h);
            }
        })
        .build(handle)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn picker_hide<R: Runtime>(app: AppHandle<R>, paste: Option<bool>) -> Result<(), String> {
    hide_picker_window(&app, paste.unwrap_or(false))
}

#[tauri::command]
fn clips_recent(
    store: tauri::State<'_, SharedDb>,
    limit: Option<i64>,
) -> Result<Vec<db::ClipRow>, String> {
    let conn = lock_db(store.inner())?;
    db::list_recent(&conn, limit.unwrap_or(80)).map_err(|e| e.to_string())
}

#[tauri::command]
fn clips_search(
    store: tauri::State<'_, SharedDb>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<db::ClipRow>, String> {
    let conn = lock_db(store.inner())?;
    let q = query.trim();
    if q.is_empty() {
        return db::list_recent(&conn, limit.unwrap_or(80)).map_err(|e| e.to_string());
    }
    db::search(&conn, q, limit.unwrap_or(80)).map_err(|e| e.to_string())
}

#[tauri::command]
fn clips_clear(app: AppHandle, store: tauri::State<'_, SharedDb>) -> Result<(), String> {
    let conn = lock_db(store.inner())?;
    db::clear_all(&conn).map_err(|e| e.to_string())?;
    let _ = app.emit("clips-updated", serde_json::Value::Null);
    Ok(())
}

#[tauri::command]
fn clips_copy(
    store: tauri::State<'_, SharedDb>,
    skip_own: tauri::State<'_, SkipOwnPasteboardWrites>,
    id: i64,
) -> Result<(), String> {
    let plaintext = {
        let conn = lock_db(store.inner())?;
        db::get_plaintext(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("clip #{id} not found"))?
    };

    skip_own.store(true, Ordering::SeqCst);

    let mut clipboard = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            skip_own.store(false, Ordering::SeqCst);
            return Err(format!("clipboard unavailable: {e}"));
        }
    };

    if let Err(e) = clipboard.set_text(plaintext) {
        skip_own.store(false, Ordering::SeqCst);
        return Err(format!("clipboard write failed: {e}"));
    }

    Ok(())
}

#[tauri::command]
fn clips_set_pinned(
    app: AppHandle,
    store: tauri::State<'_, SharedDb>,
    id: i64,
    pinned: bool,
) -> Result<bool, String> {
    let conn = lock_db(store.inner())?;
    let n = db::set_pinned(&conn, id, pinned).map_err(|e| e.to_string())?;
    if n > 0 {
        let _ = app.emit("clips-updated", serde_json::Value::Null);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
fn settings_get() -> settings::SettingsSnapshot {
    settings::snapshot()
}

#[tauri::command]
fn settings_set_picker_hotkey(
    app: AppHandle,
    hotkey: settings::PickerHotkey,
) -> Result<settings::SettingsSnapshot, String> {
    hotkey.to_shortcut_string()?;

    let mut cfg = settings::load();
    cfg.picker_hotkey = hotkey;
    settings::save(&cfg)?;

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    settings::apply_picker_hotkey(&app, &cfg.picker_hotkey)?;

    Ok(settings::snapshot())
}

#[tauri::command]
fn settings_set_quick_pick_prefix(
    enabled: bool,
    prefix: String,
) -> Result<settings::SettingsSnapshot, String> {
    let prefix = settings::QuickPickPrefix::normalize_prefix(&prefix)?;

    let mut cfg = settings::load();
    cfg.quick_pick_prefix.enabled = enabled;
    cfg.quick_pick_prefix.prefix = prefix;
    settings::save(&cfg)?;

    Ok(settings::snapshot())
}

#[tauri::command]
fn settings_set_history_limits(
    app: AppHandle,
    store: tauri::State<'_, SharedDb>,
    max_entries: u32,
    use_max_age: bool,
    max_age_days: u32,
) -> Result<settings::SettingsSnapshot, String> {
    let limits = settings::HistoryLimits::normalize(max_entries, use_max_age, max_age_days)?;

    let mut cfg = settings::load();
    cfg.history_limits = limits;
    settings::save(&cfg)?;

    let conn = lock_db(store.inner())?;
    db::prune_and_cap(&conn, cfg.history_limits.to_db_limits()).map_err(|e| e.to_string())?;
    let _ = app.emit("clips-updated", serde_json::Value::Null);

    Ok(settings::snapshot())
}

#[tauri::command]
fn clips_set_label(
    app: AppHandle,
    store: tauri::State<'_, SharedDb>,
    id: i64,
    label: Option<String>,
) -> Result<bool, String> {
    let conn = lock_db(store.inner())?;
    let n = db::set_label(&conn, id, label.as_deref()).map_err(|e| e.to_string())?;
    if n > 0 {
        let _ = app.emit("clips-updated", serde_json::Value::Null);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
fn clips_delete(
    app: AppHandle,
    store: tauri::State<'_, SharedDb>,
    id: i64,
) -> Result<bool, String> {
    let conn = lock_db(store.inner())?;
    let n = db::delete_by_id(&conn, id).map_err(|e| e.to_string())?;
    if n > 0 {
        let _ = app.emit("clips-updated", serde_json::Value::Null);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            install_desktop_shell(app.handle())
                .map_err(|e| format!("desktop shell setup: {e}"))?;

            let path = db::sqlite_path();
            let conn =
                db::open_connection(&path).map_err(|e| format!("Opening database: {e}"))?;
            db::migrate(&conn).map_err(|e| format!("Database migration: {e}"))?;
            let limits = settings::load().history_limits.to_db_limits();
            db::prune_and_cap(&conn, limits)
                .map_err(|e| format!("Applying history limits: {e}"))?;
            let shared: SharedDb = Arc::new(Mutex::new(conn));
            let skip_own_writes: SkipOwnPasteboardWrites = Arc::new(AtomicBool::new(false));
            clipboard_watch::spawn(
                app.handle().clone(),
                shared.clone(),
                skip_own_writes.clone(),
            );
            app.manage(shared);
            app.manage(skip_own_writes);
            app.manage(PreviousFrontApp(Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            picker_hide,
            clips_recent,
            clips_search,
            clips_clear,
            clips_copy,
            clips_set_pinned,
            clips_set_label,
            clips_delete,
            settings_get,
            settings_set_picker_hotkey,
            settings_set_quick_pick_prefix,
            settings_set_history_limits
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
