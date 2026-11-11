//! Revisioned, validated settings state for a connected adapter lifetime.

use crate::connection::{
    HostCapabilities, NativeSelectorScope, RawSettingValue, SettingsDiagnostic,
    SettingsDiagnosticKind as Kind, SettingsReply, SettingsRequest, SettingsStatus,
};
use crate::settings_catalog::{self as catalog, FieldType, SettingValue, Timing, Validity};
use std::collections::BTreeMap;

const WIRE_FIELDS: [(&str, &str); 20] = [
    ("gap", catalog::FIELD_INNER_GAP),
    ("outer", catalog::FIELD_OUTER_GAP),
    ("defaultTiled", catalog::FIELD_WORKSPACE_DEFAULT_TILED),
    ("mode", catalog::FIELD_WORKSPACE_MODE),
    ("sameAxis", catalog::FIELD_SAME_AXIS_MOVE),
    ("bornMaximized", catalog::FIELD_BORN_MAXIMIZED),
    ("predicate", catalog::FIELD_FIXED_SIZE),
    ("anchorFallback", catalog::FIELD_ANCHOR_FALLBACK),
    ("noSpaceResult", catalog::FIELD_NO_SPACE),
    ("refill", catalog::FIELD_MIGRATION_REFILL),
    ("switchTarget", catalog::FIELD_SWITCH_TARGET),
    ("freshPlacement", catalog::FIELD_FRESH_PLACEMENT),
    ("focusFollowsCursor", catalog::FIELD_FOCUS_FOLLOWS_CURSOR),
    ("cursorWarp", catalog::FIELD_CURSOR_WARP),
    ("borderEnabled", catalog::FIELD_BORDER_ENABLED),
    ("underlayEnabled", catalog::FIELD_UNDERLAY_ENABLED),
    ("borderWidth", catalog::FIELD_BORDER_WIDTH),
    ("borderGap", catalog::FIELD_BORDER_GAP),
    ("borderRadius", catalog::FIELD_BORDER_RADIUS),
    ("underlayExtension", catalog::FIELD_UNDERLAY_EXTENSION),
];

/// Validated computational settings. Text values are only closed enum tokens.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectiveSetting {
    Bool(bool),
    Int(i32),
    Float(f64),
    Enum(&'static str),
}

/// The effective settings and revision state for one adapter lifetime.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsState {
    /// Last-good accepted values, including a later restart-only request.
    pub requested: BTreeMap<&'static str, EffectiveSetting>,
    /// Values currently in use; restart-only values remain at startup values.
    pub effective: BTreeMap<&'static str, EffectiveSetting>,
    pub workspace_mode: crate::workspace_scope::WorkspaceMode,
    pub same_axis_move: crate::directional::SameAxisMove,
    pub born_maximized: crate::born_maximized::BornMaximized,
    pub fixed_size_predicate: crate::size_hints::FixedSizePredicate,
    pub anchor_fallback: crate::admission::AnchorFallback,
    pub no_space_result: crate::admission::NoSpaceResult,
    pub migration_refill: crate::output_selection::MigrationSourceRefill,
    pub switch_target: crate::cursor_targeting::SwitchTarget,
    pub fresh_placement: crate::cursor_targeting::FreshPlacement,
    revision: Option<u64>,
    last_values: Option<Vec<(String, RawSettingValue)>>,
    last_diagnostics: Vec<SettingsDiagnostic>,
    started: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        let mut requested = BTreeMap::new();
        for (wire, field_id) in WIRE_FIELDS {
            let field = catalog::find_field(field_id).expect("wire field in catalog");
            requested.insert(field_id, from_catalog(field.default));
            let _ = wire;
        }
        let workspace_mode = crate::workspace_scope::WorkspaceMode::PerOutputLocal;
        let mut state = Self {
            effective: requested.clone(),
            requested,
            workspace_mode,
            same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
            born_maximized: crate::born_maximized::BornMaximized::TileOnLaunch,
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            anchor_fallback: crate::admission::AnchorFallback::LargestSplittable,
            no_space_result: crate::admission::NoSpaceResult::Float,
            migration_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
            switch_target: crate::cursor_targeting::SwitchTarget::Cursor,
            fresh_placement: crate::cursor_targeting::FreshPlacement::CursorOutputMru,
            revision: None,
            last_values: None,
            last_diagnostics: Vec::new(),
            started: false,
        };
        state.refresh_policy_enums();
        state
    }
}

impl SettingsState {
    pub fn process(&mut self, request: &SettingsRequest, caps: &HostCapabilities) -> SettingsReply {
        let mut reply = SettingsReply {
            adapter: request.adapter.clone(),
            generation: request.generation.clone(),
            request: request.request.clone(),
            revision: request.revision,
            status: SettingsStatus::Accepted,
            diagnostics: Vec::new(),
        };
        if let Some(revision) = self.revision {
            if request.revision < revision {
                reply.status = SettingsStatus::Stale;
                return reply;
            }
            if request.revision == revision {
                if equivalent_values(
                    self.last_values.as_deref().unwrap_or_default(),
                    &request.values,
                ) {
                    reply.diagnostics.clone_from(&self.last_diagnostics);
                    return reply;
                }
                reply.status = SettingsStatus::RevisionReused;
                return reply;
            }
        }

        let supplied: BTreeMap<&str, &RawSettingValue> = request
            .values
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect();
        let mut next = self.requested.clone();
        for (wire, field_id) in WIRE_FIELDS {
            match supplied.get(wire) {
                None => reply.diagnostics.push(diag(wire, Kind::Missing)),
                Some(raw) => {
                    let field = catalog::find_field(field_id).expect("wire field in catalog");
                    let value = match convert(raw, field) {
                        Ok(value) => value,
                        Err(kind) => {
                            reply.diagnostics.push(diag(wire, kind));
                            continue;
                        }
                    };
                    match catalog::validate_field_value(field, &value) {
                        Validity::Invalid(reason) => {
                            let kind = if reason == "unknown token" {
                                Kind::UnknownValue
                            } else if reason == "below minimum" || reason == "above maximum" {
                                Kind::OutOfRange
                            } else {
                                Kind::WrongType
                            };
                            reply.diagnostics.push(diag(wire, kind));
                        }
                        Validity::Valid => {
                            let effective = from_catalog(value);
                            if field.timing == Timing::Restart
                                && self.started
                                && self.effective.get(field_id) != Some(&effective)
                            {
                                reply.diagnostics.push(diag(wire, Kind::RestartRequired));
                            }
                            next.insert(field_id, effective);
                        }
                    }
                }
            }
        }
        for key in supplied
            .keys()
            .copied()
            .filter(|key| !WIRE_FIELDS.iter().any(|(known, _)| known == key))
        {
            reply.diagnostics.push(diag(key, Kind::UnknownField));
        }

        // Defaults and good sibling updates are committed together; invalid fields retain last-good.
        self.requested = next;
        if !self.started {
            self.effective = self.requested.clone();
            self.started = true;
        } else {
            for (_, field_id) in WIRE_FIELDS {
                let field = catalog::find_field(field_id).expect("wire field in catalog");
                if field.timing != Timing::Restart {
                    self.effective
                        .insert(field_id, self.requested[field_id].clone());
                }
            }
        }
        let requested_mode = match self.effective[catalog::FIELD_WORKSPACE_MODE] {
            EffectiveSetting::Enum(token) => token,
            _ => unreachable!(),
        };
        self.workspace_mode = if caps.host_owned
            && caps.native_selector_scope == NativeSelectorScope::AllOutputs
            && requested_mode != "shared"
        {
            if !reply.diagnostics.iter().any(|d| d.key == "mode") {
                reply.diagnostics.push(diag("mode", Kind::UnsupportedMode));
            }
            crate::workspace_scope::WorkspaceMode::Shared
        } else {
            crate::workspace_scope::parse_workspace_mode(requested_mode)
        };
        if matches!(
            self.effective[catalog::FIELD_CURSOR_WARP],
            EffectiveSetting::Bool(true)
        ) && !caps.warp
            && !reply.diagnostics.iter().any(|d| d.key == "cursorWarp")
        {
            reply
                .diagnostics
                .push(diag("cursorWarp", Kind::UnsupportedCapability));
        }
        self.refresh_policy_enums();
        self.revision = Some(request.revision);
        self.last_values = Some(request.values.clone());
        self.last_diagnostics.clone_from(&reply.diagnostics);
        reply
    }

    fn refresh_policy_enums(&mut self) {
        let token = |id| match self.effective[id] {
            EffectiveSetting::Enum(value) => value,
            _ => unreachable!(),
        };
        self.same_axis_move =
            crate::directional::SameAxisMove::parse_wire(token(catalog::FIELD_SAME_AXIS_MOVE))
                .expect("validated policy token");
        self.born_maximized =
            crate::born_maximized::BornMaximized::parse_wire(token(catalog::FIELD_BORN_MAXIMIZED))
                .expect("validated policy token");
        self.fixed_size_predicate =
            crate::size_hints::FixedSizePredicate::parse_wire(token(catalog::FIELD_FIXED_SIZE))
                .expect("validated policy token");
        self.anchor_fallback =
            crate::admission::AnchorFallback::parse_wire(token(catalog::FIELD_ANCHOR_FALLBACK))
                .expect("validated policy token");
        self.no_space_result =
            crate::admission::NoSpaceResult::parse_wire(token(catalog::FIELD_NO_SPACE))
                .expect("validated policy token");
        self.migration_refill = crate::output_selection::MigrationSourceRefill::parse_wire(token(
            catalog::FIELD_MIGRATION_REFILL,
        ))
        .expect("validated policy token");
        self.switch_target =
            crate::cursor_targeting::SwitchTarget::parse_wire(token(catalog::FIELD_SWITCH_TARGET))
                .expect("validated policy token");
        self.fresh_placement = crate::cursor_targeting::FreshPlacement::parse_wire(token(
            catalog::FIELD_FRESH_PLACEMENT,
        ))
        .expect("validated policy token");
    }
}

fn diag(key: &str, kind: Kind) -> SettingsDiagnostic {
    SettingsDiagnostic {
        key: key.to_owned(),
        kind,
    }
}

fn from_catalog(value: SettingValue) -> EffectiveSetting {
    match value {
        SettingValue::Bool(v) => EffectiveSetting::Bool(v),
        SettingValue::Int(v) => EffectiveSetting::Int(v),
        SettingValue::Float(v) => EffectiveSetting::Float(v),
        SettingValue::Text(v) => EffectiveSetting::Enum(v),
    }
}

fn convert(raw: &RawSettingValue, field: &catalog::FieldDef) -> Result<SettingValue, Kind> {
    match (field.field_type, raw) {
        (FieldType::Bool, RawSettingValue::Bool(v)) => Ok(SettingValue::Bool(*v)),
        (FieldType::Int, RawSettingValue::Number(v)) if !v.is_finite() || v.fract() != 0.0 => {
            Err(Kind::WrongType)
        }
        (FieldType::Int, RawSettingValue::Number(v))
            if *v < i32::MIN as f64 || *v > i32::MAX as f64 =>
        {
            Err(Kind::OutOfRange)
        }
        (FieldType::Int, RawSettingValue::Number(v)) => Ok(SettingValue::Int(*v as i32)),
        (FieldType::Float, RawSettingValue::Number(v)) => Ok(SettingValue::Float(*v)),
        (FieldType::Enum, RawSettingValue::String(v)) => field
            .options
            .iter()
            .find(|option| option.token == v)
            .map(|option| SettingValue::Text(option.token))
            .ok_or(Kind::UnknownValue),
        _ => Err(Kind::WrongType),
    }
}

fn equivalent_values(a: &[(String, RawSettingValue)], b: &[(String, RawSettingValue)]) -> bool {
    fn eq(a: &RawSettingValue, b: &RawSettingValue) -> bool {
        match (a, b) {
            (RawSettingValue::Number(a), RawSettingValue::Number(b)) => a == b,
            (RawSettingValue::Array(a), RawSettingValue::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| eq(a, b))
            }
            (RawSettingValue::Object(a), RawSettingValue::Object(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        b.iter()
                            .find(|(key, _)| key == k)
                            .is_some_and(|(_, x)| eq(v, x))
                    })
            }
            _ => a == b,
        }
    }
    a.len() == b.len()
        && a.iter().all(|(k, v)| {
            b.iter()
                .find(|(key, _)| key == k)
                .is_some_and(|(_, x)| eq(v, x))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(scope: NativeSelectorScope, warp: bool) -> HostCapabilities {
        HostCapabilities {
            host_owned: true,
            equal_count: true,
            shared_native_roster: false,
            birth_clear: true,
            native_selector_scope: scope,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            warp,
            tiled_float_transfer: true,
            unavailable_facts: Vec::new(),
        }
    }
    fn req(revision: u64, values: Vec<(&str, RawSettingValue)>) -> SettingsRequest {
        SettingsRequest {
            adapter: "a".into(),
            generation: "g".into(),
            request: format!("r{revision}"),
            revision,
            values: values.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        }
    }
    fn s(v: &str) -> RawSettingValue {
        RawSettingValue::String(v.into())
    }
    fn n(v: f64) -> RawSettingValue {
        RawSettingValue::Number(v)
    }

    #[test]
    fn startup_defaults_then_later_omission_keeps_last_good() {
        let mut state = SettingsState::default();
        let first = state.process(
            &req(1, vec![("gap", n(12.0))]),
            &caps(NativeSelectorScope::PerOutput, true),
        );
        assert_eq!(first.diagnostics.len(), 19);
        assert_eq!(
            state.requested[catalog::FIELD_OUTER_GAP],
            EffectiveSetting::Int(8)
        );
        let second = state.process(&req(2, vec![]), &caps(NativeSelectorScope::PerOutput, true));
        assert_eq!(second.diagnostics.len(), 20);
        assert!(second.diagnostics.iter().all(|d| d.kind == Kind::Missing));
        assert_eq!(
            state.requested[catalog::FIELD_INNER_GAP],
            EffectiveSetting::Int(12)
        );
    }

    #[test]
    fn revisions_are_idempotent_stale_or_reused_without_mutation() {
        let mut state = SettingsState::default();
        let c = caps(NativeSelectorScope::PerOutput, true);
        state.process(&req(2, vec![("gap", n(12.0))]), &c);
        let equivalent = req(
            2,
            vec![
                ("gap", n(12.0)),
                (
                    "extra",
                    RawSettingValue::Object(vec![
                        ("b".into(), n(-0.0)),
                        ("a".into(), RawSettingValue::Bool(true)),
                    ]),
                ),
            ],
        );
        // Different content at equal revision is rejected; same-content key ordering is checked separately below.
        assert_eq!(
            state.process(&equivalent, &c).status,
            SettingsStatus::RevisionReused
        );
        assert_eq!(
            state.process(&req(1, vec![]), &c).status,
            SettingsStatus::Stale
        );
        assert_eq!(
            state.requested[catalog::FIELD_INNER_GAP],
            EffectiveSetting::Int(12)
        );
        state.last_values = Some(vec![(
            "x".into(),
            RawSettingValue::Object(vec![
                ("a".into(), RawSettingValue::Bool(true)),
                ("b".into(), n(0.0)),
            ]),
        )]);
        state.revision = Some(3);
        state.last_diagnostics.clear();
        let same = req(
            3,
            vec![(
                "x",
                RawSettingValue::Object(vec![
                    ("b".into(), n(-0.0)),
                    ("a".into(), RawSettingValue::Bool(true)),
                ]),
            )],
        );
        assert_eq!(state.process(&same, &c).status, SettingsStatus::Accepted);
    }

    #[test]
    fn duplicate_revision_replays_diagnostics_with_new_request_correlation() {
        let mut state = SettingsState::default();
        let c = caps(NativeSelectorScope::PerOutput, true);
        let first = state.process(&req(1, vec![]), &c);
        assert_eq!(first.diagnostics.len(), 20);
        let mut duplicate = req(1, vec![]);
        duplicate.request = "redelivery".into();
        let replay = state.process(&duplicate, &c);
        assert_eq!(replay.request, "redelivery");
        assert_eq!(replay.diagnostics, first.diagnostics);
    }

    #[test]
    fn field_diagnostics_bounds_restart_and_support_precedence() {
        let mut state = SettingsState::default();
        let all_outputs = caps(NativeSelectorScope::AllOutputs, false);
        let first = state.process(
            &req(
                1,
                vec![
                    ("mode", s("per-output-local")),
                    ("cursorWarp", RawSettingValue::Bool(true)),
                    ("gap", n(100.0)),
                    ("outer", n(7.0)),
                    ("sameAxis", s("bad")),
                    ("defaultTiled", n(1.0)),
                ],
            ),
            &all_outputs,
        );
        assert!(first.diagnostics.contains(&diag("gap", Kind::OutOfRange)));
        assert!(
            first
                .diagnostics
                .contains(&diag("sameAxis", Kind::UnknownValue))
        );
        assert!(
            first
                .diagnostics
                .contains(&diag("defaultTiled", Kind::WrongType))
        );
        assert!(
            first
                .diagnostics
                .iter()
                .any(|d| d.key == "mode" && d.kind == Kind::UnsupportedMode)
        );
        assert!(
            first
                .diagnostics
                .iter()
                .any(|d| d.key == "cursorWarp" && d.kind == Kind::UnsupportedCapability)
        );
        assert_eq!(
            state.workspace_mode,
            crate::workspace_scope::WorkspaceMode::Shared
        );
        assert_eq!(
            state.requested[catalog::FIELD_OUTER_GAP],
            EffectiveSetting::Int(7)
        );
        let second = state.process(
            &req(2, vec![("mode", s("global-unique")), ("gap", n(9.0))]),
            &all_outputs,
        );
        assert_eq!(
            second
                .diagnostics
                .iter()
                .filter(|d| d.key == "mode")
                .count(),
            1
        );
        assert_eq!(
            second
                .diagnostics
                .iter()
                .find(|d| d.key == "mode")
                .unwrap()
                .kind,
            Kind::RestartRequired
        );
        assert_eq!(
            state.effective[catalog::FIELD_INNER_GAP],
            EffectiveSetting::Int(9)
        );
        assert_eq!(
            state.workspace_mode,
            crate::workspace_scope::WorkspaceMode::Shared
        );
        let third = state.process(&req(3, vec![("mode", s("global-unique"))]), &all_outputs);
        assert_eq!(
            third
                .diagnostics
                .iter()
                .find(|d| d.key == "mode")
                .unwrap()
                .kind,
            Kind::RestartRequired
        );
        assert_eq!(
            state.workspace_mode,
            crate::workspace_scope::WorkspaceMode::Shared
        );
    }

    #[test]
    fn native_selector_scope_is_ignored_for_non_host_owned_adapters() {
        let mut state = SettingsState::default();
        let mut capabilities = caps(NativeSelectorScope::AllOutputs, true);
        capabilities.host_owned = false;
        let reply = state.process(&req(1, vec![("mode", s("global-unique"))]), &capabilities);
        assert!(
            !reply
                .diagnostics
                .iter()
                .any(|d| d.key == "mode" && d.kind == Kind::UnsupportedMode)
        );
        assert_eq!(
            state.workspace_mode,
            crate::workspace_scope::WorkspaceMode::GlobalUnique
        );
    }
}
