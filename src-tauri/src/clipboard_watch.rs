//! Polls system clipboard change count on macOS; inactive elsewhere.
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

    fn pasteboard_plaintext() -> Option<String> {
        let pb = NSPasteboard::generalPasteboard();
        let s_ret = pb.stringForType(ns_string!("public.utf8-plain-text"));
        let ns = s_ret.as_deref()?;
        let rust = nsstring_utf8(ns)?;
        if rust.is_empty() {
            None
        } else {
            Some(rust)
        }
    }

    pub fn spawn(app: AppHandle, db: SharedDb, skip_own_clipboard_writes: Arc<AtomicBool>) {
        thread::spawn(move || {
            let mut seeded = false;
            let mut last_count: isize = 0;

            loop {
                thread::sleep(Duration::from_millis(120));

                let pb = NSPasteboard::generalPasteboard();
                let count = pb.changeCount();

                if !seeded {
                    seeded = true;
                    last_count = count;
                    continue;
                }

                if count == last_count {
                    continue;
                }
                last_count = count;

                if skip_own_clipboard_writes.swap(false, Ordering::SeqCst) {
                    continue;
                }

                let Some(txt) = pasteboard_plaintext() else {
                    continue;
                };

                let outcome = match db.lock() {
                    Ok(conn) => {
                        let limits = crate::settings::load().history_limits.to_db_limits();
                        db::record_plaintext_capture(&conn, txt, limits)
                    }
                    Err(_) => {
                        eprintln!("[ESTM] db mutex poisoned");
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                };

                match outcome {
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
