//! Protocol-owned canonical v4 screen and window row emission, including held
//! rows. Uses the protocol canonical scalar/raw writer in [`crate::canonical_json`].

use crate::canonical_json::{NonFiniteNumber, push_json_number, push_raw_setting_value};
use crate::world_protocol::push_json_string;
use tiler_core::connection::RawSettingValue;
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::world_report::{NativeWorkspaceFact, ScreenReport, VisibilityFact, WindowFact};

/// Exact canonical encoding for one held screen row: the retained carried
/// fields as a JSON object in stored order (canonical schema order at
/// decode: `out`, `full`, `work`, `complete`, `scale`, `identity`, then any
/// other carried keys lexicographically). Checked: a non-finite retained
/// number is an error, never a silent `null`.
pub fn encode_held_screen_canonical(
    fields: &[(String, RawSettingValue)],
) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    out.push('{');
    for (i, (key, value)) in fields.iter().enumerate() {
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
/// `suppressionAvailable`, `visibility`, then any other carried keys
/// lexicographically, with nested `hints` and `visibility` in schema order).
/// Checked: a non-finite retained number is an error, never a silent `null`.
pub fn encode_held_window_canonical(
    fields: &[(String, RawSettingValue)],
) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    out.push('{');
    for (i, (key, value)) in fields.iter().enumerate() {
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
/// `visibility`, with nested `hints` and `visibility` in schema order).
/// Reuses [`retained_window_fields`] so spelling cannot drift from the
/// held-row writer.
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
/// Map one valid window fact to its carried fields in canonical schema order
/// (`win`, `out`, `rect`, `fullscreen`, `maximized`, `sticky`, `minimized`,
/// `hints`, `nativeWs`, `hostHidden`, `privileged`, `managed`, `acceptsInput`,
/// `activationInhibited`, `toolWindow`, `owned`, `transient`, `modal`,
/// `resizable`, `above`, `below`, `owner`, `parent`, `role`, `appId`,
/// `executable`, `processId`, `class`, `title`, `type`, `activationPolicy`,
/// `layer`, `fullscreenOwner`, `fullscreenExitable`, `suppressionAvailable`,
/// `visibility`), with nested `hints` and `visibility` in schema order.
pub(crate) fn retained_window_fields(fact: &WindowFact) -> Vec<(String, RawSettingValue)> {
    fn opt_bool(value: Option<bool>) -> RawSettingValue {
        value.map_or(RawSettingValue::Null, RawSettingValue::Bool)
    }
    fn opt_text(value: &Option<String>) -> RawSettingValue {
        value.as_deref().map_or(RawSettingValue::Null, |text| {
            RawSettingValue::String(text.to_owned())
        })
    }
    fn opt_id(value: &Option<WindowId>) -> RawSettingValue {
        value.as_ref().map_or(RawSettingValue::Null, |id| {
            RawSettingValue::String(id.0.clone())
        })
    }
    fn rect_value(rect: Option<Rect>) -> RawSettingValue {
        rect.map_or(RawSettingValue::Null, |row| {
            RawSettingValue::Array(vec![
                RawSettingValue::Number(f64::from(row.x)),
                RawSettingValue::Number(f64::from(row.y)),
                RawSettingValue::Number(f64::from(row.w)),
                RawSettingValue::Number(f64::from(row.h)),
            ])
        })
    }
    fn visibility_value(visibility: VisibilityFact) -> RawSettingValue {
        RawSettingValue::Object(vec![
            ("visible".to_owned(), opt_bool(visibility.visible)),
            (
                "suppressionOwned".to_owned(),
                opt_bool(visibility.suppression_owned),
            ),
        ])
    }
    fn native_value(native: &NativeWorkspaceFact) -> Option<RawSettingValue> {
        match native {
            NativeWorkspaceFact::Omitted => None,
            NativeWorkspaceFact::Null => Some(RawSettingValue::Null),
            NativeWorkspaceFact::Value(id) => Some(RawSettingValue::String(id.clone())),
        }
    }
    let mut fields = vec![
        (
            "win".to_owned(),
            RawSettingValue::String(fact.window.0.clone()),
        ),
        (
            "out".to_owned(),
            fact.output.as_ref().map_or(RawSettingValue::Null, |out| {
                RawSettingValue::String(out.0.clone())
            }),
        ),
        ("rect".to_owned(), rect_value(fact.rect)),
        ("fullscreen".to_owned(), opt_bool(fact.fullscreen)),
        ("maximized".to_owned(), opt_bool(fact.maximized)),
        ("sticky".to_owned(), opt_bool(fact.sticky)),
        ("minimized".to_owned(), opt_bool(fact.minimized)),
        (
            "hints".to_owned(),
            fact.hints.map_or(RawSettingValue::Null, |hints| {
                RawSettingValue::Object(vec![
                    (
                        "minW".to_owned(),
                        hints.min_w.map_or(RawSettingValue::Null, |bound| {
                            RawSettingValue::Number(f64::from(bound))
                        }),
                    ),
                    (
                        "minH".to_owned(),
                        hints.min_h.map_or(RawSettingValue::Null, |bound| {
                            RawSettingValue::Number(f64::from(bound))
                        }),
                    ),
                    (
                        "maxW".to_owned(),
                        hints.max_w.map_or(RawSettingValue::Null, |bound| {
                            RawSettingValue::Number(f64::from(bound))
                        }),
                    ),
                    (
                        "maxH".to_owned(),
                        hints.max_h.map_or(RawSettingValue::Null, |bound| {
                            RawSettingValue::Number(f64::from(bound))
                        }),
                    ),
                ])
            }),
        ),
    ];
    if let Some(native) = native_value(&fact.native_workspace) {
        fields.push(("nativeWs".to_owned(), native));
    }
    fields.extend([
        ("hostHidden".to_owned(), opt_bool(fact.host_hidden)),
        ("privileged".to_owned(), opt_bool(fact.privileged)),
        ("managed".to_owned(), opt_bool(fact.managed)),
        ("acceptsInput".to_owned(), opt_bool(fact.accepts_input)),
        (
            "activationInhibited".to_owned(),
            opt_bool(fact.activation_inhibited),
        ),
        ("toolWindow".to_owned(), opt_bool(fact.tool_window)),
        ("owned".to_owned(), opt_bool(fact.owned)),
        ("transient".to_owned(), opt_bool(fact.transient)),
        ("modal".to_owned(), opt_bool(fact.modal)),
        ("resizable".to_owned(), opt_bool(fact.resizable)),
        ("above".to_owned(), opt_bool(fact.above)),
        ("below".to_owned(), opt_bool(fact.below)),
        ("owner".to_owned(), opt_id(&fact.owner)),
        ("parent".to_owned(), opt_id(&fact.parent)),
        ("role".to_owned(), opt_text(&fact.role)),
        ("appId".to_owned(), opt_text(&fact.app_id)),
        ("executable".to_owned(), opt_text(&fact.executable)),
        ("processId".to_owned(), opt_text(&fact.process_id)),
        ("class".to_owned(), opt_text(&fact.class)),
        ("title".to_owned(), opt_text(&fact.title)),
        (
            "type".to_owned(),
            fact.window_type.map_or(RawSettingValue::Null, |kind| {
                RawSettingValue::String(kind.as_str().to_owned())
            }),
        ),
        (
            "activationPolicy".to_owned(),
            fact.activation_policy
                .map_or(RawSettingValue::Null, |policy| {
                    RawSettingValue::String(policy.as_str().to_owned())
                }),
        ),
        (
            "layer".to_owned(),
            fact.layer.map_or(RawSettingValue::Null, |layer| {
                RawSettingValue::Number(f64::from(layer))
            }),
        ),
        (
            "fullscreenOwner".to_owned(),
            fact.fullscreen_owner
                .map_or(RawSettingValue::Null, |owner| {
                    RawSettingValue::String(owner.as_str().to_owned())
                }),
        ),
        (
            "fullscreenExitable".to_owned(),
            opt_bool(fact.fullscreen_exitable),
        ),
        (
            "suppressionAvailable".to_owned(),
            opt_bool(fact.suppression_available),
        ),
        ("visibility".to_owned(), visibility_value(fact.visibility)),
    ]);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::connection::RawSettingValue;
    use tiler_core::directional::OutputId;
    use tiler_core::world_report::OutputIdentity;
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
        let fields = vec![("scale".to_owned(), RawSettingValue::Number(f64::INFINITY))];
        assert_eq!(encode_held_screen_canonical(&fields), Err(NonFiniteNumber));
    }
}
