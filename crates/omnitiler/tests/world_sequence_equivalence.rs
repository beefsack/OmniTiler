//! Sequence-equivalence gate for the pre-KDE core interface:
//! verifies that matched initial worlds produce identical final logical state,
//! per-command outcomes and result payload meanings (including a refusal control),
//! and allowed correlation relationships across all six routes:
//! - typed prepared (`tiler_protocol::world_admission::handle_report_checked`):
//!   1. combined single-capture batch,
//!   2. split with modeled application of each eligible intermediate publication (native readback/attempt reports held late),
//!   3. split with intermediate applications omitted (native feedback held late);
//! - real checked broker (`omnitiler::world_service::WorldService`):
//!   4. combined single-capture drain,
//!   5. split with modeled application of each eligible intermediate publication (native readback/attempt reports held late),
//!   6. split with intermediate applications omitted (native feedback held late).
//!
//! Also verifies exact report/source redelivery with no command re-execution.

use omnitiler::world_service::WorldService;
use std::collections::{BTreeMap, BTreeSet};
use tiler_core::boundary::CoreCommand;
use tiler_core::connection::{
    ApplicationIdentity, HandshakeRequest, HostCapabilities, NativeSelectorScope, RawSettingValue,
    SettingsRequest, SettingsStatus,
};
use tiler_core::directional::{OutputId, WindowId, WindowSubject};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ActivationPolicy, CommandDisposition, CommandRejectKind, CommandResult, NativeWorkspaceFact,
    OutputIdentity, PresentationIngressV4, PresentationRowV4, ReportIncorporation, ScopeAuthority,
    ScreenReport, StateProperty, VisibilityFact, WORLD_CONTRACT_VERSION_V4, WindowFact, WindowType,
    WorldCommand, WorldReplyV4, WorldReportV4,
};
use tiler_protocol::world_admission::handle_report_checked;

const ADAPTER: &str = "kwin-seq-equiv";
const SENDER: &str = ":1.77";
const CLIENT: &str = "seq-client";

#[derive(Debug, Clone, PartialEq, Eq)]
enum SemanticPayload {
    None,
    Float {
        win: String,
        has_rect: bool,
        no_space: bool,
    },
    Maximize {
        win: String,
        maximized: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SemanticOutcome {
    Executed(SemanticPayload),
    Refused(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticWindow {
    win: String,
    logical_output: String,
    logical_workspace: String,
    maximized: bool,
    maximized_cmd_seq: Option<u64>,
    floating: bool,
    allocation: Option<[i64; 4]>,
    effective_frame: Option<[i64; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticWorld {
    focus_target: Option<String>,
    border_visible: bool,
    windows: BTreeMap<String, SemanticWindow>,
    outcomes: Vec<SemanticOutcome>,
}

#[derive(Debug, Default)]
struct ModeledNativeHost {
    seen_focus_ids: BTreeSet<String>,
    observed_focus_ids: Vec<String>,
    applied_intermediate_focus: Vec<(String, String)>,
    applied_intermediate_frame_ops: Vec<(String, String)>,
    applied_intermediate_float_placements: Vec<(String, [i64; 4])>,
    applied_intermediate_maximized: Vec<(String, bool)>,
    omitted_intermediate_focus: Vec<(String, String)>,
    omitted_intermediate_frame_ops: Vec<(String, String)>,
    omitted_intermediate_float_placements: Vec<(String, [i64; 4])>,
    omitted_intermediate_maximized: Vec<(String, bool)>,
    native_focused: String,
    native_frames: BTreeMap<String, [i64; 4]>,
    native_maximized: BTreeMap<String, bool>,
}

impl ModeledNativeHost {
    fn new() -> Self {
        let mut native_frames = BTreeMap::new();
        native_frames.insert("win-a".to_owned(), [8, 0, 400, 300]);
        native_frames.insert("win-b".to_owned(), [968, 0, 400, 300]);
        let mut native_maximized = BTreeMap::new();
        native_maximized.insert("win-a".to_owned(), false);
        native_maximized.insert("win-b".to_owned(), false);
        Self {
            seen_focus_ids: BTreeSet::new(),
            observed_focus_ids: Vec::new(),
            applied_intermediate_focus: Vec::new(),
            applied_intermediate_frame_ops: Vec::new(),
            applied_intermediate_float_placements: Vec::new(),
            applied_intermediate_maximized: Vec::new(),
            omitted_intermediate_focus: Vec::new(),
            omitted_intermediate_frame_ops: Vec::new(),
            omitted_intermediate_float_placements: Vec::new(),
            omitted_intermediate_maximized: Vec::new(),
            native_focused: "win-a".to_owned(),
            native_frames,
            native_maximized,
        }
    }

    fn observe_intermediate_typed(
        &mut self,
        reply: &WorldReplyV4,
        dispositions: &[CommandDisposition],
        apply: bool,
    ) {
        if let Some(fr) = &reply.focus_request {
            self.observed_focus_ids.push(fr.id.clone());
            // Focus is spent at most once per retained focus_request.id.
            if self.seen_focus_ids.insert(fr.id.clone()) {
                if apply {
                    self.applied_intermediate_focus
                        .push((fr.target.0.clone(), fr.id.clone()));
                    self.native_focused = fr.target.0.clone();
                } else {
                    self.omitted_intermediate_focus
                        .push((fr.target.0.clone(), fr.id.clone()));
                }
            }
        }
        for d in dispositions {
            if let CommandDisposition::Executed(Some(CommandResult::Float {
                window,
                rect: Some(rect),
                ..
            })) = d
            {
                let frame = [
                    i64::from(rect.x),
                    i64::from(rect.y),
                    i64::from(rect.w),
                    i64::from(rect.h),
                ];
                if apply {
                    self.applied_intermediate_float_placements
                        .push((window.0.clone(), frame));
                    self.native_frames.insert(window.0.clone(), frame);
                } else {
                    self.omitted_intermediate_float_placements
                        .push((window.0.clone(), frame));
                }
            }
        }
        for w in &reply.windows {
            if let Some(op) = &w.frame_operation
                && w.frame_authority == ScopeAuthority::Controlled
            {
                assert!(op.starts_with("op-"), "expected op- correlation: {op}");
                if apply {
                    self.applied_intermediate_frame_ops
                        .push((w.lifetime.0.clone(), op.clone()));
                    if let Some(rect) = w.effective_frame.or(w.allocation) {
                        self.native_frames.insert(
                            w.lifetime.0.clone(),
                            [
                                i64::from(rect.x),
                                i64::from(rect.y),
                                i64::from(rect.w),
                                i64::from(rect.h),
                            ],
                        );
                    }
                } else {
                    self.omitted_intermediate_frame_ops
                        .push((w.lifetime.0.clone(), op.clone()));
                }
            }
            if let Some(max_target) = w
                .state
                .iter()
                .find(|s| s.property == StateProperty::Maximized)
                && max_target.authority == ScopeAuthority::Controlled
                && self.native_maximized.get(&w.lifetime.0).copied() != Some(max_target.value)
            {
                if apply {
                    self.applied_intermediate_maximized
                        .push((w.lifetime.0.clone(), max_target.value));
                    self.native_maximized
                        .insert(w.lifetime.0.clone(), max_target.value);
                } else {
                    self.omitted_intermediate_maximized
                        .push((w.lifetime.0.clone(), max_target.value));
                }
            }
        }
    }

    fn observe_intermediate_broker(
        &mut self,
        reply: &serde_json::Value,
        settled: &serde_json::Value,
        apply: bool,
    ) {
        if let Some(fr) = reply["focusRequest"].as_object()
            && let (Some(id), Some(target)) = (
                fr.get("id").and_then(|v| v.as_str()),
                fr.get("target").and_then(|v| v.as_str()),
            )
        {
            self.observed_focus_ids.push(id.to_owned());
            if self.seen_focus_ids.insert(id.to_owned()) {
                if apply {
                    self.applied_intermediate_focus
                        .push((target.to_owned(), id.to_owned()));
                    self.native_focused = target.to_owned();
                } else {
                    self.omitted_intermediate_focus
                        .push((target.to_owned(), id.to_owned()));
                }
            }
        }
        if settled["outcome"].as_str() == Some("ready")
            && settled["result"]["op"].as_str() == Some("toggle-float")
            && let (Some(win), Some(arr)) = (
                settled["result"]["win"].as_str(),
                settled["result"]["rect"].as_array(),
            )
        {
            let frame = [
                arr[0].as_i64().unwrap(),
                arr[1].as_i64().unwrap(),
                arr[2].as_i64().unwrap(),
                arr[3].as_i64().unwrap(),
            ];
            if apply {
                self.applied_intermediate_float_placements
                    .push((win.to_owned(), frame));
                self.native_frames.insert(win.to_owned(), frame);
            } else {
                self.omitted_intermediate_float_placements
                    .push((win.to_owned(), frame));
            }
        }
        for w in reply["windows"].as_array().expect("windows") {
            let win = w["win"].as_str().expect("win");
            let frame_controlled = w["frame_auth"].as_str() == Some("controlled");
            let op = w["frame_op"].as_str().unwrap_or("");
            if !op.is_empty() && frame_controlled {
                assert!(op.starts_with("op-"), "expected op- correlation: {op}");
                if apply {
                    self.applied_intermediate_frame_ops
                        .push((win.to_owned(), op.to_owned()));
                    if let Some(arr) = w["frame"].as_array().or_else(|| w["alloc"].as_array()) {
                        self.native_frames.insert(
                            win.to_owned(),
                            [
                                arr[0].as_i64().unwrap(),
                                arr[1].as_i64().unwrap(),
                                arr[2].as_i64().unwrap(),
                                arr[3].as_i64().unwrap(),
                            ],
                        );
                    }
                } else {
                    self.omitted_intermediate_frame_ops
                        .push((win.to_owned(), op.to_owned()));
                }
            }
            if let Some(max_target) = w["state"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|s| s["property"].as_str() == Some("maximized"))
                && max_target["auth"].as_str() == Some("controlled")
                && let Some(val) = max_target["value"].as_bool()
                && self.native_maximized.get(win).copied() != Some(val)
            {
                if apply {
                    self.applied_intermediate_maximized
                        .push((win.to_owned(), val));
                    self.native_maximized.insert(win.to_owned(), val);
                } else {
                    self.omitted_intermediate_maximized
                        .push((win.to_owned(), val));
                }
            }
        }
    }
}

fn capabilities() -> HostCapabilities {
    HostCapabilities {
        host_owned: false,
        equal_count: false,
        shared_native_roster: false,
        birth_clear: false,
        native_selector_scope: NativeSelectorScope::PerOutput,
        window_suppression: false,
        maximize: true,
        fullscreen: true,
        native_sticky: true,
        above: false,
        warp: false,
        tiled_float_transfer: false,
        unavailable_facts: Vec::new(),
        detached_fullscreen_view: false,
    }
}

fn handshake_json(adapter: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
          \"applicationIdentity\":{{\"appId\":null,\"executable\":\"fixture-actuator\",\
          \"processId\":\"pid-own-1\"}},\
          \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\
          \"sharedNativeRoster\":false,\"birthClear\":false,\
          \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
          \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
          \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[],\
          \"detachedFullscreenView\":false}}}}"
    )
}

fn settings_json(adapter: &str, generation: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\
          \"gen\":\"{generation}\",\"req\":\"{req}\",\"rev\":\"1\",\
          \"values\":{{\"mode\":\"per-output-local\",\"gap\":8,\"outer\":8,\
          \"bornMaximized\":\"tile-on-launch\"}}}}"
    )
}

fn window_json_at(id: &str, x: i32) -> String {
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\
          \"rect\":[{x},0,400,300],\
          \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\
          \"minimized\":false,\
          \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}},\
          \"hostHidden\":false,\"privileged\":false,\"managed\":true,\
          \"acceptsInput\":true,\"activationInhibited\":false,\
          \"toolWindow\":false,\"owned\":false,\"transient\":false,\
          \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\
          \"owner\":null,\"parent\":null,\"role\":null,\"appId\":null,\
          \"executable\":null,\"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\
          \"title\":null,\"type\":\"normal\",\"activationPolicy\":\"regular\",\
          \"layer\":0,\"fullscreenOwner\":null,\"fullscreenExitable\":null,\
          \"suppressionAvailable\":null,\
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}},\
          \"detachedSurface\":null}}"
    )
}

fn inner_report_json(focused: &str) -> String {
    let windows = [window_json_at("win-a", 8), window_json_at("win-b", 968)];
    format!(
        "{{\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{}],\"closed\":[],\"native\":[],\
          \"focused\":\"{focused}\",\"foregroundKnown\":{},\
          \"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"pointerActivity\":null,\"warpSpent\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
          \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\
          \"frame\":null,\"chordHeld\":false,\"moveWindow\":\"\",\
          \"pagerOrder\":[],\"pagerFrames\":[],\"witness\":null,\"foregroundOut\":null}},\
          \"intentRecords\":[],\"propertyAttempts\":[],\"focusFeedback\":[],\"focusChanges\":null,\
          \"rendererInput\":null}}",
        windows.join(","),
        !focused.is_empty()
    )
}

fn capture_json(generation: &str, seq: u64, inner: &str) -> String {
    format!(
        "{{\"v\":4,\"adapter\":\"{ADAPTER}\",\"gen\":\"{generation}\",\
          \"source\":\"src-{seq}\",\"sourceSeq\":\"{seq}\",\
          \"nativeInvalid\":\"{seq}\",\"report\":{inner}}}"
    )
}

fn typed_window(id: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(id.to_owned()),
        output: Some(OutputId("DP-1".to_owned())),
        rect: Some(Rect {
            x,
            y: 0,
            w: 400,
            h: 300,
        }),
        native_workspace: NativeWorkspaceFact::Omitted,
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        minimized: Some(false),
        hints: Some(WindowSizeHints::none()),
        host_hidden: Some(false),
        privileged: Some(false),
        managed: Some(true),
        accepts_input: Some(true),
        activation_inhibited: Some(false),
        tool_window: Some(false),
        owned: Some(false),
        transient: Some(false),
        modal: Some(false),
        resizable: Some(true),
        above: Some(false),
        below: Some(false),
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: None,
        process_id: Some("pid-foreign-1".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(WindowType::Normal),
        activation_policy: Some(ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        detached_surface: None,
    }
}

fn typed_report(generation: &str, seq: u64, focused: &str) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: ADAPTER.to_owned(),
        core_generation: generation.to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("src-{seq}"),
        command_seq: None,
        command: None,
        screens: vec![ScreenReport {
            output: OutputId("DP-1".to_owned()),
            full_rect: Some(Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            }),
            work_area: Some(Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1040,
            }),
            complete: true,
            scale: 1.0,
            identity: OutputIdentity::unknown(),
        }],
        held_screens: Vec::new(),
        windows: vec![typed_window("win-a", 8), typed_window("win-b", 968)],
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: None,
        action_cached: None,
        action_outputs: vec![OutputId("DP-1".to_owned())],
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: seq * 10,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        visibility_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        focus_changes: None,
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
        pointer_activity: None,
        warp_spent: None,
    }
}

fn setup_typed_engine() -> (WorldEngine, String) {
    let generation = "gen-seq-1".to_owned();
    let mut engine = WorldEngine::new(&generation).expect("valid generation");
    let hs = engine.handshake(&HandshakeRequest {
        adapter: ADAPTER.to_owned(),
        request: "req-0".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: Some("fixture-actuator".to_owned()),
            process_id: Some("pid-own-1".to_owned()),
        },
        capabilities: capabilities(),
    });
    assert!(hs.generation.is_some());
    let settings = engine
        .handle_settings(&SettingsRequest {
            adapter: ADAPTER.to_owned(),
            generation: generation.clone(),
            request: "settings-0".to_owned(),
            revision: 1,
            values: vec![
                (
                    "mode".to_owned(),
                    RawSettingValue::String("per-output-local".to_owned()),
                ),
                ("gap".to_owned(), RawSettingValue::Number(8.0)),
                ("outer".to_owned(), RawSettingValue::Number(8.0)),
                (
                    "bornMaximized".to_owned(),
                    RawSettingValue::String("tile-on-launch".to_owned()),
                ),
            ],
        })
        .expect("settings route");
    assert_eq!(settings.status, SettingsStatus::Accepted);

    let init = typed_report(&generation, 1, "win-a");
    let tr = handle_report_checked(&mut engine, &init, &[]);
    assert!(matches!(tr.outcome, WorldOutcome::Accepted(_)));
    (engine, generation)
}

fn setup_broker() -> (WorldService, String) {
    let service = WorldService::new();
    let hs: serde_json::Value = serde_json::from_str(
        &service
            .discover(&handshake_json(ADAPTER, "req-0"))
            .expect("handshake"),
    )
    .expect("json");
    let generation = hs["gen"].as_str().expect("gen").to_owned();
    let st: serde_json::Value = serde_json::from_str(
        &service
            .discover(&settings_json(ADAPTER, &generation, "settings-0"))
            .expect("settings"),
    )
    .expect("json");
    assert_eq!(st["status"], "accepted");

    let init_cap = capture_json(&generation, 1, &inner_report_json("win-a"));
    let reply: serde_json::Value =
        serde_json::from_str(&service.evaluate_capture(&init_cap).expect("init capture"))
            .expect("json");
    assert_eq!(reply["ready"], true, "{reply}");
    (service, generation)
}

fn reject_kind_str(kind: CommandRejectKind) -> &'static str {
    kind.as_str()
}

fn semantic_from_typed(reply: &WorldReplyV4, outcomes: Vec<SemanticOutcome>) -> SemanticWorld {
    let focus_target = reply.focus_request.as_ref().map(|fr| fr.target.0.clone());
    let mut windows = BTreeMap::new();
    for w in &reply.windows {
        let max_target = w
            .state
            .iter()
            .find(|s| s.property == StateProperty::Maximized);
        let maximized = max_target.is_some_and(|s| s.value);
        let maximized_cmd_seq = max_target.and_then(|s| s.command_seq);
        let allocation = w.allocation.map(|r| {
            [
                i64::from(r.x),
                i64::from(r.y),
                i64::from(r.w),
                i64::from(r.h),
            ]
        });
        let effective_frame = w.effective_frame.map(|r| {
            [
                i64::from(r.x),
                i64::from(r.y),
                i64::from(r.w),
                i64::from(r.h),
            ]
        });
        windows.insert(
            w.lifetime.0.clone(),
            SemanticWindow {
                win: w.lifetime.0.clone(),
                logical_output: w.output.0.clone(),
                logical_workspace: w.workspace.clone(),
                maximized,
                maximized_cmd_seq,
                floating: w.floating,
                allocation,
                effective_frame,
            },
        );
    }
    SemanticWorld {
        focus_target,
        border_visible: reply.presentation.border.visible,
        windows,
        outcomes,
    }
}

fn semantic_from_broker(
    reply: &serde_json::Value,
    outcomes: Vec<SemanticOutcome>,
) -> SemanticWorld {
    let focus_target = reply["focusRequest"]
        .as_object()
        .and_then(|o| o.get("target"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let mut windows = BTreeMap::new();
    for w in reply["windows"].as_array().expect("windows array") {
        let id = w["win"].as_str().expect("win").to_owned();
        let max_target = w["state"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| s["property"].as_str() == Some("maximized"));
        let maximized = max_target
            .and_then(|s| s["value"].as_bool())
            .unwrap_or(false);
        let maximized_cmd_seq = max_target
            .and_then(|s| s["cmdSeq"].as_str())
            .map(|s| s.parse::<u64>().expect("cmdSeq"));
        let allocation = w["alloc"].as_array().map(|arr| {
            [
                arr[0].as_i64().unwrap(),
                arr[1].as_i64().unwrap(),
                arr[2].as_i64().unwrap(),
                arr[3].as_i64().unwrap(),
            ]
        });
        let effective_frame = w["frame"].as_array().map(|arr| {
            [
                arr[0].as_i64().unwrap(),
                arr[1].as_i64().unwrap(),
                arr[2].as_i64().unwrap(),
                arr[3].as_i64().unwrap(),
            ]
        });
        windows.insert(
            id.clone(),
            SemanticWindow {
                win: id,
                logical_output: w["out"].as_str().expect("out").to_owned(),
                logical_workspace: w["ws"].as_str().expect("ws").to_owned(),
                maximized,
                maximized_cmd_seq,
                floating: w["floating"].as_bool().expect("floating"),
                allocation,
                effective_frame,
            },
        );
    }
    let border_visible = reply["presentation"]["border"]["visible"]
        .as_bool()
        .expect("border visible");
    SemanticWorld {
        focus_target,
        border_visible,
        windows,
        outcomes,
    }
}

fn typed_payload_to_semantic(payload: &Option<CommandResult>) -> SemanticPayload {
    match payload {
        None => SemanticPayload::None,
        Some(CommandResult::Float {
            window,
            rect,
            no_space,
        }) => SemanticPayload::Float {
            win: window.0.clone(),
            has_rect: rect.is_some(),
            no_space: *no_space,
        },
        Some(CommandResult::Maximize {
            window, maximized, ..
        }) => SemanticPayload::Maximize {
            win: window.0.clone(),
            maximized: *maximized,
        },
        Some(other) => panic!("unexpected typed command payload: {other:?}"),
    }
}

fn broker_payload_to_semantic(payload: &serde_json::Value) -> SemanticPayload {
    if payload.is_null() {
        return SemanticPayload::None;
    }
    match payload["op"].as_str() {
        Some("toggle-float") => SemanticPayload::Float {
            win: payload["win"].as_str().expect("win").to_owned(),
            has_rect: !payload["rect"].is_null(),
            no_space: payload
                .get("noSpace")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        },
        Some("maximize") => SemanticPayload::Maximize {
            win: payload["win"].as_str().expect("win").to_owned(),
            maximized: payload["maximized"].as_bool().expect("maximized"),
        },
        other => panic!("unexpected broker command payload: {other:?} ({payload})"),
    }
}

fn disposition_to_semantic(d: &CommandDisposition) -> SemanticOutcome {
    match d {
        CommandDisposition::Executed(payload) => {
            SemanticOutcome::Executed(typed_payload_to_semantic(payload))
        }
        CommandDisposition::Refused(kind) => {
            SemanticOutcome::Refused(reject_kind_str(*kind).to_owned())
        }
        other => panic!("unexpected command disposition: {other:?}"),
    }
}

fn take_to_semantic(v: &serde_json::Value) -> SemanticOutcome {
    match v["outcome"].as_str().expect("outcome") {
        "ready" => SemanticOutcome::Executed(broker_payload_to_semantic(&v["result"])),
        "failed" => SemanticOutcome::Refused(v["kind"].as_str().expect("kind").to_owned()),
        other => panic!("unexpected take outcome: {other} ({v})"),
    }
}

fn run_typed_combined(cmds: &[WorldCommand]) -> SemanticWorld {
    let (mut engine, generation) = setup_typed_engine();
    let batch: Vec<(u64, WorldCommand)> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| ((i as u64) + 1, c.clone()))
        .collect();
    let report = typed_report(&generation, 2, "win-a");
    let tr = handle_report_checked(&mut engine, &report, &batch);
    let reply = match tr.outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let outcomes = tr
        .dispositions
        .iter()
        .map(|d| disposition_to_semantic(&d.disposition))
        .collect();

    // Exact report replay must not rerun commands or execute a new batch.
    let replay_tr = handle_report_checked(&mut engine, &report, &batch);
    assert_eq!(replay_tr.incorporation, ReportIncorporation::Replay);
    assert!(
        replay_tr
            .dispositions
            .iter()
            .all(|d| matches!(d.disposition, CommandDisposition::NotRun)),
        "typed replay must not rerun commands: {:?}",
        replay_tr.dispositions
    );

    semantic_from_typed(&reply, outcomes)
}

fn run_typed_split(
    cmds: &[WorldCommand],
    apply_intermediate: bool,
) -> (SemanticWorld, ModeledNativeHost, Option<String>) {
    let (mut engine, generation) = setup_typed_engine();
    let mut host = ModeledNativeHost::new();
    let mut outcomes = Vec::with_capacity(cmds.len());
    let mut final_reply: Option<WorldReplyV4> = None;

    for (i, cmd) in cmds.iter().enumerate() {
        let seq = (i as u64) + 2;
        // Native readback/attempt reports are held late until after the
        // sequence: no proof reaches core before the next command.
        let report = typed_report(&generation, seq, "win-a");
        assert!(report.frames.is_empty());
        assert!(report.focus_feedback.is_empty());
        assert!(report.state_attempts.is_empty());
        assert_eq!(report.focused_global, "win-a");

        let tr = handle_report_checked(&mut engine, &report, &[((i as u64) + 1, cmd.clone())]);
        assert_eq!(tr.incorporation, ReportIncorporation::Incorporated);
        let reply = match tr.outcome {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted at step {i}, got {other:?}"),
        };
        assert_eq!(tr.dispositions.len(), 1);
        outcomes.push(disposition_to_semantic(&tr.dispositions[0].disposition));

        if i + 1 < cmds.len() {
            let disps: Vec<CommandDisposition> = tr
                .dispositions
                .iter()
                .map(|d| d.disposition.clone())
                .collect();
            host.observe_intermediate_typed(&reply, &disps, apply_intermediate);
        } else {
            final_reply = Some(reply);
        }
    }
    let final_reply = final_reply.expect("final reply");
    let final_focus_id = final_reply.focus_request.as_ref().map(|fr| fr.id.clone());
    (
        semantic_from_typed(&final_reply, outcomes),
        host,
        final_focus_id,
    )
}

fn queue_cmd_json(generation: &str, id: &str, cmd_json: &str) -> String {
    format!(
        "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"{CLIENT}\",\
          \"clientCmd\":\"{id}\",\"command\":{cmd_json}}}"
    )
}

fn take_cmd_json(generation: &str, id: &str) -> String {
    format!(
        "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"{CLIENT}\",\
          \"clientCmd\":\"{id}\"}}"
    )
}

fn run_broker_combined(cmds_json: &[String]) -> SemanticWorld {
    let (service, generation) = setup_broker();
    for (i, cmd) in cmds_json.iter().enumerate() {
        let id = (i + 1).to_string();
        let q: serde_json::Value = serde_json::from_str(
            &service
                .queue_command(SENDER, &queue_cmd_json(&generation, &id, cmd))
                .expect("queue"),
        )
        .expect("json");
        assert_eq!(q["outcome"], "queued");
        let pending: serde_json::Value = serde_json::from_str(
            &service
                .take_result(SENDER, &take_cmd_json(&generation, &id))
                .expect("take"),
        )
        .expect("json");
        assert_eq!(pending["outcome"], "pending");
    }

    let cap = capture_json(&generation, 2, &inner_report_json("win-a"));
    let reply_raw = service.evaluate_capture(&cap).expect("combined capture");
    let reply: serde_json::Value = serde_json::from_str(&reply_raw).expect("json");
    assert_eq!(reply["ready"], true, "{reply}");

    let mut outcomes = Vec::with_capacity(cmds_json.len());
    for i in 0..cmds_json.len() {
        let id = (i + 1).to_string();
        let settled: serde_json::Value = serde_json::from_str(
            &service
                .take_result(SENDER, &take_cmd_json(&generation, &id))
                .expect("take"),
        )
        .expect("json");
        outcomes.push(take_to_semantic(&settled));
    }

    // Exact source redelivery must not rerun commands or drain a newly parked one.
    let probe_id = (cmds_json.len() + 1).to_string();
    let q_probe: serde_json::Value = serde_json::from_str(
        &service
            .queue_command(
                SENDER,
                &queue_cmd_json(
                    &generation,
                    &probe_id,
                    "{\"op\":\"focus\",\"win\":\"win-a\"}",
                ),
            )
            .expect("queue probe"),
    )
    .expect("json");
    assert_eq!(q_probe["outcome"], "queued");

    let redelivered_raw = service.evaluate_capture(&cap).expect("exact redelivery");
    assert_eq!(
        redelivered_raw, reply_raw,
        "exact broker redelivery returns identical remembered reply"
    );
    let probe_take: serde_json::Value = serde_json::from_str(
        &service
            .take_result(SENDER, &take_cmd_json(&generation, &probe_id))
            .expect("take probe"),
    )
    .expect("json");
    assert_eq!(
        probe_take["outcome"], "pending",
        "exact redelivery must not drain or execute newly parked commands"
    );

    semantic_from_broker(&reply, outcomes)
}

fn run_broker_split(
    cmds_json: &[String],
    apply_intermediate: bool,
) -> (SemanticWorld, ModeledNativeHost, Option<String>) {
    let (service, generation) = setup_broker();
    let mut host = ModeledNativeHost::new();
    let mut outcomes = Vec::with_capacity(cmds_json.len());
    let mut final_reply: Option<serde_json::Value> = None;

    for (i, cmd) in cmds_json.iter().enumerate() {
        let id = (i + 1).to_string();
        let q: serde_json::Value = serde_json::from_str(
            &service
                .queue_command(SENDER, &queue_cmd_json(&generation, &id, cmd))
                .expect("queue"),
        )
        .expect("json");
        assert_eq!(q["outcome"], "queued");

        let seq = (i as u64) + 2;
        // Native readback/attempt reports are held late until after the
        // sequence: no proof reaches core before the next command.
        let inner = inner_report_json("win-a");
        let inner_val: serde_json::Value = serde_json::from_str(&inner).expect("inner json");
        assert_eq!(inner_val["focusFeedback"].as_array().map(Vec::len), Some(0));
        assert_eq!(
            inner_val["propertyAttempts"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(inner_val["focused"].as_str(), Some("win-a"));

        let cap = capture_json(&generation, seq, &inner);
        let reply: serde_json::Value =
            serde_json::from_str(&service.evaluate_capture(&cap).expect("split capture"))
                .expect("json");
        assert_eq!(reply["ready"], true, "{reply}");

        let settled: serde_json::Value = serde_json::from_str(
            &service
                .take_result(SENDER, &take_cmd_json(&generation, &id))
                .expect("take"),
        )
        .expect("json");
        outcomes.push(take_to_semantic(&settled));

        if i + 1 < cmds_json.len() {
            host.observe_intermediate_broker(&reply, &settled, apply_intermediate);
        } else {
            final_reply = Some(reply);
        }
    }

    let final_reply = final_reply.expect("final reply");
    let final_focus_id = final_reply["focusRequest"]
        .as_object()
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    (
        semantic_from_broker(&final_reply, outcomes),
        host,
        final_focus_id,
    )
}

#[test]
fn sequence_equivalence_two_toggle_float_all_six_routes_agree() {
    check_two_toggle_float_all_six_routes(Some(Rect {
        x: 32,
        y: 32,
        w: 400,
        h: 300,
    }));
}

#[test]
fn sequence_equivalence_two_toggle_float_without_rect_keeps_visible_frame() {
    check_two_toggle_float_all_six_routes(None);
}

fn check_two_toggle_float_all_six_routes(first_rect: Option<Rect>) {
    let typed_cmds = vec![
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: WindowSubject::CurrentFocus,
            float_rect: first_rect,
        }),
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: WindowSubject::CurrentFocus,
            float_rect: None,
        }),
        WorldCommand::Focus {
            window: WindowSubject::Window(WindowId("win-missing".to_owned())),
            direction: None,
        },
    ];
    let broker_cmds = vec![
        serde_json::json!({
            "op": "toggle-float",
            "win": {"currentFocus": true},
            "rect": first_rect.map(|rect| [rect.x, rect.y, rect.w, rect.h]),
        })
        .to_string(),
        "{\"op\":\"toggle-float\",\"win\":{\"currentFocus\":true},\"rect\":null}".to_owned(),
        "{\"op\":\"focus\",\"win\":\"win-missing\"}".to_owned(),
    ];

    let typed_combined = run_typed_combined(&typed_cmds);
    let (typed_split_applied, typed_applied_host, _) = run_typed_split(&typed_cmds, true);
    let (typed_split_omitted, typed_omitted_host, _) = run_typed_split(&typed_cmds, false);
    let broker_combined = run_broker_combined(&broker_cmds);
    let (broker_split_applied, broker_applied_host, _) = run_broker_split(&broker_cmds, true);
    let (broker_split_omitted, broker_omitted_host, _) = run_broker_split(&broker_cmds, false);

    assert_eq!(typed_combined, typed_split_applied);
    assert_eq!(typed_combined, typed_split_omitted);
    assert_eq!(typed_combined, broker_combined);
    assert_eq!(typed_combined, broker_split_applied);
    assert_eq!(typed_combined, broker_split_omitted);

    // Intermediate float placement write was applied in applied legs and omitted in omitted legs.
    assert_eq!(
        typed_applied_host
            .applied_intermediate_float_placements
            .len(),
        1
    );
    assert_eq!(
        typed_applied_host.applied_intermediate_float_placements[0].0,
        "win-a"
    );
    let expected_float_frame = first_rect
        .map(|rect| [rect.x, rect.y, rect.w, rect.h].map(i64::from))
        .unwrap_or([8, 0, 400, 300]);
    assert_eq!(
        typed_applied_host.native_frames["win-a"],
        expected_float_frame
    );
    assert_eq!(
        broker_applied_host
            .applied_intermediate_float_placements
            .len(),
        1
    );
    assert_eq!(
        broker_applied_host.applied_intermediate_float_placements[0].0,
        "win-a"
    );
    assert_eq!(
        broker_applied_host.applied_intermediate_float_placements,
        typed_applied_host.applied_intermediate_float_placements
    );
    assert_eq!(
        broker_applied_host.native_frames["win-a"],
        typed_applied_host.native_frames["win-a"]
    );

    assert!(
        typed_omitted_host
            .applied_intermediate_float_placements
            .is_empty()
    );
    assert_eq!(
        typed_omitted_host.omitted_intermediate_float_placements,
        typed_applied_host.applied_intermediate_float_placements
    );
    assert_eq!(typed_omitted_host.native_frames["win-a"], [8, 0, 400, 300]);
    assert!(
        broker_omitted_host
            .applied_intermediate_float_placements
            .is_empty()
    );
    assert_eq!(
        broker_omitted_host.omitted_intermediate_float_placements,
        broker_applied_host.applied_intermediate_float_placements
    );
    assert_eq!(broker_omitted_host.native_frames["win-a"], [8, 0, 400, 300]);

    assert_eq!(
        typed_combined.outcomes,
        vec![
            SemanticOutcome::Executed(SemanticPayload::Float {
                win: "win-a".to_owned(),
                has_rect: true,
                no_space: false,
            }),
            SemanticOutcome::Executed(SemanticPayload::Float {
                win: "win-a".to_owned(),
                has_rect: false,
                no_space: false,
            }),
            SemanticOutcome::Refused("focus-unknown".to_owned()),
        ]
    );
    assert!(!typed_combined.windows["win-a"].floating);
    assert!(!typed_combined.windows["win-b"].floating);
    assert!(typed_combined.windows["win-a"].allocation.is_some());
    assert!(typed_combined.windows["win-b"].allocation.is_some());
}

#[test]
fn sequence_equivalence_noncommuting_focus_b_then_maximize_all_six_routes_agree() {
    let typed_cmds = vec![
        WorldCommand::Focus {
            window: WindowSubject::Window(WindowId("win-b".to_owned())),
            direction: None,
        },
        WorldCommand::ToggleMaximize {
            window: WindowSubject::CurrentFocus,
        },
        WorldCommand::Focus {
            window: WindowSubject::Window(WindowId("win-missing".to_owned())),
            direction: None,
        },
    ];
    let broker_cmds = vec![
        "{\"op\":\"focus\",\"win\":\"win-b\"}".to_owned(),
        "{\"op\":\"toggle-maximize\",\"win\":{\"currentFocus\":true}}".to_owned(),
        "{\"op\":\"focus\",\"win\":\"win-missing\"}".to_owned(),
    ];

    let typed_combined = run_typed_combined(&typed_cmds);
    let (typed_split_applied, typed_applied_host, typed_applied_final_focus_id) =
        run_typed_split(&typed_cmds, true);
    let (typed_split_omitted, typed_omitted_host, typed_omitted_final_focus_id) =
        run_typed_split(&typed_cmds, false);
    let broker_combined = run_broker_combined(&broker_cmds);
    let (broker_split_applied, broker_applied_host, broker_applied_final_focus_id) =
        run_broker_split(&broker_cmds, true);
    let (broker_split_omitted, broker_omitted_host, broker_omitted_final_focus_id) =
        run_broker_split(&broker_cmds, false);

    assert_eq!(typed_combined, typed_split_applied);
    assert_eq!(typed_combined, typed_split_omitted);
    assert_eq!(typed_combined, broker_combined);
    assert_eq!(typed_combined, broker_split_applied);
    assert_eq!(typed_combined, broker_split_omitted);

    // Retained focus id stays identical across delayed intermediate and final publications,
    // and focus spend is modeled once per retained id (applied vs omitted).
    let typed_applied_fid = typed_applied_final_focus_id.expect("typed applied final focus id");
    assert_eq!(
        typed_applied_host.observed_focus_ids,
        vec![typed_applied_fid.clone(), typed_applied_fid.clone()]
    );
    assert_eq!(
        typed_applied_host.applied_intermediate_focus,
        vec![("win-b".to_owned(), typed_applied_fid)]
    );
    assert_eq!(typed_applied_host.native_focused, "win-b");
    assert_eq!(
        typed_applied_host.applied_intermediate_maximized,
        vec![("win-b".to_owned(), true)]
    );
    assert!(typed_applied_host.native_maximized["win-b"]);

    let broker_applied_fid = broker_applied_final_focus_id.expect("broker applied final focus id");
    assert_eq!(
        broker_applied_host.observed_focus_ids,
        vec![broker_applied_fid.clone(), broker_applied_fid.clone()]
    );
    assert_eq!(
        broker_applied_host.applied_intermediate_focus,
        vec![("win-b".to_owned(), broker_applied_fid)]
    );
    assert_eq!(broker_applied_host.native_focused, "win-b");
    assert_eq!(
        broker_applied_host.applied_intermediate_maximized,
        vec![("win-b".to_owned(), true)]
    );
    assert!(broker_applied_host.native_maximized["win-b"]);

    let typed_omitted_fid = typed_omitted_final_focus_id.expect("typed omitted final focus id");
    assert_eq!(
        typed_omitted_host.observed_focus_ids,
        vec![typed_omitted_fid.clone(), typed_omitted_fid.clone()]
    );
    assert!(typed_omitted_host.applied_intermediate_focus.is_empty());
    assert_eq!(
        typed_omitted_host.omitted_intermediate_focus,
        vec![("win-b".to_owned(), typed_omitted_fid)]
    );
    assert_eq!(typed_omitted_host.native_focused, "win-a");
    assert!(typed_omitted_host.applied_intermediate_maximized.is_empty());
    assert_eq!(
        typed_omitted_host.omitted_intermediate_maximized,
        vec![("win-b".to_owned(), true)]
    );
    assert!(!typed_omitted_host.native_maximized["win-b"]);

    let broker_omitted_fid = broker_omitted_final_focus_id.expect("broker omitted final focus id");
    assert_eq!(
        broker_omitted_host.observed_focus_ids,
        vec![broker_omitted_fid.clone(), broker_omitted_fid.clone()]
    );
    assert!(broker_omitted_host.applied_intermediate_focus.is_empty());
    assert_eq!(
        broker_omitted_host.omitted_intermediate_focus,
        vec![("win-b".to_owned(), broker_omitted_fid)]
    );
    assert_eq!(broker_omitted_host.native_focused, "win-a");
    assert!(
        broker_omitted_host
            .applied_intermediate_maximized
            .is_empty()
    );
    assert_eq!(
        broker_omitted_host.omitted_intermediate_maximized,
        vec![("win-b".to_owned(), true)]
    );
    assert!(!broker_omitted_host.native_maximized["win-b"]);

    assert_eq!(
        typed_combined.outcomes,
        vec![
            SemanticOutcome::Executed(SemanticPayload::None),
            SemanticOutcome::Executed(SemanticPayload::Maximize {
                win: "win-b".to_owned(),
                maximized: true,
            }),
            SemanticOutcome::Refused("focus-unknown".to_owned()),
        ]
    );
    assert_eq!(typed_combined.focus_target.as_deref(), Some("win-b"));
    assert!(typed_combined.windows["win-b"].maximized);
    assert_eq!(typed_combined.windows["win-b"].maximized_cmd_seq, Some(2));
    assert!(!typed_combined.windows["win-a"].maximized);
    assert_eq!(typed_combined.windows["win-a"].maximized_cmd_seq, None);
    assert!(!typed_combined.windows["win-a"].floating);
}
