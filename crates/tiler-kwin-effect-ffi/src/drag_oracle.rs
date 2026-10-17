#![allow(unsafe_code)]

use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicU64, Ordering},
};
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
// Optional bounded press evidence carried atomically with the final verdict.
// The C++ effect captures the pointer press passively (InputEventSpy, no
// grab/interception) and matches it to the same effect window plus identity
// within a bounded monotonic age at drag start. Rust never classifies
// thirds/move-vs-resize: a valid press is echoed verbatim so the script
// adapter can gate on its own start.resize knowledge plus presence.
// Field order mirrors the C++ DragOraclePress layout exactly (C layout).
// binding is 1 when the press matched the live configured
// MouseUnrestrictedResize binding, 0 when it matched the Alt+Right fallback
// used only while the public options are unavailable.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragPress {
    pub has_press: u8,
    pub binding: u8,
    pub x: f64,
    pub y: f64,
}
impl DragPress {
    pub const fn absent() -> DragPress {
        DragPress {
            has_press: 0,
            binding: 0,
            x: 0.0,
            y: 0.0,
        }
    }
}
const _: () = assert!(std::mem::size_of::<DragPress>() == 24);
pub const DRAG_ORACLE_MAX_ID_LEN: usize = 128;
pub const DRAG_ORACLE_MAX_JSON: usize = 1024;
pub const DRAG_ORACLE_PRESS_MAX_ABS: f64 = 16384.0;
pub const DRAG_PRESS_BINDING_DEFAULT: u8 = 0;
pub const DRAG_PRESS_BINDING_CONFIGURED: u8 = 1;
const COORD_LIMIT: i32 = 16384;
static CORRELATION: AtomicU64 = AtomicU64::new(0);
/// Trusted cancelled verdict used to reset poisoned storage. It is served
/// only for reads arriving before any fresh observation after recovery;
/// poisoned reads themselves fail closed (null/0) and never serve the
/// untrusted stale bytes.
const POISON_RECOVERY_JSON: &[u8] = br#"{"v":1,"cancelled":true,"finalRect":{"x":0,"y":0,"w":1,"h":1},"windowIdentity":"","correlation":"drag-0","reason":"no-observation"}"#;
static LAST_JSON: LazyLock<Mutex<Vec<u8>>> =
    LazyLock::new(|| Mutex::new(POISON_RECOVERY_JSON.to_vec()));
fn bad_dim(r: DragRect) -> bool {
    r.w <= 0 || r.h <= 0
}
fn bad_range(r: DragRect) -> bool {
    r.x < -COORD_LIMIT
        || r.x > COORD_LIMIT
        || r.y < -COORD_LIMIT
        || r.y > COORD_LIMIT
        || r.w > COORD_LIMIT
        || r.h > COORD_LIMIT
}
fn identity_reason(id: &[u8]) -> Option<&'static str> {
    if id.len() > DRAG_ORACLE_MAX_ID_LEN {
        return Some("identity-too-long");
    }
    if id.is_empty() {
        return Some("empty-identity");
    }
    if !id
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_' || *b == b'.')
    {
        return Some("identity-invalid");
    }
    None
}
// A press is echoed only when fully verified: explicit presence flag,
// closed-vocabulary binding, and finite coordinates inside the shared
// carried-geometry bound. Anything else reads as absent, never a rejection:
// the exact existing cancelled/reason contract is preserved.
fn verified_press(press: DragPress) -> Option<(f64, f64, &'static str)> {
    if press.has_press != 1 {
        return None;
    }
    let binding = match press.binding {
        DRAG_PRESS_BINDING_CONFIGURED => "configured",
        DRAG_PRESS_BINDING_DEFAULT => "default",
        _ => return None,
    };
    if !press.x.is_finite() || !press.y.is_finite() {
        return None;
    }
    if press.x.abs() > DRAG_ORACLE_PRESS_MAX_ABS || press.y.abs() > DRAG_ORACLE_PRESS_MAX_ABS {
        return None;
    }
    Some((press.x, press.y, binding))
}
pub fn build_verdict(
    start: DragRect,
    end: DragRect,
    id: &[u8],
    press: DragPress,
) -> (Vec<u8>, u64) {
    let correlation = CORRELATION.fetch_add(1, Ordering::SeqCst) + 1;
    let (cancelled, reason) = if bad_dim(start) || bad_dim(end) {
        (true, "geometry-invalid")
    } else if bad_range(start) || bad_range(end) {
        (true, "geometry-out-of-range")
    } else if let Some(why) = identity_reason(id) {
        (true, why)
    } else if start == end {
        (true, "no-change")
    } else {
        (false, "ok-moved")
    };
    let id_out = if identity_reason(id).is_none() {
        id
    } else {
        b""
    };
    let mut out = format!(
        "{{\"v\":1,\"cancelled\":{},\"finalRect\":{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}},\"windowIdentity\":\"{}\",\"correlation\":\"drag-{}\",\"reason\":\"{}\"",
        cancelled,
        end.x,
        end.y,
        end.w,
        end.h,
        String::from_utf8_lossy(id_out),
        correlation,
        reason
    );
    // Optional press evidence appends INSIDE the outer object atomically with
    // the same verdict reply: no second D-Bus call, no separate service or
    // object. Debug formatting keeps full f64 precision so script thirds
    // comparisons match the native press exactly; values are finite and
    // bounded above.
    if let Some((x, y, binding)) = verified_press(press) {
        out.push_str(&format!(
            ",\"press\":{{\"x\":{x:?},\"y\":{y:?},\"binding\":\"{binding}\"}}"
        ));
    }
    out.push('}');
    let mut bytes = out.into_bytes();
    debug_assert!(bytes.len() <= DRAG_ORACLE_MAX_JSON);
    bytes.truncate(DRAG_ORACLE_MAX_JSON);
    (bytes, correlation)
}
fn store_last(json: &[u8]) {
    match LAST_JSON.lock() {
        Ok(mut guard) => {
            guard.clear();
            guard.extend_from_slice(json);
        }
        // Row Q: a previous panic under the verdict lock poisoned storage.
        // Recover at the boundary by storing this fresh observation instead
        // of silently dropping it; the fresh bytes replace the untrusted
        // stale ones.
        Err(poison) => {
            let mut guard = poison.into_inner();
            guard.clear();
            guard.extend_from_slice(json);
            LAST_JSON.clear_poison();
        }
    }
}
fn record_inner(start: DragRect, end: DragRect, id: &[u8], press: DragPress) -> u64 {
    let (json, correlation) = build_verdict(start, end, id, press);
    store_last(&json);
    correlation
}
#[unsafe(no_mangle)]
pub extern "C" fn drag_oracle_record(
    start: DragRect,
    end: DragRect,
    id_ptr: *const u8,
    id_len: usize,
    press: DragPress,
) -> u64 {
    match std::panic::catch_unwind(|| {
        let id: &[u8] = if id_len == 0 || id_ptr.is_null() {
            &[]
        } else if id_len > 4096 {
            return record_inner(start, end, &vec![0u8; DRAG_ORACLE_MAX_ID_LEN + 1], press);
        } else {
            unsafe { std::slice::from_raw_parts(id_ptr, id_len) }
        };
        record_inner(start, end, id, press)
    }) {
        Ok(correlation) => correlation,
        Err(_) => {
            let _ = std::panic::catch_unwind(|| {
                store_last(br#"{"v":1,"cancelled":true,"finalRect":{"x":0,"y":0,"w":1,"h":1},"windowIdentity":"","correlation":"drag-0","reason":"oracle-panic"}"#)
            });
            0
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn drag_oracle_last(out_len: *mut usize) -> *const u8 {
    match std::panic::catch_unwind(|| {
        if out_len.is_null() {
            return std::ptr::null();
        }
        match LAST_JSON.lock() {
            Ok(guard) => {
                unsafe {
                    *out_len = guard.len();
                }
                guard.as_ptr()
            }
            // Row Q: poisoned storage never serves the untrusted stale
            // bytes. Reset to the trusted cancelled verdict and fail closed
            // (null) for this read; FFI panic containment is unchanged and
            // the next fresh record round-trips normally.
            Err(poison) => {
                let mut guard = poison.into_inner();
                guard.clear();
                guard.extend_from_slice(POISON_RECOVERY_JSON);
                LAST_JSON.clear_poison();
                std::ptr::null()
            }
        }
    }) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn drag_oracle_last_copy(out: *mut u8, capacity: usize) -> usize {
    match std::panic::catch_unwind(|| {
        if out.is_null() || capacity == 0 {
            return 0;
        }
        match LAST_JSON.lock() {
            Ok(guard) => {
                let take = guard.len().min(capacity);
                unsafe {
                    std::ptr::copy_nonoverlapping(guard.as_ptr(), out, take);
                }
                take
            }
            // Row Q: same fail-closed recovery as the borrowed read: reset
            // to the trusted verdict, serve nothing (0) for this read, and
            // leave storage usable for the next fresh record.
            Err(poison) => {
                let mut guard = poison.into_inner();
                guard.clear();
                guard.extend_from_slice(POISON_RECOVERY_JSON);
                LAST_JSON.clear_poison();
                0
            }
        }
    }) {
        Ok(take) => take,
        Err(_) => 0,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    static LAST_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn id(text: &str) -> Vec<u8> {
        text.as_bytes().to_vec()
    }

    fn press(binding: u8, x: f64, y: f64) -> DragPress {
        DragPress {
            has_press: 1,
            binding,
            x,
            y,
        }
    }

    fn reason_of(json: &[u8]) -> String {
        let text = std::str::from_utf8(json).expect("verdict is ASCII JSON");
        let marker = "\"reason\":\"";
        let start = text.find(marker).expect("reason field") + marker.len();
        let rest = &text[start..];
        rest[..rest.find('"').expect("reason end")].to_string()
    }

    fn cancelled_of(json: &[u8]) -> bool {
        std::str::from_utf8(json)
            .expect("verdict is ASCII JSON")
            .contains("\"cancelled\":true")
    }

    // Minimal structural shape check for what TS will JSON.parse: the reply
    // ends with the outer closing brace and braces balance outside strings.
    // Exact full-string equality in each test below pins the field order.
    fn assert_json_shape(text: &str) {
        assert!(
            text.starts_with('{') && text.ends_with('}'),
            "verdict must be one closed object: {}",
            text
        );
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;
        for c in text.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    assert!(depth >= 0, "unbalanced braces: {}", text);
                }
                _ => {}
            }
        }
        assert!(!in_string && depth == 0, "unbalanced shape: {}", text);
    }

    #[test]
    fn equal_geometry_reports_no_change_cancelled() {
        let r = DragRect {
            x: 10,
            y: 20,
            w: 300,
            h: 200,
        };
        let (json, _) = build_verdict(r, r, &id("win-1"), DragPress::absent());
        assert!(cancelled_of(&json));
        assert_eq!(reason_of(&json), "no-change");
    }

    #[test]
    fn moved_geometry_reports_ok_not_cancelled() {
        let start = DragRect {
            x: 10,
            y: 20,
            w: 300,
            h: 200,
        };
        let end = DragRect {
            x: 60,
            y: 20,
            w: 300,
            h: 200,
        };
        let (json, _) = build_verdict(start, end, &id("win-1"), DragPress::absent());
        assert!(!cancelled_of(&json));
        assert_eq!(reason_of(&json), "ok-moved");
    }

    #[test]
    fn empty_identity_has_specific_reason() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let end = DragRect {
            x: 5,
            y: 0,
            w: 100,
            h: 100,
        };
        let (json, _) = build_verdict(start, end, &[], DragPress::absent());
        assert!(cancelled_of(&json));
        assert_eq!(reason_of(&json), "empty-identity");
    }

    #[test]
    fn bad_charset_identity_has_specific_reason() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let end = DragRect {
            x: 5,
            y: 0,
            w: 100,
            h: 100,
        };
        let (json, _) = build_verdict(start, end, b"win\";drop", DragPress::absent());
        assert!(cancelled_of(&json));
        assert_eq!(reason_of(&json), "identity-invalid");
    }

    #[test]
    fn oversize_identity_has_specific_reason() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let end = DragRect {
            x: 5,
            y: 0,
            w: 100,
            h: 100,
        };
        let (json, _) = build_verdict(start, end, &vec![b'a'; 129], DragPress::absent());
        assert!(cancelled_of(&json));
        assert_eq!(reason_of(&json), "identity-too-long");
    }

    #[test]
    fn degenerate_geometry_has_specific_reason() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        let end = DragRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let (json, _) = build_verdict(start, end, &id("win-1"), DragPress::absent());
        assert!(cancelled_of(&json));
        assert_eq!(reason_of(&json), "geometry-invalid");
    }

    #[test]
    fn out_of_range_geometry_has_specific_reason() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let end = DragRect {
            x: 20000,
            y: 0,
            w: 100,
            h: 100,
        };
        let (json, _) = build_verdict(start, end, &id("win-1"), DragPress::absent());
        assert!(cancelled_of(&json));
        assert_eq!(reason_of(&json), "geometry-out-of-range");
    }

    #[test]
    fn verdict_carries_required_fields_and_stays_bounded() {
        let start = DragRect {
            x: 1,
            y: 2,
            w: 3,
            h: 4,
        };
        let end = DragRect {
            x: 5,
            y: 6,
            w: 7,
            h: 8,
        };
        let (json, correlation) = build_verdict(start, end, &id("win-9"), DragPress::absent());
        let text = std::str::from_utf8(&json).expect("verdict is ASCII JSON");
        assert!(text.contains("\"cancelled\":"));
        assert!(text.contains("\"finalRect\":{\"x\":5,\"y\":6,\"w\":7,\"h\":8}"));
        assert!(text.contains("\"windowIdentity\":\"win-9\""));
        assert!(text.contains(&format!("\"correlation\":\"drag-{correlation}\"")));
        assert!(json.len() <= DRAG_ORACLE_MAX_JSON);
    }

    #[test]
    fn absent_press_emits_no_press_key() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let end = DragRect {
            x: 5,
            y: 0,
            w: 100,
            h: 100,
        };
        let (json, correlation) = build_verdict(start, end, &id("win-1"), DragPress::absent());
        let text = std::str::from_utf8(&json).expect("verdict is ASCII JSON");
        assert_json_shape(text);
        // Exact legacy shape is preserved byte-for-byte when no press is verified.
        let expected = format!(
            "{{\"v\":1,\"cancelled\":false,\"finalRect\":{{\"x\":5,\"y\":0,\"w\":100,\"h\":100}},\"windowIdentity\":\"win-1\",\"correlation\":\"drag-{correlation}\",\"reason\":\"ok-moved\"}}"
        );
        assert_eq!(text, expected);
        assert_eq!(reason_of(&json), "ok-moved");
    }

    #[test]
    fn configured_press_is_echoed_atomically_with_verdict() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 300,
            h: 200,
        };
        let end = DragRect {
            x: 10,
            y: 0,
            w: 300,
            h: 200,
        };
        let (json, correlation) = build_verdict(
            start,
            end,
            &id("win-1"),
            press(DRAG_PRESS_BINDING_CONFIGURED, 250.5, 100.25),
        );
        let text = std::str::from_utf8(&json).expect("verdict is ASCII JSON");
        assert_json_shape(text);
        assert!(!cancelled_of(&json));
        assert_eq!(reason_of(&json), "ok-moved");
        let expected = format!(
            "{{\"v\":1,\"cancelled\":false,\"finalRect\":{{\"x\":10,\"y\":0,\"w\":300,\"h\":200}},\"windowIdentity\":\"win-1\",\"correlation\":\"drag-{correlation}\",\"reason\":\"ok-moved\",\"press\":{{\"x\":250.5,\"y\":100.25,\"binding\":\"configured\"}}}}"
        );
        assert_eq!(text, expected);
        assert!(json.len() <= DRAG_ORACLE_MAX_JSON);
    }

    #[test]
    fn default_binding_press_is_echoed_with_default_token() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 300,
            h: 200,
        };
        let end = DragRect {
            x: 10,
            y: 0,
            w: 300,
            h: 200,
        };
        let (json, correlation) = build_verdict(
            start,
            end,
            &id("win-1"),
            press(DRAG_PRESS_BINDING_DEFAULT, 12.0, 34.0),
        );
        let text = std::str::from_utf8(&json).expect("verdict is ASCII JSON");
        assert_json_shape(text);
        let expected = format!(
            "{{\"v\":1,\"cancelled\":false,\"finalRect\":{{\"x\":10,\"y\":0,\"w\":300,\"h\":200}},\"windowIdentity\":\"win-1\",\"correlation\":\"drag-{correlation}\",\"reason\":\"ok-moved\",\"press\":{{\"x\":12.0,\"y\":34.0,\"binding\":\"default\"}}}}"
        );
        assert_eq!(text, expected);
    }

    #[test]
    fn press_survives_cancelled_verdict_without_changing_reason() {
        // The press is drag-start evidence, independent of the final geometry
        // verdict: a no-change drag still carries the verified press so the
        // adapter observes presence uniformly.
        let r = DragRect {
            x: 10,
            y: 20,
            w: 300,
            h: 200,
        };
        let (json, correlation) = build_verdict(
            r,
            r,
            &id("win-1"),
            press(DRAG_PRESS_BINDING_CONFIGURED, 20.0, 30.0),
        );
        let text = std::str::from_utf8(&json).expect("verdict is ASCII JSON");
        assert_json_shape(text);
        assert!(cancelled_of(&json));
        assert_eq!(reason_of(&json), "no-change");
        let expected = format!(
            "{{\"v\":1,\"cancelled\":true,\"finalRect\":{{\"x\":10,\"y\":20,\"w\":300,\"h\":200}},\"windowIdentity\":\"win-1\",\"correlation\":\"drag-{correlation}\",\"reason\":\"no-change\",\"press\":{{\"x\":20.0,\"y\":30.0,\"binding\":\"configured\"}}}}"
        );
        assert_eq!(text, expected);
    }

    #[test]
    fn invalid_press_reads_as_absent_never_a_rejection() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let end = DragRect {
            x: 5,
            y: 0,
            w: 100,
            h: 100,
        };
        let bad = [
            DragPress {
                has_press: 0,
                binding: 1,
                x: 1.0,
                y: 1.0,
            },
            DragPress {
                has_press: 2,
                binding: 1,
                x: 1.0,
                y: 1.0,
            },
            DragPress {
                has_press: 1,
                binding: 7,
                x: 1.0,
                y: 1.0,
            },
            DragPress {
                has_press: 1,
                binding: 1,
                x: f64::NAN,
                y: 1.0,
            },
            DragPress {
                has_press: 1,
                binding: 1,
                x: 1.0,
                y: f64::INFINITY,
            },
            DragPress {
                has_press: 1,
                binding: 1,
                x: f64::NEG_INFINITY,
                y: 1.0,
            },
            DragPress {
                has_press: 1,
                binding: 1,
                x: 16384.5,
                y: 1.0,
            },
            DragPress {
                has_press: 1,
                binding: 0,
                x: 1.0,
                y: -16385.0,
            },
        ];
        for press in bad {
            let (json, _) = build_verdict(start, end, &id("win-1"), press);
            let text = std::str::from_utf8(&json).expect("verdict is ASCII JSON");
            assert_json_shape(text);
            assert!(!cancelled_of(&json));
            assert_eq!(reason_of(&json), "ok-moved");
            assert!(!text.contains("\"press\""), "invalid press must omit");
        }
    }

    #[test]
    fn press_boundary_coordinates_are_echoed() {
        let start = DragRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let end = DragRect {
            x: 5,
            y: 0,
            w: 100,
            h: 100,
        };
        let (json, correlation) = build_verdict(
            start,
            end,
            &id("win-1"),
            press(DRAG_PRESS_BINDING_CONFIGURED, -16384.0, 16384.0),
        );
        let text = std::str::from_utf8(&json).expect("verdict is ASCII JSON");
        assert_json_shape(text);
        let expected = format!(
            "{{\"v\":1,\"cancelled\":false,\"finalRect\":{{\"x\":5,\"y\":0,\"w\":100,\"h\":100}},\"windowIdentity\":\"win-1\",\"correlation\":\"drag-{correlation}\",\"reason\":\"ok-moved\",\"press\":{{\"x\":-16384.0,\"y\":16384.0,\"binding\":\"configured\"}}}}"
        );
        assert_eq!(text, expected);
    }

    #[test]
    fn ffi_record_and_last_round_trip_without_unwind() {
        let _guard = LAST_TEST_LOCK.lock().unwrap();
        let start = DragRect {
            x: 0,
            y: 0,
            w: 120,
            h: 90,
        };
        let end = DragRect {
            x: 10,
            y: 0,
            w: 120,
            h: 90,
        };
        let bytes = id("ffi-win_1.2");
        let correlation =
            drag_oracle_record(start, end, bytes.as_ptr(), bytes.len(), DragPress::absent());
        assert!(correlation > 0);
        let mut len = 0usize;
        let ptr = drag_oracle_last(&mut len as *mut usize);
        assert!(!ptr.is_null() && len > 0 && len <= DRAG_ORACLE_MAX_JSON);
        // SAFETY: `ptr`/`len` borrow the oracle's last verdict for this read.
        let view = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert_eq!(reason_of(view), "ok-moved");
    }

    #[test]
    fn poisoned_storage_fails_closed_then_recovers_on_fresh_record() {
        let _guard = LAST_TEST_LOCK.lock().unwrap();
        // Row Q: poisoned verdict storage never serves the stale verdict;
        // poisoned reads fail closed and reset storage, and the next fresh
        // record round-trips. FFI panic containment (no unwind, null/0 on
        // failure) is unchanged.
        let start = DragRect {
            x: 0,
            y: 0,
            w: 120,
            h: 90,
        };
        let end = DragRect {
            x: 10,
            y: 0,
            w: 120,
            h: 90,
        };
        let bait = id("poison-stale-bait");
        assert!(drag_oracle_record(start, end, bait.as_ptr(), bait.len(), DragPress::absent()) > 0);
        let poisoned = std::panic::catch_unwind(|| {
            let _guard = LAST_JSON.lock().unwrap();
            panic!("inject drag oracle poison");
        });
        assert!(poisoned.is_err());
        assert!(LAST_JSON.is_poisoned());
        let mut len = 0usize;
        assert!(
            drag_oracle_last(&mut len as *mut usize).is_null(),
            "poisoned borrow must fail closed, never serve stale bytes"
        );
        assert!(
            !LAST_JSON.is_poisoned(),
            "poisoned reads must reset storage"
        );
        let repoisoned = std::panic::catch_unwind(|| {
            let _guard = LAST_JSON.lock().unwrap();
            panic!("inject drag oracle poison again");
        });
        assert!(repoisoned.is_err());
        let mut buf = vec![0u8; DRAG_ORACLE_MAX_JSON];
        assert_eq!(
            drag_oracle_last_copy(buf.as_mut_ptr(), buf.len()),
            0,
            "poisoned copy must serve nothing"
        );
        assert!(
            !LAST_JSON.is_poisoned(),
            "poisoned reads must reset storage"
        );
        let fresh = id("poison-recover-1");
        assert!(
            drag_oracle_record(start, end, fresh.as_ptr(), fresh.len(), DragPress::absent()) > 0
        );
        let taken = drag_oracle_last_copy(buf.as_mut_ptr(), buf.len());
        assert!(taken > 0 && taken <= DRAG_ORACLE_MAX_JSON);
        let text = std::str::from_utf8(&buf[..taken]).expect("verdict is ASCII JSON");
        assert_json_shape(text);
        assert!(text.contains("\"windowIdentity\":\"poison-recover-1\""));
        assert!(
            !text.contains("poison-stale-bait"),
            "stale verdict is never re-served"
        );
        assert_eq!(reason_of(&buf[..taken]), "ok-moved");
    }

    #[test]
    fn ffi_record_with_press_round_trips_evidence_without_unwind() {
        let _guard = LAST_TEST_LOCK.lock().unwrap();
        let start = DragRect {
            x: 0,
            y: 0,
            w: 120,
            h: 90,
        };
        let end = DragRect {
            x: 10,
            y: 0,
            w: 120,
            h: 90,
        };
        let bytes = id("ffi-press-1");
        let correlation = drag_oracle_record(
            start,
            end,
            bytes.as_ptr(),
            bytes.len(),
            press(DRAG_PRESS_BINDING_CONFIGURED, 7.5, 9.25),
        );
        assert!(correlation > 0);
        let mut buf = vec![0u8; DRAG_ORACLE_MAX_JSON];
        let taken = drag_oracle_last_copy(buf.as_mut_ptr(), buf.len());
        assert!(taken > 0 && taken <= DRAG_ORACLE_MAX_JSON);
        let text = std::str::from_utf8(&buf[..taken]).expect("verdict is ASCII JSON");
        assert_json_shape(text);
        let expected = format!(
            "{{\"v\":1,\"cancelled\":false,\"finalRect\":{{\"x\":10,\"y\":0,\"w\":120,\"h\":90}},\"windowIdentity\":\"ffi-press-1\",\"correlation\":\"drag-{correlation}\",\"reason\":\"ok-moved\",\"press\":{{\"x\":7.5,\"y\":9.25,\"binding\":\"configured\"}}}}"
        );
        assert_eq!(text, expected);
    }

    #[test]
    fn ffi_null_identity_reports_empty_identity_reason() {
        let _guard = LAST_TEST_LOCK.lock().unwrap();
        let r = DragRect {
            x: 0,
            y: 0,
            w: 50,
            h: 50,
        };
        let moved = DragRect {
            x: 1,
            y: 0,
            w: 50,
            h: 50,
        };
        let correlation = drag_oracle_record(r, moved, std::ptr::null(), 0, DragPress::absent());
        assert!(correlation > 0);
        let mut len = 0usize;
        let ptr = drag_oracle_last(&mut len as *mut usize);
        assert!(!ptr.is_null());
        // SAFETY: test-only immediate read of the just-stored verdict.
        let view = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert_eq!(reason_of(view), "empty-identity");
    }

    #[test]
    fn ffi_null_out_len_is_null_without_unwind() {
        let _guard = LAST_TEST_LOCK.lock().unwrap();
        assert!(drag_oracle_last(std::ptr::null_mut()).is_null());
    }

    #[test]
    fn ffi_copy_out_matches_borrowed_view_without_unwind() {
        let _guard = LAST_TEST_LOCK.lock().unwrap();
        let start = DragRect {
            x: 0,
            y: 0,
            w: 120,
            h: 90,
        };
        let end = DragRect {
            x: 11,
            y: 0,
            w: 120,
            h: 90,
        };
        let bytes = id("copy-win-1");
        assert!(
            drag_oracle_record(start, end, bytes.as_ptr(), bytes.len(), DragPress::absent()) > 0
        );
        let mut len = 0usize;
        let ptr = drag_oracle_last(&mut len as *mut usize);
        assert!(!ptr.is_null() && len > 0);
        // SAFETY: immediate same-thread read of the just-stored verdict.
        let borrowed = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
        let mut buf = vec![0u8; DRAG_ORACLE_MAX_JSON];
        let taken = drag_oracle_last_copy(buf.as_mut_ptr(), buf.len());
        assert_eq!(taken, borrowed.len());
        assert_eq!(&buf[..taken], &borrowed[..]);
        assert_eq!(reason_of(&buf[..taken]), "ok-moved");
        assert_eq!(drag_oracle_last_copy(std::ptr::null_mut(), 64), 0);
        assert_eq!(drag_oracle_last_copy(buf.as_mut_ptr(), 0), 0);
    }
}
