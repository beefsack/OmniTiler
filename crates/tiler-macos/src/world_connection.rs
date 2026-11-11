//! macOS world-connection primitive: genuine one-time handshake plus the
//! initial settings exchange through the protocol-owned connection route.
//!
//! Production path only. The typed [`HandshakeRequest`] is built from native
//! truth below, encoded with the protocol codec, routed through
//! [`route_connection_request`](tiler_protocol::connection_protocol::route_connection_request)
//! into the owned [`WorldEngine`](tiler_core::world_engine::WorldEngine),
//! and the reply is parsed with the protocol codec (no local re-codec).
//! The core-minted generation binds the adapter
//! [`WorldSequencer`](crate::world_sequencer::WorldSequencer); reports and
//! later settings gate on that bound generation.
//!
//! Initial settings ride the same route with the raw CLI-over-file
//! acquisition values, so core validation/defaulting owns every key.

use tiler_core::connection::{
    ApplicationIdentity, FactName, HandshakeReply, HandshakeRequest, HostCapabilities,
    NativeSelectorScope, RawSettingValue, SettingsDiagnostic, SettingsDiagnosticKind,
    SettingsReply, SettingsRequest,
};
use tiler_core::world_engine::WorldEngine;
use tiler_protocol::connection_protocol::{
    decode_handshake_reply, decode_settings_reply, encode_handshake_request, encode_settings,
    route_connection_request,
};

/// macOS host capabilities: conservative initial primitive.
///
/// - `host_owned: true`: native Spaces are host-managed; the adapter
///   observes them and never creates them.
/// - `native_selector_scope: PerOutput`: each display carries its own
///   current Space.
/// - `maximize: false`: maximized is unsupported (no native inference).
/// - Every other actuator (`fullscreen`, `native_sticky`, `above`,
///   `warp`, `tiled_float_transfer`, `window_suppression`,
///   `equal_count`, `birth_clear`) stays `false`: unproven until a real
///   actuator or behavior proves it.
/// - `unavailable_facts` names facts no macOS producer emits today, in
///   [`FactName`] catalog order, plus `Executable` (own executable path is
///   not read yet, so the null own value stays support-consistent) and
///   `Class`/`Title` (no native class source is read; titles are never
///   read for privacy, so both stay null).
///   Remaining facts (`Modal`, `Resizable`, `Role`, `AppId`, `ProcessId`,
///   `ActivationPolicy`, `Layer`) are claimed available: the scoped native
///   readers thread raw `AXRole`/`AXModal`/`AXSize`-settability/`CG` layer,
///   owner bundle ID, pid string, and neutral activation policy through
///   every v4 member row (null only when that leg is honestly unknown).
#[must_use]
pub(crate) fn macos_capabilities() -> HostCapabilities {
    HostCapabilities {
        host_owned: true,
        equal_count: false,
        shared_native_roster: false,
        birth_clear: false,
        native_selector_scope: NativeSelectorScope::PerOutput,
        window_suppression: false,
        maximize: false,
        fullscreen: false,
        native_sticky: false,
        above: false,
        warp: false,
        tiled_float_transfer: false,
        unavailable_facts: vec![
            FactName::Privileged,
            FactName::Managed,
            FactName::AcceptsInput,
            FactName::ActivationInhibited,
            FactName::ToolWindow,
            FactName::Owned,
            FactName::Transient,
            FactName::Above,
            FactName::Below,
            FactName::Owner,
            FactName::Parent,
            FactName::Executable,
            FactName::Class,
            FactName::Title,
        ],
    }
}

/// Own process identity from native truth only.
///
/// - `app_id`: the stable probe bundle identity (TCC-stable, always known).
/// - `executable`: `None` (own executable path is not read yet; matches the
///   `Executable` entry in [`macos_capabilities`]).
/// - `process_id`: the own pid as a positive decimal string (always known).
#[must_use]
pub(crate) fn own_identity() -> ApplicationIdentity {
    ApplicationIdentity {
        app_id: Some(crate::BUNDLE_ID.to_owned()),
        executable: None,
        process_id: Some(std::process::id().to_string()),
    }
}

/// Typed handshake request for one adapter incarnation.
#[must_use]
pub(crate) fn build_handshake_request(adapter: &str) -> HandshakeRequest {
    HandshakeRequest {
        adapter: adapter.to_owned(),
        request: "handshake-1".to_owned(),
        application_identity: own_identity(),
        capabilities: macos_capabilities(),
    }
}

/// Settings request at one revision: the supplied raw acquisition values,
// so core validation/defaulting owns every key. Keys the acquisition
// omits diagnose `Missing` core-side with core defaults applied; invalid
// supplied values diagnose per field with last-good retained while valid
// siblings still apply. The adapter never clamps or defaults core-owned
// keys.
#[must_use]
pub(crate) fn build_settings_request(
    adapter: &str,
    generation: &str,
    revision: u64,
    values: Vec<(String, RawSettingValue)>,
) -> SettingsRequest {
    SettingsRequest {
        adapter: adapter.to_owned(),
        generation: generation.to_owned(),
        request: "settings-1".to_owned(),
        revision,
        values,
    }
}

/// Outcome of one genuine handshake plus its settings exchange through
/// the protocol connection route.
pub(crate) struct ConnectOutcome {
    /// Core-minted generation binding the adapter sequencer.
    pub(crate) generation: String,
    /// Accepted core reply for the initial revision (status plus
    /// per-field diagnostics; `Missing` entries mark core defaults).
    pub(crate) settings: SettingsReply,
}

/// Run one revisioned settings exchange against `engine` through the
/// protocol connection route: encode `values` at `revision`, route through
/// `route_connection_request`, and parse the reply with the protocol
/// codec. Returns the accepted core reply, or `None` when the generation
/// mismatches (stale) or any codec leg fails. Never retries on a bound
/// wait: the caller decides whether a later revision supersedes.
pub(crate) fn apply_settings_revision(
    engine: &mut WorldEngine,
    adapter: &str,
    generation: &str,
    revision: u64,
    values: &[(String, RawSettingValue)],
) -> Option<SettingsReply> {
    let request = build_settings_request(adapter, generation, revision, values.to_vec());
    let wire = encode_settings(&request).ok()?;
    let reply_wire = route_connection_request(engine, &wire).ok()?;
    decode_settings_reply(&reply_wire).ok()
}

/// Run one genuine handshake plus the initial (revision 1) settings
/// exchange against `engine` through the protocol connection route, with
/// the supplied raw acquisition values. Returns the core-minted generation
/// plus the accepted settings reply on success, `None` when the handshake
/// refuses (no generation minted, no state mutated) or any codec leg
/// fails.
pub(crate) fn connect(
    engine: &mut WorldEngine,
    adapter: &str,
    values: &[(String, RawSettingValue)],
) -> Option<ConnectOutcome> {
    let handshake_wire = encode_handshake_request(&build_handshake_request(adapter)).ok()?;
    let handshake_reply_wire = route_connection_request(engine, &handshake_wire).ok()?;
    let handshake: HandshakeReply = decode_handshake_reply(&handshake_reply_wire).ok()?;
    if handshake.kind.is_some() {
        return None;
    }
    let generation = handshake.generation?;
    let settings = apply_settings_revision(engine, adapter, &generation, 1, values)?;
    Some(ConnectOutcome {
        generation,
        settings,
    })
}

/// Non-`Missing` diagnostics from one settings reply: the actionable
/// per-field core verdicts (wrong type, out of range, unknown value,
/// unknown field, restart-required, unsupported). `Missing` entries are
/// routine (omitted keys running on core defaults), never surfaced.
#[must_use]
pub(crate) fn hard_setting_errors(reply: &SettingsReply) -> Vec<&SettingsDiagnostic> {
    reply
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind != SettingsDiagnosticKind::Missing)
        .collect()
}

/// One-line `key=kind` rendering of [`hard_setting_errors`] for the
/// startup surface (stderr): empty when core accepted every supplied
/// value onto defaults.
#[must_use]
pub(crate) fn format_setting_errors(reply: &SettingsReply) -> String {
    hard_setting_errors(reply)
        .iter()
        .map(|diagnostic| format!("{}={}", diagnostic.key, diagnostic.kind.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::connection::{SettingsDiagnosticKind, SettingsStatus};

    const ADAPTER: &str = "macos-test-1";

    fn values(pairs: &[(&str, RawSettingValue)]) -> Vec<(String, RawSettingValue)> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn connect_with(pairs: &[(&str, RawSettingValue)]) -> (WorldEngine, ConnectOutcome) {
        let mut engine = WorldEngine::new("phase-1").expect("phase-1 generation token is valid");
        let outcome = connect(&mut engine, ADAPTER, &values(pairs)).expect("handshake binds");
        (engine, outcome)
    }

    fn kinds(reply: &SettingsReply, key: &str) -> Vec<SettingsDiagnosticKind> {
        reply
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.key == key)
            .map(|diagnostic| diagnostic.kind)
            .collect()
    }

    #[test]
    fn empty_acquisition_accepts_on_core_defaults() {
        // No supplied keys: core defaults apply, every wire key diagnoses
        // `Missing`, and nothing actionable surfaces.
        let (_engine, outcome) = connect_with(&[]);
        assert_eq!(outcome.settings.status, SettingsStatus::Accepted);
        assert_eq!(outcome.settings.revision, 1);
        assert_eq!(
            kinds(&outcome.settings, "borderWidth"),
            vec![SettingsDiagnosticKind::Missing]
        );
        assert!(hard_setting_errors(&outcome.settings).is_empty());
        assert_eq!(format_setting_errors(&outcome.settings), "");
    }

    #[test]
    fn invalid_field_diagnoses_per_field_while_siblings_apply() {
        // One out-of-range core number diagnoses exactly its key while the
        // valid siblings ride clean: no refusal, no sibling fallout.
        let (mut engine, outcome) = connect_with(&[
            ("borderEnabled", RawSettingValue::Bool(false)),
            ("borderWidth", RawSettingValue::Number(99.0)),
            ("borderGap", RawSettingValue::Number(2.0)),
        ]);
        assert_eq!(outcome.settings.status, SettingsStatus::Accepted);
        assert_eq!(
            kinds(&outcome.settings, "borderWidth"),
            vec![SettingsDiagnosticKind::OutOfRange]
        );
        assert!(kinds(&outcome.settings, "borderEnabled").is_empty());
        assert!(kinds(&outcome.settings, "borderGap").is_empty());
        assert_eq!(
            format_setting_errors(&outcome.settings),
            "borderWidth=out-of-range"
        );
        // Correcting just the bad key clears its diagnostic; the retained
        // siblings keep applying (only the supplied key re-diagnoses, the
        // omitted siblings fall back to `Missing` on this revision).
        let generation = outcome.generation.clone();
        let fixed = apply_settings_revision(
            &mut engine,
            ADAPTER,
            &generation,
            2,
            &values(&[("borderWidth", RawSettingValue::Number(3.0))]),
        )
        .expect("rev2 routes");
        assert_eq!(fixed.status, SettingsStatus::Accepted);
        assert!(kinds(&fixed, "borderWidth").is_empty());
        assert!(hard_setting_errors(&fixed).is_empty());
    }

    #[test]
    fn wrong_typed_values_diagnose_wrong_type() {
        // Non-numeric CLI text and wrong-typed JSON values ride as strings;
        // core diagnoses `WrongType` per field, siblings unaffected.
        let (_engine, outcome) = connect_with(&[
            ("borderWidth", RawSettingValue::String("abc".to_owned())),
            ("borderGap", RawSettingValue::Number(2.0)),
            ("borderRadius", RawSettingValue::Bool(true)),
        ]);
        assert_eq!(outcome.settings.status, SettingsStatus::Accepted);
        assert_eq!(
            kinds(&outcome.settings, "borderWidth"),
            vec![SettingsDiagnosticKind::WrongType]
        );
        assert_eq!(
            kinds(&outcome.settings, "borderRadius"),
            vec![SettingsDiagnosticKind::WrongType]
        );
        assert!(kinds(&outcome.settings, "borderGap").is_empty());
    }

    #[test]
    fn json_wrong_types_ride_raw_to_core_diagnostics() {
        // The file loader preserves wrong-typed core numbers (no typed
        // refusal); the raw lane forwards them and core diagnoses per field.
        let settings: crate::dev_selection::DevSettings =
            serde_json::from_str(r#"{"active_border":{"width":"wide","gap":true}}"#)
                .expect("parses shape");
        assert!(settings.validate().is_ok());
        let acquired = crate::dev_selection::collect_raw_core_settings(
            &crate::dev_selection::BorderCli::default(),
            settings.active_border.as_ref(),
        );
        let mut engine = WorldEngine::new("phase-1").expect("phase-1 generation token is valid");
        let outcome = connect(&mut engine, ADAPTER, &acquired).expect("handshake binds");
        assert_eq!(outcome.settings.status, SettingsStatus::Accepted);
        assert_eq!(
            kinds(&outcome.settings, "borderWidth"),
            vec![SettingsDiagnosticKind::WrongType]
        );
        assert_eq!(
            kinds(&outcome.settings, "borderGap"),
            vec![SettingsDiagnosticKind::WrongType]
        );
    }

    #[test]
    fn revision_primitive_gates_reuse_stale_and_foreign_generation() {
        let (mut engine, outcome) = connect_with(&[("borderWidth", RawSettingValue::Number(5.0))]);
        let generation = outcome.generation.clone();
        // New values at rev2 apply cleanly.
        let rev2 = apply_settings_revision(
            &mut engine,
            ADAPTER,
            &generation,
            2,
            &values(&[("borderWidth", RawSettingValue::Number(6.0))]),
        )
        .expect("rev2 routes");
        assert_eq!(rev2.status, SettingsStatus::Accepted);
        assert!(kinds(&rev2, "borderWidth").is_empty());
        // Different values at the consumed revision refuse as reused.
        let reused = apply_settings_revision(
            &mut engine,
            ADAPTER,
            &generation,
            2,
            &values(&[("borderWidth", RawSettingValue::Number(7.0))]),
        )
        .expect("reused revision routes");
        assert_eq!(reused.status, SettingsStatus::RevisionReused);
        // An older revision is stale, never applied.
        let stale = apply_settings_revision(
            &mut engine,
            ADAPTER,
            &generation,
            1,
            &values(&[("borderWidth", RawSettingValue::Number(8.0))]),
        )
        .expect("stale revision routes");
        assert_eq!(stale.status, SettingsStatus::Stale);
        // A foreign generation never reaches settings state.
        assert!(
            apply_settings_revision(
                &mut engine,
                ADAPTER,
                "gen-foreign",
                3,
                &values(&[("borderWidth", RawSettingValue::Number(3.0))]),
            )
            .is_none()
        );
    }
}
