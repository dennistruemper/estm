//! Polls the system clipboard for new plaintext (macOS: arboard + fingerprint tracking).

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[cfg(target_os = "macos")]
mod inner {
    use crate::db;
    use crate::SharedDb;
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{ns_string, NSString};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::{ffi::CStr, thread, time::Duration};
    use tauri::{AppHandle, Emitter};

    const POLL_MS: u64 = 120;

    #[inline]
    fn nsstring_utf8(ns: &NSString) -> Option<String> {
        unsafe {
            let p = ns.UTF8String();
            if p.is_null() {
                return Some(String::new());
            }
            Some(CStr::from_ptr(p).to_string_lossy().into_owned())
        }
    }

    fn pasteboard_plaintext_ns() -> Option<String> {
        let pb = NSPasteboard::generalPasteboard();
        for ty in [
            ns_string!("NSStringPboardType"),
            ns_string!("public.utf8-plain-text"),
            ns_string!("public.plain-text"),
        ] {
            let Some(ns) = pb.stringForType(ty) else {
                continue;
            };
            let rust = nsstring_utf8(&ns)?;
            if !rust.is_empty() {
                return Some(rust);
            }
        }
        None
    }

    fn read_plaintext() -> Option<String> {
        if let Some(t) = pasteboard_plaintext_ns() {
            return Some(t);
        }
        let mut cb = arboard::Clipboard::new().ok()?;
        let t = cb.get_text().ok()?;
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    }

    fn ingest(db: &SharedDb, txt: String) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let conn = db.lock().map_err(|_| "db mutex poisoned")?;
        let limits = crate::settings::load().history_limits.to_db_limits();
        Ok(db::record_plaintext_capture(&conn, txt, limits)?)
    }

    pub fn spawn(app: AppHandle, db: SharedDb, skip_own_clipboard_writes: Arc<AtomicBool>) {
        thread::spawn(move || {
            let mut last_fp: Option<String> = None;
            if let Some(t) = read_plaintext() {
                last_fp = Some(db::fingerprint_plaintext(&t));
            }

            loop {
                thread::sleep(Duration::from_millis(POLL_MS));

                if skip_own_clipboard_writes.swap(false, Ordering::SeqCst) {
                    if let Some(t) = read_plaintext() {
                        last_fp = Some(db::fingerprint_plaintext(&t));
                    }
                    continue;
                }

                let Some(txt) = read_plaintext() else {
                    continue;
                };

                let fp = db::fingerprint_plaintext(&txt);
                if fp.is_empty() {
                    continue;
                }
                if last_fp.as_ref() == Some(&fp) {
                    continue;
                }
                last_fp = Some(fp);

                match ingest(&db, txt) {
                    Ok(true) => {
                        let _ = app.emit("clips-updated", serde_json::Value::Null);
                    }
                    Ok(false) => {}
                    Err(e) => {
                        eprintln!("[ESTM] clipboard ingest: {e}");
                    }
                }
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod inner {
    use crate::SharedDb;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tauri::AppHandle;

    #[allow(unused_variables)]
    pub fn spawn(_app: AppHandle, _db: SharedDb, _skip: Arc<AtomicBool>) {
        eprintln!("[ESTM] clipboard watcher inactive on non-macOS.");
    }
}

pub fn spawn(app: tauri::AppHandle, db: crate::SharedDb, skip_own_writes: Arc<AtomicBool>) {
    inner::spawn(app, db, skip_own_writes);
}
