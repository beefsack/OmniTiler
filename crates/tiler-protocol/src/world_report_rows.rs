//! Protocol-owned canonical v4 screen and window row emission, including held
//! rows. Uses the protocol canonical scalar/raw writer in [`crate::canonical_json`]
//! and reuses [`tiler_core::world_report::retained_window_fields`] so valid and
//! held window rows share one field mapping without duplication.

use crate::canonical_json::{NonFiniteNumber, push_json_number, push_raw_setting_value};
use crate::world_protocol::push_json_string;
use tiler_core::geometry::Rect;
use tiler_core::world_report::{
    HeldScreenRow, HeldWindowRow, ScreenReport, WindowFact, retained_window_fields,
};

/// Exact canonical encoding for one held screen row: the retained carried
/// fields as a JSON object in stored order (canonical schema order at
/// decode: `out`, `full`, `work`, `complete`, `scale`, `identity`, then any
/// other carried keys lexicographically). Checked: a non-finite retained
/// number is an error, never a silent `null`.
pub fn encode_held_screen_canonical(held: &HeldScreenRow) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    out.push('{');
    for (i, (key, value)) in held.fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, key);
        out.push(':');
        push_raw_setting_value(&mut out, value)?;
    }
    out.push('}');
    Ok(out)
}

/// Exact canonical encoding for one held window row: the retained carried
/// fields as a JSON object in stored order (canonical schema order at
/// decode: `win`, `out`, `rect`, `fullscreen`, `maximized`, `sticky`,
/// `minimized`, `hints`, `nativeWs`, `hostHidden`, `privileged`, `managed`,
/// `acceptsInput`, `activationInhibited`, `toolWindow`, `owned`,
/// `transient`, `modal`, `resizable`, `above`, `below`, `owner`, `parent`,
/// `role`, `appId`, `executable`, `processId`, `class`, `title`, `type`,
/// `activationPolicy`, `layer`, `fullscreenOwner`, `fullscreenExitable`,
/// `suppressionAvailable`, `visibility`, `lastWrite`, then any other carried
/// keys lexicographically, with the nested `hints`, `visibility`, and
/// `lastWrite` objects in schema order). Checked: a non-finite retained
/// number is an error, never a silent `null`.
pub fn encode_held_window_canonical(held: &HeldWindowRow) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    out.push('{');
    for (i, (key, value)) in held.fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, key);
        out.push(':');
        push_raw_setting_value(&mut out, value)?;
    }
    out.push('}');
    Ok(out)
}

/// Exact canonical encoding for one valid window row: schema key order
/// (`win`, `out`, `rect`, `fullscreen`, `maximized`, `sticky`, `minimized`,
/// `hints`, `nativeWs`, `hostHidden`, `privileged`, `managed`, `acceptsInput`,
/// `activationInhibited`, `toolWindow`, `owned`, `transient`, `modal`,
/// `resizable`, `above`, `below`, `owner`, `parent`, `role`, `appId`,
/// `executable`, `processId`, `class`, `title`, `type`, `activationPolicy`,
/// `layer`, `fullscreenOwner`, `fullscreenExitable`, `suppressionAvailable`,
/// `visibility`, `lastWrite`, with the nested `hints`, `visibility`, and
/// `lastWrite` objects in schema order). Reuses [`retained_window_fields`]
/// so the spelling cannot drift from the held-row writer.
pub fn encode_window_canonical(fact: &WindowFact) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    out.push('{');
    let mut first = true;
    for (key, value) in retained_window_fields(fact) {
        if !first {
            out.push(',');
        }
        first = false;
        push_json_string(&mut out, &key);
        out.push(':');
        push_raw_setting_value(&mut out, &value)?;
    }
    out.push('}');
    Ok(out)
}

/// Exact canonical encoding for one valid screen row: schema key order
/// (`out`, `full`, `work`, `complete`, `scale`, `identity`) with
/// schema-ordered identity and shared canonical scalars. Checked: a
/// non-finite scale is an error, never a silent `null`.
pub fn encode_screen_canonical(screen: &ScreenReport) -> Result<String, NonFiniteNumber> {
    fn push_rect(out: &mut String, rect: Option<Rect>) {
        match rect {
            None => out.push_str("null"),
            Some(rect) => {
                out.push('[');
                out.push_str(&rect.x.to_string());
                out.push(',');
                out.push_str(&rect.y.to_string());
                out.push(',');
                out.push_str(&rect.w.to_string());
                out.push(',');
                out.push_str(&rect.h.to_string());
                out.push(']');
            }
        }
    }
    fn push_identity_text(out: &mut String, text: &Option<String>) {
        match text {
            None => out.push_str("null"),
            Some(text) => push_json_string(out, text),
        }
    }
    let mut out = String::new();
    out.push_str("{\"out\":");
    push_json_string(&mut out, &screen.output.0);
    out.push_str(",\"full\":");
    push_rect(&mut out, screen.full_rect);
    out.push_str(",\"work\":");
    push_rect(&mut out, screen.work_area);
    out.push_str(",\"complete\":");
    out.push_str(if screen.complete { "true" } else { "false" });
    out.push_str(",\"scale\":");
    push_json_number(&mut out, screen.scale)?;
    out.push_str(",\"identity\":{\"manufacturer\":");
    push_identity_text(&mut out, &screen.identity.manufacturer);
    out.push_str(",\"model\":");
    push_identity_text(&mut out, &screen.identity.model);
    out.push_str(",\"serial\":");
    push_identity_text(&mut out, &screen.identity.serial);
    out.push_str(",\"connector\":");
    push_identity_text(&mut out, &screen.identity.connector);
    out.push_str("}}");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::connection::RawSettingValue;
    use tiler_core::directional::OutputId;
    use tiler_core::world_report::{
        AuthorityScope, OutputIdentity, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane,
    };

    #[test]
    fn non_finite_rows_are_checked_errors_never_null() {
        // A non-finite scale never encodes as a valid row.
        let screen = ScreenReport {
            output: OutputId("out".to_owned()),
            full_rect: None,
            work_area: None,
            complete: false,
            scale: f64::NAN,
            identity: OutputIdentity::unknown(),
        };
        assert_eq!(encode_screen_canonical(&screen), Err(NonFiniteNumber));

        // A non-finite retained field never encodes as a held row.
        let held = HeldScreenRow {
            output: OutputId("out".to_owned()),
            ordinal: 0,
            fields: vec![("scale".to_owned(), RawSettingValue::Number(f64::INFINITY))],
            diagnostic: RowDiagnostic {
                scope: AuthorityScope::Output(OutputId("out".to_owned())),
                row: RowDiagnosticLane::Screen,
                id: Some("out".to_owned()),
                kind: RowDiagnosticKind::InvalidFact,
            },
        };
        assert_eq!(encode_held_screen_canonical(&held), Err(NonFiniteNumber));
    }
}
