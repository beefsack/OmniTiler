#![forbid(unsafe_code)]
//! Sanitized owner-process panic diagnostics (observability only).
//!
//! Installed as a scoped hook after the per-run owner log exists and before
//! worker threads/native startup, restored on normal scope end. The hook
//! logs one closed-vocabulary line per panic: thread role, Rust source file
//! basename (never a full source/machine path), line/column, and payload
//! *type* only (never the panic message, user data, error string, or OS
//! info). The previous hook is preserved and still runs; unwind/abort/exit
//! policy is unchanged. No Win32 calls, no channel/thread locks, no
//! recursive panics: the file append is best-effort and lock-free.

use std::any::Any;
use std::path::{Path, PathBuf};

/// Closed thread-role vocabulary for the panic record. Derived from the
/// current thread name without logging the name itself.
#[must_use]
pub fn classify_thread_role(name: Option<&str>) -> &'static str {
    match name {
        Some("main") => "owner",
        Some(name) if name.contains("render") => "renderer",
        _ => "other",
    }
}

/// Closed payload-type vocabulary: whether the panic payload was a `&str`,
/// a `String`, or anything else. The payload value is never read.
#[must_use]
pub fn classify_payload_type(payload: &(dyn Any + Send)) -> &'static str {
    if payload.is::<&str>() {
        "str"
    } else if payload.is::<String>() {
        "string"
    } else {
        "other"
    }
}

/// Basename of a Rust source path: the text after the last `/` or `\`.
/// Never returns a directory, machine path, or user name. Empty input maps
/// to `"unknown"`.
#[must_use]
pub fn file_basename(path: &str) -> &str {
    let tail = path.rsplit(['/', '\\']).next().unwrap_or("");
    if tail.is_empty() { "unknown" } else { tail }
}

/// One sanitized panic record. Only closed-vocabulary strings plus numeric
/// line/column: no panic message, no paths, no thread names or ids, no
/// window handles, no user data.
#[must_use]
pub fn panic_event(
    thread_role: &str,
    file: &str,
    line: Option<u32>,
    column: Option<u32>,
    payload_type: &str,
) -> serde_json::Value {
    serde_json::json!({
        "event": "panic",
        "component": "owner",
        "thread_role": thread_role,
        "file": file,
        "line": line,
        "column": column,
        "payload_type": payload_type,
    })
}

/// Preserved panic-hook behavior installed before this scope.
type PreservedHook = std::sync::Arc<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

/// Scoped panic-hook guard: installs the sanitized owner hook on creation
/// and restores the previous hook on normal scope end. The hook delegates
/// to the previous hook after logging, so existing behavior is preserved.
/// While unwinding the hook is deliberately left installed: Rust forbids
/// `set_hook` from a panicking thread (a second panic there would abort),
/// so a guard dropped during a panic keeps the crash diagnostic instead of
/// restoring.
pub struct PanicDiagGuard {
    previous: Option<PreservedHook>,
}

impl PanicDiagGuard {
    /// Install the sanitized hook capturing `log_path` internally (the path
    /// itself is never logged). Best-effort: logging failures are ignored so
    /// the hook can never panic or recurse.
    #[must_use]
    pub fn install(log_path: PathBuf) -> Self {
        let previous: PreservedHook = std::sync::Arc::from(std::panic::take_hook());
        let delegate = std::sync::Arc::clone(&previous);
        std::panic::set_hook(Box::new(move |info: &std::panic::PanicHookInfo<'_>| {
            append_panic_line(&log_path, info);
            delegate(info);
        }));
        Self {
            previous: Some(previous),
        }
    }
}

impl Drop for PanicDiagGuard {
    fn drop(&mut self) {
        // Never `set_hook` while unwinding: Rust forbids it from a
        // panicking thread (the second panic would abort the process,
        // violating observability-only). The sanitized hook then stays
        // installed through process failure, preserving the crash
        // diagnostic; normal scopes restore below. No locks, no logging,
        // no Win32 calls.
        if std::thread::panicking() {
            return;
        }
        // Restore the preserved hook by delegating to the remaining share
        // (LIFO scopes nest correctly; each drop reinstalls the hook its
        // scope replaced).
        if let Some(previous) = self.previous.take() {
            std::panic::set_hook(Box::new(move |info: &std::panic::PanicHookInfo<'_>| {
                previous(info);
            }));
        }
    }
}

/// Best-effort panic line append: lock-free, never panics, never logs the
/// path. Runs before the preserved previous hook.
fn append_panic_line(log_path: &Path, info: &std::panic::PanicHookInfo<'_>) {
    let role = classify_thread_role(std::thread::current().name());
    let payload_type = classify_payload_type(info.payload());
    let (file, line, column) = match info.location() {
        Some(location) => (
            file_basename(location.file()).to_owned(),
            Some(location.line()),
            Some(location.column()),
        ),
        None => ("unknown".to_owned(), None, None),
    };
    let event = panic_event(role, &file, line, column, payload_type);
    append_json_line(log_path, &event);
}

/// Lock-free best-effort JSON line append using the existing owner-writer
/// shape (create/append, single row). Ignores all errors.
fn append_json_line(path: &Path, value: &serde_json::Value) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{value}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes the hook-mutating tests: the panic hook is process-global,
    /// so concurrent hook tests could observe each other's hooks.
    static HOOK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn thread_role_vocabulary_is_closed() {
        assert_eq!(classify_thread_role(Some("main")), "owner");
        assert_eq!(classify_thread_role(Some("omnitiler-render")), "renderer");
        assert_eq!(classify_thread_role(Some("omntiler-pager-icons")), "other");
        assert_eq!(classify_thread_role(Some("worker-1")), "other");
        assert_eq!(classify_thread_role(None), "other");
    }

    #[test]
    fn payload_type_never_carries_the_message() {
        let short: &(dyn Any + Send) = &"secret-message";
        let owned: &(dyn Any + Send) = &String::from("secret-message");
        let other: &(dyn Any + Send) = &42u32;
        assert_eq!(classify_payload_type(short), "str");
        assert_eq!(classify_payload_type(owned), "string");
        assert_eq!(classify_payload_type(other), "other");
    }

    #[test]
    fn basename_strips_directories_and_never_empties() {
        assert_eq!(file_basename("src/panic_diag.rs"), "panic_diag.rs");
        assert_eq!(
            file_basename("C:\\Users\\someone\\repo\\src\\lifecycle.rs"),
            "lifecycle.rs"
        );
        assert_eq!(file_basename("lifecycle.rs"), "lifecycle.rs");
        assert_eq!(file_basename(""), "unknown");
    }

    #[test]
    fn panic_event_shape_carries_no_identity_or_paths() {
        let event = panic_event("renderer", "lifecycle.rs", Some(10), Some(4), "string");
        assert_eq!(event["event"], "panic");
        assert_eq!(event["component"], "owner");
        assert_eq!(event["thread_role"], "renderer");
        assert_eq!(event["file"], "lifecycle.rs");
        assert_eq!(event["line"], 10);
        assert_eq!(event["column"], 4);
        assert_eq!(event["payload_type"], "string");
        let text = serde_json::to_string(&event).expect("event serializes");
        assert!(!text.contains("C:\\"), "no machine path: {text}");
        assert!(!text.contains("secret"), "no payload content: {text}");
    }

    #[test]
    fn scoped_guard_installs_restores_and_delegates() {
        let _serial = HOOK_TEST_LOCK.lock().expect("hook test lock");
        let dir = std::env::temp_dir().join(format!("omnitiler-panic-diag-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log_path = dir.join("panic-scope.log");
        let _ = std::fs::remove_file(&log_path);
        let delegated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = std::sync::Arc::clone(&delegated);
        // A known previous hook the guard must preserve and delegate to.
        // The pre-test hook is saved first so the test leaves no global
        // state behind for other tests' failure messages.
        let saved = std::panic::take_hook();
        let before: Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static> =
            Box::new(move |_| {
                seen.store(true, std::sync::atomic::Ordering::SeqCst);
            });
        std::panic::set_hook(before);
        {
            let _guard = PanicDiagGuard::install(log_path.clone());
            let caught = std::panic::catch_unwind(|| {
                std::panic::panic_any("scoped-secret-must-not-log");
            });
            assert!(caught.is_err(), "probe panic must unwind");
            assert!(
                delegated.load(std::sync::atomic::Ordering::SeqCst),
                "previous hook must still run"
            );
        }
        let text = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(text.contains("\"event\":\"panic\""), "hook logged: {text}");
        assert!(
            !text.contains("scoped-secret-must-not-log"),
            "payload message never logged: {text}"
        );
        assert!(!text.contains("C:\\"), "no machine path: {text}");
        // Normal scope end restored the hook we set: the scoped hook no
        // longer fires, and the preserved hook still delegates.
        delegated.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = std::panic::catch_unwind(|| {
            std::panic::panic_any("after-scope");
        });
        assert!(
            delegated.load(std::sync::atomic::Ordering::SeqCst),
            "restored hook must still run"
        );
        let after = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert_eq!(after, text, "scoped hook must not fire after restore");
        let _ = std::fs::remove_file(&log_path);
        // Restore the pre-test hook so no test-global state leaks.
        std::panic::set_hook(saved);
    }

    #[test]
    fn guard_drop_while_panicking_never_sets_hook() {
        // The guard lives inside the unwound closure, so its Drop runs on
        // the panicking thread: restoring there would `set_hook` while
        // unwinding (second panic/abort). The drop must skip the restore
        // and leave the crash hook installed. Reaching the asserts below
        // proves no abort happened.
        let _serial = HOOK_TEST_LOCK.lock().expect("hook test lock");
        let dir =
            std::env::temp_dir().join(format!("omnitiler-panic-unwind-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log_path = dir.join("panic-unwind.log");
        let _ = std::fs::remove_file(&log_path);
        let delegated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = std::sync::Arc::clone(&delegated);
        let saved = std::panic::take_hook();
        let before: Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static> =
            Box::new(move |_| {
                seen.store(true, std::sync::atomic::Ordering::SeqCst);
            });
        std::panic::set_hook(before);
        let caught = std::panic::catch_unwind(|| {
            let _guard = PanicDiagGuard::install(log_path.clone());
            std::panic::panic_any("unwind-with-guard-live");
        });
        // Restore the pre-test hook before asserting so no failure path
        // leaks global state.
        std::panic::set_hook(saved);
        assert!(caught.is_err(), "probe panic must unwind");
        assert!(
            delegated.load(std::sync::atomic::Ordering::SeqCst),
            "hook must fire for the unwinding panic"
        );
        let text = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(text.contains("\"event\":\"panic\""), "hook logged: {text}");
        assert!(
            !text.contains("unwind-with-guard-live"),
            "payload message never logged: {text}"
        );
        let _ = std::fs::remove_file(&log_path);
    }
}
