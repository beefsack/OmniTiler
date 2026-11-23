#![forbid(unsafe_code)]

//! Read-only pre-attribution observation of the pointer under the cursor.
//! Reports the live cursor position, AX trust, the raw element role/subrole,
//! and the positional hit owner. No synthetic posts, no settings writes, and
//! no attribution or window mapping.

#[cfg(target_os = "macos")]
pub fn run() -> Result<String, (i32, String)> {
    let (x, y) = crate::synth::cursor_position()
        .ok_or_else(|| (1, "cursor position unreadable".to_owned()))?;
    let trusted = crate::ffi::ax_is_process_trusted();
    let (element, element_error) = match crate::ffi::ax_element_at_position(x, y) {
        Ok(element) => {
            let role = crate::ffi::ax_string_attribute(&element, "AXRole")
                .ok()
                .flatten();
            let subrole = crate::ffi::ax_string_attribute(&element, "AXSubrole")
                .ok()
                .flatten();
            (
                serde_json::json!({"role": role, "subrole": subrole}),
                None,
            )
        }
        Err(error) => (serde_json::Value::Null, Some(error)),
    };
    let (hit, hit_error) = match crate::ffi::ax_hit_test_owner(x, y) {
        Ok((pid, wid)) => (serde_json::json!({"pid": pid, "wid": wid}), None),
        Err(error) => (serde_json::Value::Null, Some(error)),
    };
    let body = serde_json::json!({
        "action": "cursor-probe",
        "cursor": {"x": x, "y": y},
        "ax_trusted": trusted,
        "element": element,
        "element_error": element_error,
        "hit": hit,
        "hit_error": hit_error,
    });
    Ok(body.to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn run() -> Result<String, (i32, String)> {
    Err((1, "cursor-probe is macOS-only".to_owned()))
}
