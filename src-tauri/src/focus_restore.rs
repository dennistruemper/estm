//! Remember the app in front before the picker opens; restore it when the picker hides.

use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};

/// Last frontmost app PID (not ESTM), captured when the picker is shown.
pub struct PreviousFrontApp(pub Mutex<Option<i32>>);

const ESTM_BUNDLE_ID: &str = "fun.estm";

#[cfg(target_os = "macos")]
mod macos {
    use super::{PreviousFrontApp, ESTM_BUNDLE_ID};
    use objc2_app_kit::{
        NSApplicationActivationOptions, NSRunningApplication, NSWorkspace,
    };
    use objc2_foundation::NSString;
    use std::ffi::CStr;
    use std::time::Duration;
    use tauri::{AppHandle, Runtime};

    const HIDE_SETTLE_MS: u64 = 40;

    fn ns_string_to_string(ns: &NSString) -> Option<String> {
        unsafe {
            let p = ns.UTF8String();
            if p.is_null() {
                return Some(String::new());
            }
            Some(CStr::from_ptr(p).to_string_lossy().into_owned())
        }
    }

    fn is_estm_app(app: &NSRunningApplication) -> bool {
        let Some(bundle) = app.bundleIdentifier() else {
            return false;
        };
        ns_string_to_string(&bundle).as_deref() == Some(ESTM_BUNDLE_ID)
    }

    pub fn capture(state: &PreviousFrontApp) {
        let workspace = NSWorkspace::sharedWorkspace();
        let Some(app) = workspace.frontmostApplication() else {
            return;
        };
        if is_estm_app(&app) {
            return;
        }
        let pid = app.processIdentifier();
        if pid >= 0 {
            if let Ok(mut slot) = state.0.lock() {
                *slot = Some(pid);
            }
        }
    }

    fn activate_pid(pid: i32) -> bool {
        let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
            return false;
        };
        let options = NSApplicationActivationOptions::ActivateAllWindows;
        app.activateWithOptions(options)
    }

    pub fn restore<R: Runtime>(app: &AppHandle<R>, state: &PreviousFrontApp) {
        let pid = match state.0.lock() {
            Ok(mut slot) => slot.take(),
            Err(_) => return,
        };
        let Some(pid) = pid else {
            return;
        };

        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(HIDE_SETTLE_MS));
            let _ = app.run_on_main_thread(move || {
                let _ = activate_pid(pid);
            });
        });
    }
}

#[cfg(target_os = "macos")]
pub fn capture_previous_front_app<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<PreviousFrontApp>() {
        macos::capture(state.inner());
    }
}

#[cfg(not(target_os = "macos"))]
pub fn capture_previous_front_app<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(target_os = "macos")]
pub fn restore_previous_front_app<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<PreviousFrontApp>() {
        macos::restore(app, state.inner());
    }
}

#[cfg(not(target_os = "macos"))]
pub fn restore_previous_front_app<R: Runtime>(_app: &AppHandle<R>) {}
