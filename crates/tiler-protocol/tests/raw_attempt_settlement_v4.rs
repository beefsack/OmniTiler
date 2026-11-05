//! Ordered internal execution of the immutable `raw-attempt-settlement`,
//! `window-state` and both `visibility-*` scenarios (unit 6 sub-increments
//! 7f-2, 7f-3a and 7f-4): one fresh engine per connection lifetime, genuine
//! handshake/settings/report decoders, the internal v4 admission boundary
//! through a test-support bridge, genuine canonical encoders, full engine
//! drop/recreation on disconnect, explicit OPEN-gate retention, and every
//! original jq assertion evaluated literally in step order against
//! `{output,history}` envelopes. Public ingress, fixtures, and the manifest
//! are untouched. Visibility scenarios additionally use genuine reply
//! decoding for adapter-directed messages, genuine report decoding for
//! prepare-request pairing only, and an explicit offline host-evidence
//! driver for native/request lifecycle; no adapter code changes.
//!
//! The `persistent-above` scenario additionally has a passing first-twenty-step
//! prefix test, an ignored full ordered run (parked blocker, not a pass), and
//! labelled DIAGNOSTIC tests that record executed assertion outcomes without
//! claiming any scenario pass.
//! `linked-selectors-warp` runs its full strict ordered scenario: workspace
//! row identity is the (output, id) pair, so shared rings repeat logical ids
//! across outputs.
//! `visibility-cross-output-supersession` runs strict end to end through the
//! ordered runner with an offline host driver; `visibility-order-failure`
//! keeps a passing sixteen-step prefix, an ignored strict full run and a
//! labelled diagnostic (step-17 switchSettled needs shown-member win-a rev4
//! LastWrite evidence absent from all core reports). Native-only evidence
//! stays labelled as modeled, never as core proof.

use std::io::Write as _;
use std::process::{Command, Stdio};

use tiler_core::world_engine::WorldEngine;
use tiler_protocol::connection_protocol::{
    decode_handshake_request, decode_settings, encode_handshake_reply, encode_settings_reply,
};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, decode_reply_v4, decode_report_v4, encode_reply_v4,
};

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4")
}

/// Evaluate one literal fixture jq assertion: `Ok(())` on a strict JSON
/// `true` result, `Err(detail)` on a literal JSON `false`. Anything else
/// (bad exit, syntax or runtime error, non-boolean output) panics: a broken
/// jq evaluation must never be recorded as an expected semantic failure.
fn check_jq(step: &str, expression: &str, envelope: &serde_json::Value) -> Result<(), String> {
    let input = serde_json::to_string(envelope).expect("envelope serializes");
    let mut child = Command::new("jq")
        .arg("-e")
        .arg(expression)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{step}: jq unavailable: {error}"));
    child
        .stdin
        .as_mut()
        .expect("jq stdin")
        .write_all(input.as_bytes())
        .expect("jq stdin writes");
    let output = child.wait_with_output().expect("jq completes");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() && stdout.trim() == "true" {
        return Ok(());
    }
    // Capped failure context: the failing step output only, never the
    // accumulated history.
    let context = serde_json::to_string(envelope.get("output").unwrap_or(&serde_json::Value::Null))
        .expect("output serializes");
    let mut capped: String = context.chars().take(8000).collect();
    if capped.len() < context.len() {
        capped.push_str("...[truncated]");
    }
    let detail = format!("stdout: {stdout}\nstderr: {stderr}\noutput: {capped}");
    // `jq -e` exits 1 with literal `false` output; that alone is a recorded
    // semantic failure. Every other non-true outcome is a jq malfunction.
    if output.status.code() == Some(1) && stdout.trim() == "false" {
        Err(detail)
    } else {
        panic!("{step}: jq evaluation error: {expression}\n{detail}");
    }
}

/// Evaluate one literal fixture jq assertion against `envelope`, requiring a
/// strict JSON `true` result (not merely a successful exit with output).
fn assert_jq(step: &str, expression: &str, envelope: &serde_json::Value) {
    if let Err(detail) = check_jq(step, expression, envelope) {
        panic!("{step}: assertion failed: {expression}\n{detail}");
    }
}

struct OrderedRun {
    steps: usize,
    wires: usize,
    handshakes: usize,
    settings: usize,
    reports: usize,
    events: usize,
    assertions: usize,
    pending: usize,
    pending_topics: Vec<String>,
    generations: Vec<String>,
    generations_consumed: usize,
    adapter_replies: usize,
    prepare_requests: usize,
    native_events: usize,
}

/// Test-only lifetime-bound coupled-property stacking storage driver.
///
/// Models the adapter's mandatory persist-before-write obligation for the
/// independent native stacking properties (`keepAbove`/`keepBelow` are
/// coupled: the host clears one when the other is set). A
/// `recoverable-state` event persists the observed native preimage under
/// the exact `(win, tag)` lifetime binding before applying the requested
/// write; a failed slot (`slotWritable == false`) persists nothing and
/// writes nothing. A `restart-and-restore` event restores only on an exact
/// lifetime match; a reused lifetime with no stored preimage restores
/// nothing. Core eligibility is never consulted here: these are offline
/// storage invariants, independent of stacking policy.
#[derive(Default)]
struct StackingStorageDriver {
    slots: std::collections::BTreeMap<(String, String), StoredStackingPreimage>,
}

#[derive(Clone, Copy)]
struct StoredStackingPreimage {
    keep_above: bool,
    keep_below: bool,
}

impl StackingStorageDriver {
    fn recoverable_state(
        &mut self,
        win: &str,
        tag: &str,
        target: &serde_json::Value,
        native: &serde_json::Value,
        slot_writable: bool,
    ) -> serde_json::Value {
        // Fail-closed guards before any persistence or write: only the
        // acquire-true above write under controlled authority is modeled.
        // `reason`/`cmdSeq` are inert correlation, never consulted.
        let property = target
            .get("property")
            .and_then(serde_json::Value::as_str)
            .expect("storage target property");
        let value = target
            .get("value")
            .and_then(serde_json::Value::as_bool)
            .expect("storage target value");
        let auth = target
            .get("auth")
            .and_then(serde_json::Value::as_str)
            .expect("storage target authority");
        assert_eq!(
            property, "above",
            "unsupported storage target property \"{property}\": only above is modeled"
        );
        assert_eq!(
            auth, "controlled",
            "unsupported storage target authority \"{auth}\": only controlled is modeled"
        );
        assert!(
            value,
            "unsupported storage target release (value false): no release model; refusing"
        );
        let pre_keep_above = native
            .get("keepAbove")
            .and_then(serde_json::Value::as_bool)
            .expect("storage keepAbove");
        let pre_keep_below = native
            .get("keepBelow")
            .and_then(serde_json::Value::as_bool)
            .expect("storage keepBelow");
        if !slot_writable {
            return serde_json::json!({
                "actions": [],
                "native": {"keepAbove": pre_keep_above, "keepBelow": pre_keep_below},
            });
        }
        self.slots.insert(
            (win.to_owned(), tag.to_owned()),
            StoredStackingPreimage {
                keep_above: pre_keep_above,
                keep_below: pre_keep_below,
            },
        );
        serde_json::json!({
            "actions": [{"kind": "persist-preimage"}, {"kind": "state"}],
            "native": {"keepAbove": true, "keepBelow": false},
        })
    }

    fn restart_and_restore(&self, win: &str, tag: &str) -> serde_json::Value {
        match self.slots.get(&(win.to_owned(), tag.to_owned())) {
            Some(preimage) => serde_json::json!({
                "actions": [{"kind": "restore"}],
                "native": {"keepAbove": preimage.keep_above, "keepBelow": preimage.keep_below},
            }),
            None => serde_json::json!({"actions": []}),
        }
    }
}

/// Test-only offline host-evidence driver for `visibility-*` scenarios.
///
/// Honestly models adapter/native mechanics from the declarative initial
/// native state plus ordered fixture events, separating executable internal
/// legs from native-only evidence. It never manufactures outputs to match
/// jq and never mutates report inputs.
///
/// Request pairing: each adapter `prepare-request` retains its decoded
/// `(req, obs)`; the matching reply must arrive under the same req and the
/// current native watermark with `ready` set and no consumption. Pairings
/// spend at-most-once: unprepared, reused, mismatched or stale requests
/// panic. The native watermark advances only through ordered fixture
/// events, never through reply execution.
///
/// Modeled state: native `obs` watermark (monotonic, advanced only by
/// outside-change/release-callback), foreground lifetime, per-window native
/// facts (visible, suppression ownership, output, frame; the model only
/// represents unminimized, unhidden, non-fullscreen, non-game windows with
/// matched lifetimes and available preimages), held focus callbacks,
/// pending refusal gates, retained visibility authorizations
/// `(shown, rev, fallback)`, at-most-once `(win, rev, shown)` spend,
/// seen effect ids, held `(id, target, issuing rev)` focus effects and
/// pending hides.
///
/// Initial native facts are validated at parse: only unminimized,
/// unhidden, non-fullscreen, non-game windows with matched lifetimes,
/// available preimages and nonempty tags are representable; anything else
/// panics. At dispatch, a repeat spend or a refused setter is a no-call
/// `show-blocked` block. Arrival brackets use the output arm only
/// (`kind: output`, `out` and `frame` from native affinity, no `home`)
/// because the fixtures declare `hostOwned: false`; every bracket carries
/// genuine before/after [`VisibilitySample`]-shaped snapshots read from the
/// modeled native state, bounded by the modeled watermark. Desired
/// output/frame equality is checked against the reply row before dispatch;
/// mismatch panics. Synchronous accepted focus updates the modeled
/// foreground first and the equality read decides the proof bit, never
/// setter success alone.
///
/// Externally observed visibility only clears host-side suppression
/// ownership when observed visible (unsuppressed elsewhere).
/// Observed-hidden preserves the existing bit: external hiding grants no
/// project ownership. Adapter show/hide writes update the same bit on
/// accepted setters only; refusals leave native facts unchanged.
///
/// Native-only evidence distinction: `actions`, `focusFeedback`,
/// `lastWrites`, `native` and `retained` here are modeled host execution
/// records, not core replies and not native proof. Core reports still run
/// through genuine decoding plus internal `handle_report_v4`; adapter
/// replies run through genuine `decode_reply_v4`; prepare-request reports
/// run through genuine `decode_report_v4` pairing only. A refused setter
/// with unchanged native facts is refusal evidence; unknown or held facts
/// stay unproven. Late evidence never overrides the retained target but is
/// exposed verbatim alongside the snapshots.
///
/// Fail-closed rules (no skipping): unknown event/input/native keys,
/// unknown action/refusal/callback/effect kinds, non-controlled visibility
/// authority other than untouched, controlled rows without a target,
/// untouched rows with a target, fallback on a shown target, disagreeing
/// facts rewriting the same revision, strictly stale visibility revisions
/// (rejected before any action), duplicate effect ids, unprepared/reused/
/// mismatched/stale request pairings, and stale generation/adapter binding
/// all panic.
struct VisibilityHostDriver {
    adapter: String,
    generation: String,
    obs: u64,
    foreground: String,
    windows: std::collections::BTreeMap<String, VisibilityNativeWindow>,
    callbacks: std::collections::BTreeMap<String, String>,
    refusals: Vec<(String, String)>,
    retained: std::collections::BTreeMap<String, (bool, u64, bool)>,
    considered: std::collections::BTreeSet<(String, u64, bool)>,
    prepared: std::collections::BTreeMap<String, u64>,
    seen_requests: std::collections::BTreeSet<String>,
    seen_effects: std::collections::BTreeSet<String>,
    held: std::collections::BTreeMap<String, HeldFocus>,
    pending_hides: Vec<(String, u64, bool)>,
}

/// One retained held focus: the original effect tuple plus the foreground
/// image and the frozen output-member arrival set sampled after shows at
/// hold time. Release replays all three instead of rebuilding them.
#[derive(Clone)]
struct HeldFocus {
    target: String,
    rev: u64,
    pre_foreground: String,
    arrival: Vec<serde_json::Value>,
}

#[derive(Clone)]
struct VisibilityNativeWindow {
    out: String,
    frame: [i32; 4],
    visible: bool,
    suppression_owned: bool,
    minimized: bool,
    host_hidden: bool,
    fullscreen: bool,
    game: bool,
    lifetime_matched: bool,
    preimage_available: bool,
}

fn parse_counter_string(value: &serde_json::Value, what: &str) -> u64 {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{what} counter string"))
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{what} counter parses"))
}

fn parse_frame(value: &serde_json::Value) -> [i32; 4] {
    let items = value.as_array().expect("frame array");
    assert_eq!(items.len(), 4, "frame length");
    let mut out = [0, 0, 0, 0];
    for (index, item) in items.iter().enumerate() {
        out[index] = item.as_i64().expect("frame int") as i32;
    }
    out
}

impl VisibilityHostDriver {
    fn new(initial_native: &serde_json::Value, adapter: &str, generation: &str) -> Self {
        let obs = parse_counter_string(
            initial_native.get("obs").expect("initial native obs"),
            "initial native obs",
        );
        let foreground = initial_native
            .get("foreground")
            .and_then(serde_json::Value::as_str)
            .expect("initial foreground")
            .to_owned();
        let mut windows = std::collections::BTreeMap::new();
        for item in initial_native
            .get("windows")
            .and_then(serde_json::Value::as_array)
            .expect("initial native windows")
        {
            let win = item
                .get("win")
                .and_then(serde_json::Value::as_str)
                .expect("native win")
                .to_owned();
            // The offline model only represents safe facts: any minimized,
            // host-hidden, fullscreen or game window, any unmatched lifetime
            // or unavailable preimage, and any empty lifetime tag fails
            // closed here instead of implementing unsupported safety.
            let minimized = item
                .get("minimized")
                .and_then(serde_json::Value::as_bool)
                .expect("native minimized");
            assert!(!minimized, "unsupported minimized initial native for {win}");
            let host_hidden = item
                .get("hostHidden")
                .and_then(serde_json::Value::as_bool)
                .expect("native hostHidden");
            assert!(
                !host_hidden,
                "unsupported hostHidden initial native for {win}"
            );
            let fullscreen = item
                .get("fullscreen")
                .and_then(serde_json::Value::as_bool)
                .expect("native fullscreen");
            assert!(
                !fullscreen,
                "unsupported fullscreen initial native for {win}"
            );
            let game = item
                .get("game")
                .and_then(serde_json::Value::as_bool)
                .expect("native game");
            assert!(!game, "unsupported game initial native for {win}");
            let lifetime_matched = item
                .get("lifetimeMatched")
                .and_then(serde_json::Value::as_bool)
                .expect("native lifetimeMatched");
            assert!(
                lifetime_matched,
                "unmatched lifetime initial native for {win}"
            );
            let preimage_available = item
                .get("preimageAvailable")
                .and_then(serde_json::Value::as_bool)
                .expect("native preimageAvailable");
            assert!(
                preimage_available,
                "unavailable preimage initial native for {win}"
            );
            let tag = item
                .get("tag")
                .and_then(serde_json::Value::as_str)
                .expect("native tag");
            assert!(!tag.is_empty(), "empty lifetime tag for {win}");
            windows.insert(
                win,
                VisibilityNativeWindow {
                    out: item
                        .get("out")
                        .and_then(serde_json::Value::as_str)
                        .expect("native out")
                        .to_owned(),
                    frame: parse_frame(item.get("frame").expect("native frame")),
                    visible: item
                        .get("visible")
                        .and_then(serde_json::Value::as_bool)
                        .expect("native visible"),
                    suppression_owned: item
                        .get("suppressionOwned")
                        .and_then(serde_json::Value::as_bool)
                        .expect("native suppressionOwned"),
                    minimized,
                    host_hidden,
                    fullscreen,
                    game,
                    lifetime_matched,
                    preimage_available,
                },
            );
        }
        let mut callbacks = std::collections::BTreeMap::new();
        if let Some(initial_callbacks) = initial_native.get("callbacks") {
            let map = initial_callbacks
                .as_object()
                .expect("initial callbacks object");
            for key in map.keys() {
                assert_eq!(key, "focus", "unsupported callbacks key {key}");
            }
            if let Some(focus) = initial_callbacks.get("focus") {
                let held = focus.as_object().expect("callbacks focus object");
                for (win, state) in held {
                    let text = state.as_str().expect("callback state");
                    assert_eq!(text, "held", "unsupported callback state {text}");
                    assert!(windows.contains_key(win), "unknown callback win {win}");
                    callbacks.insert(win.clone(), text.to_owned());
                }
            }
        }
        let mut refusals = Vec::new();
        if let Some(items) = initial_native.get("refusals").and_then(|v| v.as_array()) {
            for item in items {
                let kind = item
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .expect("refusal kind")
                    .to_owned();
                let win = item
                    .get("win")
                    .and_then(serde_json::Value::as_str)
                    .expect("refusal win")
                    .to_owned();
                assert!(
                    kind == "show" || kind == "hide" || kind == "focus",
                    "unsupported refusal kind {kind}"
                );
                assert!(windows.contains_key(&win), "unknown refusal win {win}");
                refusals.push((kind, win));
            }
        }
        Self {
            adapter: adapter.to_owned(),
            generation: generation.to_owned(),
            obs,
            foreground,
            windows,
            callbacks,
            refusals,
            retained: std::collections::BTreeMap::new(),
            considered: std::collections::BTreeSet::new(),
            prepared: std::collections::BTreeMap::new(),
            seen_requests: std::collections::BTreeSet::new(),
            seen_effects: std::collections::BTreeSet::new(),
            held: std::collections::BTreeMap::new(),
            pending_hides: Vec::new(),
        }
    }

    /// Genuine before/after snapshot of one modeled window in the raw
    /// `VisibilitySample` shape (`obs`, `lifetimeMatched`, `visibility`,
    /// `minimized`, `hostHidden`): read from modeled native facts only,
    /// bounded by the modeled watermark, never from desired rows.
    fn visibility_sample(&self, sid: &str, win: &str) -> serde_json::Value {
        let native = self
            .windows
            .get(win)
            .unwrap_or_else(|| panic!("{sid} unknown sampled win {win}"));
        serde_json::json!({
            "obs": self.obs.to_string(),
            "lifetimeMatched": native.lifetime_matched,
            "visibility": {
                "visible": native.visible,
                "suppressionOwned": native.suppression_owned,
            },
            "minimized": native.minimized,
            "hostHidden": native.host_hidden,
        })
    }

    /// Fail-closed model invariant: the offline driver cannot represent
    /// unsafe native transitions, so any minimized, host-hidden,
    /// fullscreen or game fact, or any unmatched lifetime or unavailable
    /// preimage, panics instead of dispatching on it.
    fn check_native_safety(&self, sid: &str) {
        for (win, native) in &self.windows {
            assert!(
                !native.minimized,
                "{sid} unsupported minimized native {win}"
            );
            assert!(
                !native.host_hidden,
                "{sid} unsupported hostHidden native {win}"
            );
            assert!(
                !native.fullscreen,
                "{sid} unsupported fullscreen native {win}"
            );
            assert!(!native.game, "{sid} unsupported game native {win}");
            assert!(
                native.lifetime_matched,
                "{sid} unmatched lifetime native {win}"
            );
            assert!(
                native.preimage_available,
                "{sid} unavailable preimage native {win}"
            );
        }
    }

    /// One modeled visibility write row in the raw adapter shape, with
    /// genuine pre/post snapshots around the setter decision.
    fn visibility_write(
        win: &str,
        target: bool,
        rev: u64,
        path: &str,
        setter: &str,
        reason: &str,
        snapshots: (&serde_json::Value, &serde_json::Value),
    ) -> serde_json::Value {
        serde_json::json!({
            "win": win,
            "write": {"property": "visibility", "target": target,
                "revision": rev.to_string(),
                "path": path, "setter": setter, "reason": reason,
                "pre": snapshots.0, "post": snapshots.1},
        })
    }

    /// One modeled focus feedback row in the raw adapter shape: the original
    /// effect tuple (`id`, `target`, issuing `rev`), the outcome
    /// (`setter`, `reason`), the foreground transition around the decision,
    /// the equality-read proof bit, output-arm arrival brackets and a modeled
    /// attempt counter. `arrival` stays empty unless the caller supplies
    /// proven brackets; no native proof is claimed.
    fn focus_feedback(
        effect: (&str, &str, u64),
        outcome: (&str, &str),
        transition: (&str, &str),
        foreground_proven: bool,
        arrival: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": effect.0,
            "target": effect.1,
            "rev": effect.2.to_string(),
            "setter": outcome.0,
            "reason": outcome.1,
            "pre": transition.0,
            "post": transition.1,
            "foregroundProven": foreground_proven,
            "arrival": arrival,
            "attemptMs": "0",
        })
    }

    /// Attempt one modeled hide write: refused setters emit the attempt with
    /// unchanged native facts; accepted setters clear visibility and take
    /// host-side suppression ownership. Returns the action plus its write.
    fn hide_attempt(
        &mut self,
        sid: &str,
        win: &str,
        rev: u64,
        fallback: bool,
        pre: &serde_json::Value,
    ) -> (serde_json::Value, serde_json::Value) {
        let key = (win.to_owned(), rev, false);
        self.considered.insert(key);
        let path = if fallback { "fallback" } else { "staged" };
        if self
            .refusals
            .iter()
            .any(|(kind, w)| kind == "hide" && w == win)
        {
            let post = self.visibility_sample(sid, win);
            return (
                serde_json::json!({"kind": "hide", "win": win}),
                Self::visibility_write(
                    win,
                    false,
                    rev,
                    path,
                    "refused",
                    "native-refused",
                    (pre, &post),
                ),
            );
        }
        if let Some(entry) = self.windows.get_mut(win) {
            entry.visible = false;
            entry.suppression_owned = true;
        }
        let post = self.visibility_sample(sid, win);
        (
            serde_json::json!({"kind": "hide", "win": win}),
            Self::visibility_write(win, false, rev, path, "accepted", "", (pre, &post)),
        )
    }

    fn native_snapshot(&self) -> serde_json::Value {
        let mut items = Vec::new();
        for (win, state) in &self.windows {
            items.push(serde_json::json!({
                "win": win,
                "out": state.out,
                "frame": [state.frame[0], state.frame[1], state.frame[2], state.frame[3]],
                "visible": state.visible,
            }));
        }
        serde_json::json!({
            "obs": self.obs.to_string(),
            "foreground": self.foreground,
            "windows": items,
        })
    }

    fn retained_snapshot(&self) -> serde_json::Value {
        let mut items = Vec::new();
        for (win, (shown, rev, fallback)) in &self.retained {
            items.push(serde_json::json!({
                "win": win,
                "visibility": {"shown": shown, "rev": rev.to_string(), "fallback": fallback},
            }));
        }
        serde_json::json!({"windows": items})
    }

    fn prepare_request(&mut self, sid: &str, event: &serde_json::Value) -> serde_json::Value {
        for key in event.as_object().expect("prepare-request object").keys() {
            assert!(
                key == "kind" || key == "message",
                "{sid} unsupported prepare-request key {key}"
            );
        }
        let message = event.get("message").expect("prepare-request message");
        let wire = serde_json::to_string(message).expect("prepare message serializes");
        let decoded = decode_report_v4(&wire).expect("prepare-request decodes");
        assert_eq!(
            decoded.report.adapter_incarnation, self.adapter,
            "{sid} prepare adapter"
        );
        assert_eq!(
            decoded.report.core_generation, self.generation,
            "{sid} prepare generation"
        );
        assert!(
            decoded.report.command_seq.is_none() && decoded.report.command.is_none(),
            "{sid} prepare-request must be commandless paired"
        );
        // Retain the exact declared request pairing; the reply must arrive
        // under this req at this watermark. Duplicate preparations fail.
        let req = decoded.report.request_id.clone();
        let obs = decoded.report.observation_seq;
        assert!(
            !self.prepared.contains_key(&req) && !self.seen_requests.contains(&req),
            "{sid} duplicate prepare-request {req}"
        );
        self.prepared.insert(req, obs);
        serde_json::json!({"actions": []})
    }

    fn apply_outside_change(&mut self, sid: &str, event: &serde_json::Value) -> serde_json::Value {
        for key in event.as_object().expect("outside-change object").keys() {
            assert!(
                key == "kind"
                    || key == "obs"
                    || key == "windows"
                    || key == "foreground"
                    || key == "callbacks"
                    || key == "refusals",
                "{sid} unsupported native event key {key}"
            );
        }
        let obs = parse_counter_string(event.get("obs").expect("outside obs"), "outside obs");
        assert!(obs > self.obs, "{sid} outside obs must advance");
        self.obs = obs;
        if let Some(items) = event.get("windows").and_then(|v| v.as_array()) {
            for item in items {
                let keys: Vec<&str> = item
                    .as_object()
                    .expect("outside window object")
                    .keys()
                    .map(String::as_str)
                    .collect();
                for key in &keys {
                    assert!(
                        *key == "win" || *key == "visible",
                        "{sid} unsupported outside window key {key}"
                    );
                }
                let win = item
                    .get("win")
                    .and_then(serde_json::Value::as_str)
                    .expect("outside win");
                let visible = item
                    .get("visible")
                    .and_then(serde_json::Value::as_bool)
                    .expect("outside visible");
                let entry = self
                    .windows
                    .get_mut(win)
                    .unwrap_or_else(|| panic!("{sid} unknown outside win {win}"));
                // Externally observed visibility only clears host-side
                // suppression ownership when observed visible (unsuppressed
                // elsewhere). Observed-hidden preserves the existing bit:
                // external hiding grants no project ownership.
                entry.visible = visible;
                if visible {
                    entry.suppression_owned = false;
                }
            }
        }
        if let Some(foreground) = event.get("foreground").and_then(|v| v.as_str()) {
            self.foreground = foreground.to_owned();
        }
        if let Some(callbacks) = event.get("callbacks") {
            let map = callbacks.as_object().expect("callbacks object");
            for key in map.keys() {
                assert_eq!(key, "focus", "{sid} unsupported callbacks key {key}");
            }
            self.callbacks.clear();
            if let Some(focus) = callbacks.get("focus") {
                let map = focus.as_object().expect("callbacks focus object");
                for (win, state) in map {
                    let text = state.as_str().expect("callback state");
                    assert_eq!(text, "held", "unsupported callback state {text}");
                    assert!(
                        self.windows.contains_key(win),
                        "{sid} unknown callback win {win}"
                    );
                    self.callbacks.insert(win.clone(), text.to_owned());
                }
            } else {
                // An empty callbacks object retires every held focus
                // without installing a new one; any other shape fails.
                assert_eq!(
                    callbacks,
                    &serde_json::json!({}),
                    "{sid} unsupported callbacks shape"
                );
            }
        }
        if let Some(items) = event.get("refusals").and_then(|v| v.as_array()) {
            self.refusals.clear();
            for item in items {
                let kind = item
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .expect("refusal kind")
                    .to_owned();
                let win = item
                    .get("win")
                    .and_then(serde_json::Value::as_str)
                    .expect("refusal win")
                    .to_owned();
                assert!(
                    kind == "show" || kind == "hide" || kind == "focus",
                    "{sid} unsupported refusal kind {kind}"
                );
                assert!(
                    self.windows.contains_key(&win),
                    "{sid} unknown refusal win {win}"
                );
                self.refusals.push((kind, win));
            }
        }
        serde_json::json!({"actions": [], "native": self.native_snapshot()})
    }

    fn apply_release_callback(
        &mut self,
        sid: &str,
        event: &serde_json::Value,
    ) -> serde_json::Value {
        for key in event.as_object().expect("release-callback object").keys() {
            assert!(
                key == "kind"
                    || key == "action"
                    || key == "win"
                    || key == "setter"
                    || key == "foreground"
                    || key == "obs",
                "{sid} unsupported native event key {key}"
            );
        }
        let action = event
            .get("action")
            .and_then(serde_json::Value::as_str)
            .expect("release action");
        assert_eq!(action, "focus", "{sid} unsupported release action {action}");
        let win = event
            .get("win")
            .and_then(serde_json::Value::as_str)
            .expect("release win")
            .to_owned();
        let setter = event
            .get("setter")
            .and_then(serde_json::Value::as_str)
            .expect("release setter");
        assert_eq!(
            setter, "accepted",
            "{sid} unsupported release setter {setter}"
        );
        let foreground = event
            .get("foreground")
            .and_then(serde_json::Value::as_str)
            .expect("release foreground")
            .to_owned();
        let obs = parse_counter_string(event.get("obs").expect("release obs"), "release obs");
        assert!(obs > self.obs, "{sid} release obs must advance");
        assert!(
            self.callbacks.remove(&win).is_some(),
            "{sid} release without held callback for {win}"
        );
        // The event names only the window, so the held tuple is matched by
        // exact target: zero matches is a release without a hold, more than
        // one is ambiguous. No id-spelling or fallback guessing.
        let matches: Vec<String> = self
            .held
            .iter()
            .filter(|(_, held)| held.target == win)
            .map(|(id, _)| id.clone())
            .collect();
        assert!(
            !matches.is_empty(),
            "{sid} release without held effect for {win}"
        );
        assert!(matches.len() == 1, "{sid} ambiguous held target for {win}");
        let held = self.held.remove(&matches[0]).expect("held tuple present");
        // Held focus completes synchronously: the accepted release must land
        // on the held target, and the equality read decides the proof bit.
        // A mismatched foreground is unsupported and panics before pending
        // hides can drain. The feedback replays the original tuple,
        // the original pre foreground and the frozen hold-time arrival set.
        self.foreground = foreground.clone();
        self.obs = obs;
        assert_eq!(
            self.foreground, held.target,
            "{sid} release foreground mismatch for {win}"
        );
        let foreground_proven = self.foreground == win;
        let feedback = vec![Self::focus_feedback(
            (matches[0].as_str(), held.target.as_str(), held.rev),
            ("accepted", ""),
            (held.pre_foreground.as_str(), foreground.as_str()),
            foreground_proven,
            held.arrival.clone(),
        )];
        let mut actions = Vec::new();
        let mut last_writes = Vec::new();
        if self.held.is_empty() {
            let drained = std::mem::take(&mut self.pending_hides);
            let mut latest: std::collections::BTreeMap<String, (u64, bool)> =
                std::collections::BTreeMap::new();
            for (pending_win, rev, fallback) in drained {
                match latest.get(&pending_win) {
                    Some((old_rev, _)) if *old_rev >= rev => {}
                    _ => {
                        latest.insert(pending_win, (rev, fallback));
                    }
                }
            }
            for (pending_win, (rev, fallback)) in latest {
                let retained = self.retained.get(&pending_win).cloned();
                match retained {
                    Some((false, retained_rev, _)) if retained_rev == rev => {}
                    _ => continue,
                }
                let pre = self.visibility_sample(sid, &pending_win);
                let native_visible = self
                    .windows
                    .get(&pending_win)
                    .unwrap_or_else(|| panic!("{sid} unknown pending win"))
                    .visible;
                if !native_visible {
                    continue;
                }
                let key = (pending_win.clone(), rev, false);
                if self.considered.contains(&key) {
                    continue;
                }
                let (action, write) = self.hide_attempt(sid, &pending_win, rev, fallback, &pre);
                actions.push(action);
                last_writes.push(write);
            }
        }
        let mut output = serde_json::json!({
            "actions": actions,
            "focusFeedback": feedback,
            "native": self.native_snapshot(),
        });
        if !last_writes.is_empty() {
            output["lastWrites"] = serde_json::Value::Array(last_writes);
        }
        output
    }

    /// One output-arm arrival bracket for a shown member from modeled native
    /// facts only: `out` and `frame` come from native affinity, never from
    /// desired rows. The desired row, when present, must equal the observed
    /// native output and frame before dispatch; mismatch panics.
    fn output_bracket(
        &self,
        sid: &str,
        win: &str,
        before: &serde_json::Value,
    ) -> serde_json::Value {
        let native = self
            .windows
            .get(win)
            .unwrap_or_else(|| panic!("{sid} unknown bracket win {win}"));
        assert!(native.visible, "{sid} bracket without visible {win}");
        assert!(
            !native.minimized && !native.host_hidden,
            "{sid} bracket without readable native {win}"
        );
        assert!(
            native.frame[2] > 0 && native.frame[3] > 0,
            "{sid} bracket without readable frame {win}"
        );
        serde_json::json!({
            "win": win,
            "obs": self.obs.to_string(),
            "before": before,
            "kind": "output",
            "out": native.out,
            "frame": [native.frame[0], native.frame[1], native.frame[2], native.frame[3]],
            "after": self.visibility_sample(sid, win),
        })
    }

    fn apply_late_write_evidence(
        &mut self,
        sid: &str,
        event: &serde_json::Value,
    ) -> serde_json::Value {
        for key in event
            .as_object()
            .expect("late-write-evidence object")
            .keys()
        {
            assert!(
                key == "kind" || key == "win" || key == "write",
                "{sid} unsupported native event key {key}"
            );
        }
        let win = event
            .get("win")
            .and_then(serde_json::Value::as_str)
            .expect("late win");
        assert!(
            self.windows.contains_key(win),
            "{sid} unknown late win {win}"
        );
        let write = event.get("write").expect("late write");
        let property = write
            .get("property")
            .and_then(serde_json::Value::as_str)
            .expect("late property");
        assert_eq!(
            property, "visibility",
            "{sid} unsupported late property {property}"
        );
        let target = write
            .get("target")
            .and_then(serde_json::Value::as_bool)
            .expect("late target");
        let revision = write
            .get("revision")
            .and_then(serde_json::Value::as_str)
            .expect("late revision");
        let revision_num: u64 = revision
            .parse()
            .unwrap_or_else(|_| panic!("{sid} late revision parses"));
        let path = write
            .get("path")
            .and_then(serde_json::Value::as_str)
            .expect("late path");
        assert_eq!(path, "independent", "{sid} unsupported late path {path}");
        let setter = write
            .get("setter")
            .and_then(serde_json::Value::as_str)
            .expect("late setter");
        assert!(
            setter == "refused" || setter == "threw",
            "{sid} unsupported late setter {setter}"
        );
        // Late evidence never overrides the retained target: record the
        // spent key so the old authorization stays spent without
        // downgrading the retained revision. The raw write rides along
        // verbatim next to the snapshots; it proves nothing by itself.
        self.considered
            .insert((win.to_owned(), revision_num, target));
        serde_json::json!({
            "actions": [],
            "retained": self.retained_snapshot(),
            "native": self.native_snapshot(),
            "write": write.clone(),
        })
    }

    fn execute_reply(
        &mut self,
        sid: &str,
        reply: &tiler_core::world_report::WorldReplyV4,
    ) -> serde_json::Value {
        use tiler_core::world_report::{EffectCause, EffectKind, ScopeAuthority};
        self.check_native_safety(sid);
        assert_eq!(
            reply.adapter_incarnation, self.adapter,
            "{sid} reply adapter"
        );
        assert_eq!(
            reply.core_generation, self.generation,
            "{sid} reply generation"
        );
        // Request pairing spends at-most-once: the reply must arrive under a
        // prepared req at the prepared watermark with ready set and nothing
        // consumed. Unprepared, reused, mismatched or stale pairings panic.
        // The watermark advances only through fixture events, never here.
        let req = reply.request_id.clone();
        let prepared_obs = if let Some(obs) = self.prepared.remove(&req) {
            self.seen_requests.insert(req.clone());
            obs
        } else if self.seen_requests.contains(&req) {
            panic!("{sid} reused reply req {req}");
        } else {
            panic!("{sid} unprepared reply req {req}");
        };
        assert_eq!(
            reply.based_on_observation_seq, prepared_obs,
            "{sid} mismatched reply obs for {req}"
        );
        assert_eq!(
            reply.based_on_observation_seq, self.obs,
            "{sid} stale reply obs for {req}"
        );
        assert!(reply.ready, "{sid} reply not ready for {req}");
        assert!(!reply.command_consumed, "{sid} reply consumed for {req}");
        for window in &reply.windows {
            match window.visibility_authority {
                ScopeAuthority::Controlled => {
                    let target = window.visibility.as_ref().unwrap_or_else(|| {
                        panic!("{sid} controlled window without visibility target")
                    });
                    if target.fallback {
                        assert!(
                            !target.shown,
                            "{sid} fallback on shown target is unsupported"
                        );
                    }
                }
                ScopeAuthority::Untouched => {
                    assert!(
                        window.visibility.is_none(),
                        "{sid} untouched window with visibility target"
                    );
                }
                ScopeAuthority::Hold(_) => {
                    panic!("{sid} unsupported hold authority in reply")
                }
            }
        }
        for effect in &reply.effects {
            assert_eq!(
                effect.kind,
                EffectKind::Focus,
                "{sid} unsupported effect kind"
            );
            assert_eq!(
                effect.cause,
                EffectCause::ExplicitCommand,
                "{sid} unsupported effect cause"
            );
            assert!(
                effect.command_seq.is_some(),
                "{sid} explicit focus without command sequence"
            );
        }
        // Desired output/frame equality before dispatch: every controlled
        // shown member must name the observed native output, and a present
        // desired frame must equal the native frame. Mismatch panics.
        for window in &reply.windows {
            if window.visibility_authority != ScopeAuthority::Controlled {
                continue;
            }
            let target = window.visibility.as_ref().expect("checked target");
            if !target.shown {
                continue;
            }
            let win = window.lifetime.0.clone();
            let native = self
                .windows
                .get(&win)
                .unwrap_or_else(|| panic!("{sid} unknown reply win {win}"));
            assert_eq!(
                native.out, window.output.0,
                "{sid} mismatched desired output for {win}"
            );
            if let Some(frame) = &window.effective_frame {
                assert_eq!(
                    native.frame,
                    [frame.x, frame.y, frame.w, frame.h],
                    "{sid} mismatched desired frame for {win}"
                );
            }
        }
        // Strictly stale revisions fail before ANY action: identical
        // same-revision rows stay allowed (spent, skipped below) while any
        // older revision panics without later execution.
        let mut skip: std::collections::BTreeSet<(String, u64, bool)> =
            std::collections::BTreeSet::new();
        for window in &reply.windows {
            if window.visibility_authority != ScopeAuthority::Controlled {
                continue;
            }
            let target = window.visibility.as_ref().expect("checked target");
            let win = window.lifetime.0.clone();
            assert!(
                self.windows.contains_key(&win),
                "{sid} unknown reply win {win}"
            );
            match self.retained.get(&win).cloned() {
                Some((old_shown, old_rev, old_fallback)) => {
                    assert!(
                        target.revision >= old_rev,
                        "{sid} stale visibility rev for {win}"
                    );
                    if target.revision == old_rev {
                        assert_eq!(
                            old_shown, target.shown,
                            "{sid} disagreeing fact rewrites rev for {win}"
                        );
                        assert_eq!(
                            old_fallback, target.fallback,
                            "{sid} disagreeing fallback rewrites rev for {win}"
                        );
                        self.considered
                            .insert((win.clone(), target.revision, target.shown));
                        skip.insert((win, target.revision, target.shown));
                        continue;
                    }
                    self.retained
                        .insert(win, (target.shown, target.revision, target.fallback));
                }
                None => {
                    self.retained
                        .insert(win, (target.shown, target.revision, target.fallback));
                }
            }
        }
        // Spend every effect id before dispatch: completed focus counts too,
        // so a repeated grant under any later reply panics as a duplicate.
        for effect in &reply.effects {
            assert!(
                !self.seen_effects.contains(&effect.id),
                "{sid} duplicate effect {}",
                effect.id
            );
        }
        for effect in &reply.effects {
            self.seen_effects.insert(effect.id.clone());
        }
        let mut actions: Vec<serde_json::Value> = Vec::new();
        let mut feedback: Vec<serde_json::Value> = Vec::new();
        let mut last_writes: Vec<serde_json::Value> = Vec::new();
        let mut show_failed = false;
        for window in &reply.windows {
            if window.visibility_authority != ScopeAuthority::Controlled {
                continue;
            }
            let target = window.visibility.as_ref().expect("checked target");
            if !target.shown {
                continue;
            }
            let win = window.lifetime.0.clone();
            if skip.contains(&(win.clone(), target.revision, true)) {
                continue;
            }
            if self.windows.get(&win).expect("known win").visible {
                self.considered.insert((win, target.revision, true));
                continue;
            }
            let key = (win.clone(), target.revision, true);
            if self.considered.contains(&key) {
                show_failed = true;
                continue;
            }
            let native = self.windows.get(&win).expect("known win");
            if !native.lifetime_matched || !native.preimage_available {
                self.considered.insert(key);
                show_failed = true;
                continue;
            }
            // Before image is read just before the write, after any earlier
            // row in this reply already dispatched.
            let before = self.visibility_sample(sid, &win);
            if self
                .refusals
                .iter()
                .any(|(kind, w)| kind == "show" && w == &win)
            {
                self.considered.insert(key);
                actions.push(serde_json::json!({"kind": "show", "win": win}));
                let post = self.visibility_sample(sid, &win);
                last_writes.push(Self::visibility_write(
                    &win,
                    true,
                    target.revision,
                    "staged",
                    "refused",
                    "native-refused",
                    (&before, &post),
                ));
                show_failed = true;
                continue;
            }
            self.considered.insert(key);
            actions.push(serde_json::json!({"kind": "show", "win": win}));
            if let Some(entry) = self.windows.get_mut(&win) {
                entry.visible = true;
                entry.suppression_owned = false;
            }
            let post = self.visibility_sample(sid, &win);
            last_writes.push(Self::visibility_write(
                &win,
                true,
                target.revision,
                "staged",
                "accepted",
                "",
                (&before, &post),
            ));
        }
        // Post-show snapshots: every arrival before image is sampled here,
        // after shows dispatched and before any focus attempt, so before
        // and after are both post-show visible at the modeled watermark.
        let post_show: std::collections::BTreeMap<String, serde_json::Value> = self
            .windows
            .keys()
            .cloned()
            .map(|win| {
                let sample = self.visibility_sample(sid, &win);
                (win, sample)
            })
            .collect();
        let mut focus_proven = false;
        if show_failed {
            for effect in &reply.effects {
                let foreground = self.foreground.clone();
                feedback.push(Self::focus_feedback(
                    (
                        effect.id.as_str(),
                        effect.target.0.as_str(),
                        effect.issued_revision,
                    ),
                    ("not-called", "show-blocked"),
                    (foreground.as_str(), foreground.as_str()),
                    false,
                    Vec::new(),
                ));
            }
        } else {
            for effect in &reply.effects {
                let target_win = effect.target.0.clone();
                assert!(
                    self.windows.contains_key(&target_win),
                    "{sid} unknown focus target {target_win}"
                );
                // One output-arm bracket per controlled shown member on the
                // effect output, frozen here before this focus attempt from
                // modeled native facts only. Held callbacks retain the set;
                // the accepted path dispatches on it below.
                let effect_out = self
                    .windows
                    .get(&target_win)
                    .expect("known focus")
                    .out
                    .clone();
                let mut arrival = Vec::new();
                for window in &reply.windows {
                    if window.visibility_authority != ScopeAuthority::Controlled {
                        continue;
                    }
                    let member = window.visibility.as_ref().expect("checked target");
                    if !member.shown {
                        continue;
                    }
                    let member_win = window.lifetime.0.clone();
                    let member_native = self
                        .windows
                        .get(&member_win)
                        .unwrap_or_else(|| panic!("{sid} unknown member {member_win}"));
                    if member_native.out != effect_out {
                        continue;
                    }
                    let before = post_show
                        .get(&member_win)
                        .expect("post-show snapshot")
                        .clone();
                    arrival.push(self.output_bracket(sid, &member_win, &before));
                }
                let pre_foreground = self.foreground.clone();
                if self.callbacks.contains_key(&target_win) {
                    // Held focus actuates now and completes on release: the
                    // full tuple plus the pre foreground and the frozen
                    // arrival set are retained for exact release replay.
                    actions.push(serde_json::json!({"kind": "focus", "win": target_win}));
                    self.held.insert(
                        effect.id.clone(),
                        HeldFocus {
                            target: target_win,
                            rev: effect.issued_revision,
                            pre_foreground,
                            arrival,
                        },
                    );
                    continue;
                }
                if self
                    .refusals
                    .iter()
                    .any(|(kind, w)| kind == "focus" && w == &target_win)
                {
                    actions.push(serde_json::json!({"kind": "focus", "win": target_win}));
                    let foreground = self.foreground.clone();
                    feedback.push(Self::focus_feedback(
                        (
                            effect.id.as_str(),
                            target_win.as_str(),
                            effect.issued_revision,
                        ),
                        ("refused", "native-refused"),
                        (foreground.as_str(), foreground.as_str()),
                        false,
                        Vec::new(),
                    ));
                    continue;
                }
                let native = self.windows.get(&target_win).expect("known focus");
                assert!(
                    native.lifetime_matched,
                    "{sid} focus without matched lifetime is unsupported"
                );
                // Synchronous accepted focus updates the modeled foreground
                // first; the equality read below decides the proof bit. The
                // frozen pre-attempt arrival set dispatches with it.
                actions.push(serde_json::json!({"kind": "focus", "win": target_win}));
                self.foreground = target_win.clone();
                focus_proven = self.foreground == target_win;
                feedback.push(Self::focus_feedback(
                    (
                        effect.id.as_str(),
                        target_win.as_str(),
                        effect.issued_revision,
                    ),
                    ("accepted", ""),
                    (pre_foreground.as_str(), target_win.as_str()),
                    focus_proven,
                    arrival,
                ));
            }
        }
        if !show_failed && !self.held.is_empty() {
            for window in &reply.windows {
                if window.visibility_authority != ScopeAuthority::Controlled {
                    continue;
                }
                let target = window.visibility.as_ref().expect("checked target");
                if target.shown {
                    continue;
                }
                let win = window.lifetime.0.clone();
                if skip.contains(&(win.clone(), target.revision, false)) {
                    continue;
                }
                let native = self.windows.get(&win).expect("known win");
                if !native.visible {
                    continue;
                }
                let key = (win.clone(), target.revision, false);
                if self.considered.contains(&key) {
                    continue;
                }
                self.pending_hides
                    .push((win, target.revision, target.fallback));
            }
        } else if !show_failed && self.held.is_empty() {
            let has_focus = !reply.effects.is_empty();
            for window in &reply.windows {
                if window.visibility_authority != ScopeAuthority::Controlled {
                    continue;
                }
                let target = window.visibility.as_ref().expect("checked target");
                if target.shown {
                    continue;
                }
                let win = window.lifetime.0.clone();
                if skip.contains(&(win.clone(), target.revision, false)) {
                    continue;
                }
                let native = self.windows.get(&win).expect("known win");
                if !native.visible {
                    self.considered.insert((win, target.revision, false));
                    continue;
                }
                let key = (win.clone(), target.revision, false);
                if self.considered.contains(&key) {
                    continue;
                }
                let run_hide = if has_focus {
                    if focus_proven { true } else { target.fallback }
                } else {
                    true
                };
                if !run_hide {
                    continue;
                }
                let before = post_show.get(&win).expect("post-show snapshot").clone();
                let (action, write) =
                    self.hide_attempt(sid, &win, target.revision, target.fallback, &before);
                actions.push(action);
                last_writes.push(write);
            }
        }
        let mut output = serde_json::json!({
            "actions": actions,
            "focusFeedback": feedback,
            "lastWrites": last_writes,
            "native": self.native_snapshot(),
            "retained": self.retained_snapshot(),
        });
        // Keep empty arrays explicit so jq `any`/`all` see arrays.
        if output["actions"].as_array().is_none() {
            output["actions"] = serde_json::json!([]);
        }
        output
    }
}

fn load_scenario_value(scenario_id: &str) -> serde_json::Value {
    let text =
        std::fs::read_to_string(fixture_root().join(format!("scenarios/{scenario_id}.json")))
            .expect("scenario reads");
    serde_json::from_str(&text).expect("scenario parses")
}

/// One evaluated literal assertion, kept for labelled diagnostics.
struct StepAssertionRecord {
    sid: String,
    expression: String,
    passed: bool,
}

/// Execute one immutable ordered scenario end to end: every wire step runs
/// the genuine decoder/engine/encoder path, every `disconnect` drops the
/// full engine (the next handshake consumes the next generation seed),
/// OPEN/TBD gates are recorded without an invented outcome, and every
/// settled assertion is evaluated literally against `{output,history}`.
/// Core outputs expose the decoded message, its exact bytes and empty
/// actions. Unsupported input kinds, event kinds or expect statuses fail
/// instead of being skipped.
///
/// Test-local execution variants live in [`run_loaded_scenario`]: an ordered
/// step cap for the honest green prefix, and a diagnostic mode that records
/// every literal assertion with per-step output. This wrapper preserves the
/// strict full-scenario behavior: any false assertion panics.
fn run_ordered_scenario(scenario_id: &str) -> OrderedRun {
    run_loaded_scenario(
        &load_scenario_value(scenario_id),
        scenario_id,
        usize::MAX,
        false,
    )
    .0
}

/// Test-local execution of one ordered scenario value (fixture file content
/// or a test-local mutated copy): `max_steps` caps the ordered prefix. With
/// `diagnostic`, every settled assertion is evaluated, recorded and printed
/// instead of panicking on a literal false (jq malfunctions still panic);
/// strict mode panics on any false. Returns the run counters plus one record
/// per evaluated literal assertion.
fn run_loaded_scenario(
    scenario: &serde_json::Value,
    scenario_id: &str,
    max_steps: usize,
    diagnostic: bool,
) -> (OrderedRun, Vec<StepAssertionRecord>) {
    let mut records: Vec<StepAssertionRecord> = Vec::new();
    assert_eq!(
        scenario.get("format").and_then(serde_json::Value::as_str),
        Some("world-wire-v4-ordered-scenario"),
        "scenario format"
    );
    assert_eq!(
        scenario.get("id").and_then(serde_json::Value::as_str),
        Some(scenario_id),
        "scenario id"
    );
    let driver = scenario
        .get("driver")
        .and_then(serde_json::Value::as_str)
        .expect("scenario driver")
        .to_owned();
    assert!(
        driver == "core" || driver == "core-and-adapter",
        "unsupported scenario driver {driver}"
    );
    let initial = scenario.get("initial").expect("scenario initial");
    assert_eq!(
        initial
            .get("freshEngine")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "fresh engine"
    );
    let generations: Vec<String> = initial
        .get("generationSequence")
        .and_then(serde_json::Value::as_array)
        .expect("generation sequence")
        .iter()
        .map(|seed| seed.as_str().expect("generation seed string").to_owned())
        .collect();
    assert!(!generations.is_empty(), "generation sequence");
    let adapter = initial
        .get("adapter")
        .and_then(serde_json::Value::as_str)
        .expect("initial adapter")
        .to_owned();
    assert_eq!(adapter, "adapter-a", "initial adapter");
    let mut host: Option<VisibilityHostDriver> = None;
    if driver == "core" {
        assert_eq!(
            initial.get("native"),
            Some(&serde_json::json!({})),
            "empty native initial state"
        );
    } else {
        host = Some(VisibilityHostDriver::new(
            initial.get("native").expect("initial native state"),
            &adapter,
            &generations[0],
        ));
    }

    let mut engine = Some(WorldEngine::new(&generations[0]).expect("fresh engine seeds"));
    let mut gen_idx = 0usize;
    let mut history = serde_json::Map::new();
    let mut storage = StackingStorageDriver::default();
    let mut run = OrderedRun {
        steps: 0,
        wires: 0,
        handshakes: 0,
        settings: 0,
        reports: 0,
        events: 0,
        assertions: 0,
        pending: 0,
        pending_topics: Vec::new(),
        generations: generations.clone(),
        generations_consumed: 1,
        adapter_replies: 0,
        prepare_requests: 0,
        native_events: 0,
    };
    let steps = scenario
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .expect("scenario steps array");
    for step in steps.iter().take(max_steps) {
        let sid = step
            .get("id")
            .and_then(serde_json::Value::as_str)
            .expect("step id");
        let input = step.get("input").expect("step input");
        let expect = step.get("expect").expect("step expect");
        let status = expect
            .get("status")
            .and_then(serde_json::Value::as_str)
            .expect("step status");
        let assertions: Vec<&str> = expect
            .get("assertions")
            .and_then(serde_json::Value::as_array)
            .expect("step assertions")
            .iter()
            .map(|item| item.as_str().expect("assertion string"))
            .collect();
        let kind = input
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .expect("input kind");
        assert!(!history.contains_key(sid), "{sid} duplicate step id");
        let output = match kind {
            "wire" => {
                run.wires += 1;
                let to = input
                    .get("to")
                    .and_then(serde_json::Value::as_str)
                    .expect("input target");
                match to {
                    "core" => {
                        let message = input.get("message").expect("step message");
                        let wire = serde_json::to_string(message).expect("message serializes");
                        let (message_value, bytes) = match message
                            .get("type")
                            .and_then(serde_json::Value::as_str)
                        {
                            Some("handshake") => {
                                let request =
                                    decode_handshake_request(&wire).expect("handshake decodes");
                                assert_eq!(request.adapter, adapter, "{sid} handshake adapter");
                                if engine.is_none() {
                                    gen_idx += 1;
                                    assert!(
                                        gen_idx < generations.len(),
                                        "{sid} no reserved generation for reconnect"
                                    );
                                    engine = Some(
                                        WorldEngine::new(&generations[gen_idx])
                                            .expect("reconnect engine seeds"),
                                    );
                                    run.generations_consumed += 1;
                                }
                                let live = engine.as_mut().expect("{sid} no live connection");
                                let reply = live.handshake(&request);
                                assert_eq!(
                                    reply.generation.as_deref(),
                                    Some(generations[gen_idx].as_str()),
                                    "{sid} handshake generation binding"
                                );
                                run.handshakes += 1;
                                let text = encode_handshake_reply(&reply).unwrap_or_else(|error| {
                                    panic!("{sid} handshake reply encodes: {error:?}")
                                });
                                let value: serde_json::Value =
                                    serde_json::from_str(&text).expect("handshake reply parses");
                                (value, text)
                            }
                            Some("settings") => {
                                let request = decode_settings(&wire).expect("settings decodes");
                                assert_eq!(request.adapter, adapter, "{sid} settings adapter");
                                assert_eq!(
                                    request.generation, generations[gen_idx],
                                    "{sid} settings generation"
                                );
                                let live = engine.as_mut().expect("{sid} no live connection");
                                let reply =
                                    live.handle_settings(&request).expect("settings admitted");
                                run.settings += 1;
                                let text = encode_settings_reply(&reply).unwrap_or_else(|error| {
                                    panic!("{sid} settings reply encodes: {error:?}")
                                });
                                let value: serde_json::Value =
                                    serde_json::from_str(&text).expect("settings reply parses");
                                (value, text)
                            }
                            None => {
                                let decoded = decode_report_v4(&wire).expect("report decodes");
                                // Precondition checks: the bridge intentionally performs no
                                // envelope admission, so the runner pins the envelope
                                // binding before calling it.
                                assert_eq!(
                                    decoded.report.adapter_incarnation, adapter,
                                    "{sid} report adapter"
                                );
                                assert_eq!(
                                    decoded.report.core_generation, generations[gen_idx],
                                    "{sid} report generation"
                                );
                                let live = engine.as_mut().expect("{sid} no live connection");
                                match live.handle_report_v4_for_test(&decoded.report) {
                                    Ok(reply) => {
                                        run.reports += 1;
                                        let wire_flags = ReplyWireOptionals {
                                            suspended_output_present: reply
                                                .presentation
                                                .suspended_output
                                                .is_some(),
                                        };
                                        let text = encode_reply_v4(&DecodedReplyV4 {
                                            reply,
                                            wire: wire_flags,
                                        })
                                        .unwrap_or_else(|error| {
                                            panic!("{sid} reply encodes: {error:?}")
                                        });
                                        let value: serde_json::Value =
                                            serde_json::from_str(&text).expect("reply parses");
                                        (value, text)
                                    }
                                    Err((kind, message)) => {
                                        panic!("{sid} refused: {kind}/{message}")
                                    }
                                }
                            }
                            Some(other) => panic!("{sid} unexpected message type {other}"),
                        };
                        serde_json::json!({
                            "message": message_value,
                            "bytes": bytes,
                            "actions": [],
                        })
                    }
                    "adapter" => {
                        assert_eq!(
                            driver, "core-and-adapter",
                            "{sid} adapter wire without host driver"
                        );
                        let message = input.get("message").expect("step message");
                        let wire = serde_json::to_string(message).expect("message serializes");
                        // Genuine adapter-directed reply decode; the offline
                        // host driver then executes the authorized visibility
                        // mechanics. No core engine involvement.
                        let decoded = decode_reply_v4(&wire).expect("adapter reply decodes");
                        let host_ref = host.as_mut().expect("{sid} no host driver");
                        run.adapter_replies += 1;
                        host_ref.execute_reply(sid, &decoded.reply)
                    }
                    other => panic!("{sid} unsupported input target {other}"),
                }
            }
            "fixture-event" => {
                run.events += 1;
                let to = input
                    .get("to")
                    .and_then(serde_json::Value::as_str)
                    .expect("event target");
                let event = input.get("event").expect("step event");
                let event_kind = event
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .expect("event kind");
                match (to, event_kind) {
                    ("connection", "disconnect") => {
                        // Full engine drop: destroys connection correlations,
                        // core lifetime state and model grants without replay.
                        engine = None;
                        serde_json::json!({"actions": []})
                    }
                    ("platform-evidence", "evidence-gate") => {
                        // Explicit pending gate: recorded, never executed.
                        // A settled evidence gate is rejected below; TBD
                        // records TBD only when the fixture says so.
                        assert!(
                            status == "OPEN" || status == "TBD",
                            "{sid} evidence gate must stay pending"
                        );
                        let topic = event
                            .get("topic")
                            .and_then(serde_json::Value::as_str)
                            .expect("evidence gate topic");
                        assert_eq!(
                            topic, "windows-app-owned-fullscreen-exit",
                            "{sid} unsupported evidence topic"
                        );
                        serde_json::json!({
                            "actions": [],
                            "pending": topic,
                            "status": status,
                        })
                    }
                    ("adapter", "prepare-request") => {
                        assert_eq!(
                            driver, "core-and-adapter",
                            "{sid} prepare-request without host driver"
                        );
                        let host_ref = host.as_mut().expect("{sid} no host driver");
                        run.prepare_requests += 1;
                        host_ref.prepare_request(sid, event)
                    }
                    ("native", "outside-change") => {
                        assert_eq!(
                            driver, "core-and-adapter",
                            "{sid} outside-change without host driver"
                        );
                        let host_ref = host.as_mut().expect("{sid} no host driver");
                        run.native_events += 1;
                        host_ref.apply_outside_change(sid, event)
                    }
                    ("native", "release-callback") => {
                        assert_eq!(
                            driver, "core-and-adapter",
                            "{sid} release-callback without host driver"
                        );
                        let host_ref = host.as_mut().expect("{sid} no host driver");
                        run.native_events += 1;
                        host_ref.apply_release_callback(sid, event)
                    }
                    ("native", "late-write-evidence") => {
                        assert_eq!(
                            driver, "core-and-adapter",
                            "{sid} late evidence without host driver"
                        );
                        let host_ref = host.as_mut().expect("{sid} no host driver");
                        run.native_events += 1;
                        host_ref.apply_late_write_evidence(sid, event)
                    }
                    (other_to, other_kind) => match (to, event_kind) {
                        ("storage", "recoverable-state") => {
                            let win = event
                                .get("win")
                                .and_then(serde_json::Value::as_str)
                                .expect("storage win");
                            let tag = event
                                .get("tag")
                                .and_then(serde_json::Value::as_str)
                                .expect("storage tag");
                            let target = event.get("target").expect("storage target");
                            let native = event.get("native").expect("storage native");
                            let writable = event
                                .get("slotWritable")
                                .and_then(serde_json::Value::as_bool)
                                .expect("storage slotWritable");
                            storage.recoverable_state(win, tag, target, native, writable)
                        }
                        ("storage", "restart-and-restore") => {
                            let win = event
                                .get("win")
                                .and_then(serde_json::Value::as_str)
                                .expect("storage win");
                            let tag = event
                                .get("tag")
                                .and_then(serde_json::Value::as_str)
                                .expect("storage tag");
                            storage.restart_and_restore(win, tag)
                        }
                        _ => panic!("{sid} unsupported fixture event {other_to}/{other_kind}"),
                    },
                }
            }
            other => panic!("{sid} unsupported input kind {other}"),
        };
        let envelope = serde_json::json!({"output": output, "history": history.clone()});
        match status {
            "settled" => {
                assert!(
                    !assertions.is_empty(),
                    "{sid} settled step must carry assertions"
                );
                for assertion in &assertions {
                    // Diagnostic mode records every literal result and prints
                    // it, so the run continues past semantic failures; the
                    // failed output still enters `history` below, keeping
                    // later steps honest. Strict mode panics on any false.
                    if diagnostic {
                        let passed = check_jq(sid, assertion, &envelope).is_ok();
                        println!(
                            "DIAG {sid}: {}: {assertion}",
                            if passed { "PASS" } else { "FAIL" }
                        );
                        records.push(StepAssertionRecord {
                            sid: sid.to_owned(),
                            expression: (*assertion).to_owned(),
                            passed,
                        });
                        run.assertions += 1;
                    } else {
                        assert_jq(sid, assertion, &envelope);
                        records.push(StepAssertionRecord {
                            sid: sid.to_owned(),
                            expression: (*assertion).to_owned(),
                            passed: true,
                        });
                        run.assertions += 1;
                    }
                }
            }
            "OPEN" | "TBD" => {
                assert!(
                    assertions.is_empty(),
                    "{sid} pending gate must not carry assertions"
                );
                run.pending += 1;
                run.pending_topics.push(
                    output
                        .get("pending")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(sid)
                        .to_owned(),
                );
            }
            other => panic!("{sid} unsupported expect status {other}"),
        }
        history.insert(sid.to_owned(), output);
        run.steps += 1;
    }
    // Reserved seeds validate but are never consumed or faked.
    for seed in generations.iter().skip(run.generations_consumed) {
        assert!(
            WorldEngine::new(seed).is_some(),
            "reserved generation seed validates"
        );
    }
    (run, records)
}

#[test]
fn raw_attempt_settlement_runs_ordered_core_scenario() {
    let run = run_ordered_scenario("raw-attempt-settlement");
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned()],
        "generation sequence"
    );
    // The second seed is reserved for lifetime/reset scenarios and is not
    // exercised here; it must validate without being consumed or faked.
    assert_eq!(
        run.generations_consumed, 1,
        "reserved generation seed unconsumed"
    );
    assert_eq!(run.steps, 12, "ordered step count");
    assert_eq!(run.wires, 12, "wire step count");
    assert_eq!(run.handshakes, 1, "handshake step count");
    assert_eq!(run.settings, 1, "settings step count");
    assert_eq!(run.reports, 10, "report step count");
    assert_eq!(run.events, 0, "event step count");
    assert_eq!(run.assertions, 14, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
fn window_state_runs_ordered_core_scenario() {
    let run = run_ordered_scenario("window-state");
    assert_eq!(
        run.generations,
        vec![
            "generation-a".to_owned(),
            "generation-b".to_owned(),
            "generation-c".to_owned(),
        ],
        "generation sequence"
    );
    assert_eq!(
        run.generations_consumed, 3,
        "every generation seed consumed across reconnects"
    );
    assert_eq!(run.steps, 28, "ordered step count");
    assert_eq!(run.wires, 25, "wire step count");
    assert_eq!(run.handshakes, 3, "handshake step count");
    assert_eq!(run.settings, 3, "settings step count");
    assert_eq!(run.reports, 19, "report step count");
    assert_eq!(run.events, 3, "event step count");
    assert_eq!(run.assertions, 56, "evaluated assertion count");
    // The native app-owned fullscreen baseline stays an explicit pending
    // gate; this scenario is not full acceptance.
    assert_eq!(run.pending, 1, "pending gate count");
    assert_eq!(
        run.pending_topics,
        vec!["windows-app-owned-fullscreen-exit".to_owned()],
        "pending gate topic"
    );
}

fn storage_target(property: &str, value: bool, auth: &str) -> serde_json::Value {
    serde_json::json!({"property": property, "value": value, "auth": auth, "reason": "", "cmdSeq": null})
}

fn storage_native(keep_above: bool, keep_below: bool) -> serde_json::Value {
    serde_json::json!({"keepAbove": keep_above, "keepBelow": keep_below})
}

#[test]
fn stacking_storage_persists_before_write() {
    let mut driver = StackingStorageDriver::default();
    let output = driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", true, "controlled"),
        &storage_native(false, true),
        true,
    );
    assert_eq!(
        output
            .get("actions")
            .and_then(|actions| actions.as_array())
            .map(|actions| actions
                .iter()
                .map(|action| action.get("kind").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()),
        Some(vec![Some("persist-preimage"), Some("state")]),
        "persist precedes the coupled write"
    );
    assert_eq!(
        output.get("native"),
        Some(&serde_json::json!({"keepAbove": true, "keepBelow": false})),
        "coupled write sets above and clears below"
    );
}

#[test]
fn stacking_storage_failure_writes_nothing() {
    let mut driver = StackingStorageDriver::default();
    let output = driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", true, "controlled"),
        &storage_native(false, false),
        false,
    );
    assert_eq!(
        output.get("actions"),
        Some(&serde_json::json!([])),
        "failed slot performs no actions"
    );
    assert_eq!(
        output.get("native"),
        Some(&serde_json::json!({"keepAbove": false, "keepBelow": false})),
        "failed slot leaves native facts unchanged"
    );
    assert_eq!(
        driver.restart_and_restore("win-a", "tag-1"),
        serde_json::json!({"actions": []}),
        "failed persist stores no preimage"
    );
}

#[test]
fn stacking_storage_restores_matching_lifetime_only() {
    let mut driver = StackingStorageDriver::default();
    driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", true, "controlled"),
        &storage_native(false, true),
        true,
    );
    assert_eq!(
        driver.restart_and_restore("win-a", "tag-2"),
        serde_json::json!({"actions": []}),
        "reused lifetime restores nothing"
    );
    assert_eq!(
        driver.restart_and_restore("win-a", "tag-1"),
        serde_json::json!({
            "actions": [{"kind": "restore"}],
            "native": {"keepAbove": false, "keepBelow": true},
        }),
        "matching lifetime restores the owned preimage"
    );
}

#[test]
#[should_panic(expected = "unsupported storage target property")]
fn stacking_storage_rejects_non_above_property() {
    let mut driver = StackingStorageDriver::default();
    driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("below", true, "controlled"),
        &storage_native(false, false),
        true,
    );
}

#[test]
#[should_panic(expected = "unsupported storage target authority")]
fn stacking_storage_rejects_uncontrolled_auth() {
    let mut driver = StackingStorageDriver::default();
    driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", true, "native"),
        &storage_native(false, false),
        true,
    );
}

#[test]
#[should_panic(expected = "unsupported storage target release")]
fn stacking_storage_rejects_false_target_release() {
    let mut driver = StackingStorageDriver::default();
    driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", false, "controlled"),
        &storage_native(true, false),
        true,
    );
}

#[test]
fn stacking_storage_rejected_target_persists_nothing() {
    use std::panic::AssertUnwindSafe;
    let mut driver = StackingStorageDriver::default();
    let rejected = std::panic::catch_unwind(AssertUnwindSafe(|| {
        driver.recoverable_state(
            "win-a",
            "tag-1",
            &storage_target("below", true, "controlled"),
            &storage_native(false, false),
            true,
        )
    }));
    assert!(rejected.is_err(), "unsupported target must be rejected");
    assert_eq!(
        driver.restart_and_restore("win-a", "tag-1"),
        serde_json::json!({"actions": []}),
        "rejected target stores no preimage"
    );
}

#[test]
fn persistent_above_prefix_first_twenty_steps_pass() {
    // Honest green prefix only: the first 20 ordered steps (one handshake,
    // one settings, eighteen reports, 24 literal assertions) run the genuine
    // decoder/engine/encoder path and pass. This is not a scenario pass.
    let scenario = load_scenario_value("persistent-above");
    let (run, _) = run_loaded_scenario(&scenario, "persistent-above", 20, false);
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned(),],
        "generation sequence"
    );
    // The second seed is reserved for the later reconnect and is not
    // exercised here; it must validate without being consumed or faked.
    assert_eq!(
        run.generations_consumed, 1,
        "reserved generation seed unconsumed"
    );
    assert_eq!(run.steps, 20, "ordered step count");
    assert_eq!(run.wires, 20, "wire step count");
    assert_eq!(run.handshakes, 1, "handshake step count");
    assert_eq!(run.settings, 1, "settings step count");
    assert_eq!(run.reports, 18, "report step count");
    assert_eq!(run.events, 0, "event step count");
    assert_eq!(run.assertions, 24, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
#[ignore = "7f-3b blockers (parked for user): capacity oracle contradicts recorded D1 admission; workspace-mode oracle contradicts recorded floating-scope untouched-row semantics; see docs/changes/v4-phase1-core-protocol.md phase note"]
fn persistent_above_runs_ordered_core_scenario() {
    let run = run_ordered_scenario("persistent-above");
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned(),],
        "generation sequence"
    );
    assert_eq!(
        run.generations_consumed, 2,
        "every generation seed consumed across reconnects"
    );
    assert_eq!(run.steps, 30, "ordered step count");
    assert_eq!(run.wires, 25, "wire step count");
    assert_eq!(run.handshakes, 2, "handshake step count");
    assert_eq!(run.settings, 2, "settings step count");
    assert_eq!(run.reports, 21, "report step count");
    assert_eq!(run.events, 5, "event step count");
    assert_eq!(run.assertions, 41, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
fn persistent_above_capacity_diagnostic_maps_post_failure_steps() {
    // DIAGNOSTIC ONLY, not scenario evidence: runs all 30 ordered steps in
    // record-everything mode and pins the exact failing literal assertions.
    // Observed failures: step 21 `capacity-float-acquire` (sole
    // oversized-minimum newcomer tiles under recorded D1) and step 22
    // `workspace-mode-float-acquire`'s win-mode-float assertion (win-mode
    // arrives with no allocation and floating:false). The floating-scope
    // publication arm fixes floating:false independently of prior capacity;
    // bundled/split shape diagnostics below pin controlled above and rule
    // out same-report command/newcomer ordering. Steps 23-30 pass.
    let scenario = load_scenario_value("persistent-above");
    let assertion_at = |index: usize, position: usize| {
        scenario["steps"][index]["expect"]["assertions"][position]
            .as_str()
            .expect("fixture assertion string")
            .to_owned()
    };
    let expected = vec![
        ("capacity-float-acquire".to_owned(), assertion_at(20, 0)),
        (
            "workspace-mode-float-acquire".to_owned(),
            assertion_at(21, 1),
        ),
    ];
    let (run, records) = run_loaded_scenario(&scenario, "persistent-above", usize::MAX, true);
    assert_eq!(run.steps, 30, "diagnostic runs every ordered step");
    assert_eq!(run.reports, 21, "diagnostic runs every report");
    assert_eq!(records.len(), 41, "every literal assertion evaluated");
    let failures: Vec<(String, String)> = records
        .iter()
        .filter(|record| !record.passed)
        .map(|record| (record.sid.clone(), record.expression.clone()))
        .collect();
    assert_eq!(
        failures, expected,
        "exactly the two observed assertions fail, identified by step and expression"
    );
    println!(
        "DIAG summary: steps={} reports={} assertions={} failures=2 \
         (capacity-float-acquire, workspace-mode-float-acquire win-mode-float); \
         steps 23-30 (8 steps) pass with the actual tiled output in history",
        run.steps,
        run.reports,
        records.len(),
    );
}

#[test]
fn persistent_above_capacity_copy_two_newcomer_exhaustion() {
    // PAIRED DIAGNOSTIC ONLY, not scenario evidence or acceptance: a
    // test-local copy of the capacity step carrying two newcomers (a fresh
    // ordinary anchor plus win-capacity) on tiny-but-valid geometry. The
    // original capacity assertion and every remaining immutable step run
    // unchanged; step 22 closes win-capacity on the standard screen, so the
    // tiny geometry stays local to the copy. Serial admission tiles the
    // empty domain with the listed-first anchor, leaving win-capacity as the
    // geometrically exhausted second leg with pure 1x1 hints.
    //
    // Observed: the original capacity assertion PASSES here (genuine D1
    // exhaustion floats win-capacity with capacity origin and controlled
    // above), while step 22's win-mode-float assertion still FAILS with
    // steps 23-30 passing. Per `persistent_above_workspace_mode_split_sequence`
    // the step 22 failure is the oracle `floating == true` bit conflicting
    // with recorded floating-scope untouched-row semantics, not the step 21
    // tiling outcome. Decision evidence only.
    let mut scenario = load_scenario_value("persistent-above");
    {
        let steps = scenario
            .get_mut("steps")
            .and_then(|value| value.as_array_mut())
            .expect("scenario steps array");
        assert_eq!(
            steps[20].get("id").and_then(serde_json::Value::as_str),
            Some("capacity-float-acquire"),
            "copied step"
        );
        let message = steps[20]
            .pointer_mut("/input/message")
            .expect("step message");
        // Work 25x25 inset by outer 8 on every side gives 9x9 domain bounds;
        // a two-leaf split needs extent at least gap(8) + 2 = 10 on the split
        // axis. Every extent stays positive: valid geometry, not held output
        // or an invalid-bounds refusal.
        for screen in message
            .get_mut("screens")
            .and_then(|value| value.as_array_mut())
            .expect("step screens")
        {
            screen["full"] = serde_json::json!([0, 0, 32, 32]);
            screen["work"] = serde_json::json!([0, 0, 25, 25]);
        }
        let windows = message
            .get_mut("windows")
            .and_then(|value| value.as_array_mut())
            .expect("step windows");
        assert_eq!(windows.len(), 1, "copied step starts with one newcomer");
        let mut anchor = windows[0].clone();
        anchor["win"] = serde_json::json!("win-anchor");
        anchor["rect"] = serde_json::json!([8, 8, 8, 8]);
        anchor["hints"] = serde_json::json!({"minW": 1, "minH": 1, "maxW": null, "maxH": null});
        let capacity = &mut windows[0];
        capacity["rect"] = serde_json::json!([9, 9, 8, 8]);
        capacity["hints"] = serde_json::json!({"minW": 1, "minH": 1, "maxW": null, "maxH": null});
        // Anchor first: serial admission tiles the empty domain with it,
        // leaving win-capacity as the geometrically exhausted second leg.
        windows.insert(0, anchor);
        message["cursor"] = serde_json::json!([10, 10]);
        message["presentation"]["frame"] = serde_json::json!([8, 8, 8, 8]);
        message["presentation"]["pagerOrder"] = serde_json::json!(["win-anchor", "win-capacity"]);
        message["presentation"]["pagerFrames"] = serde_json::json!([
            {"win": "win-anchor", "live": [8, 8, 8, 8]},
            {"win": "win-capacity", "live": [9, 9, 8, 8]},
        ]);
        message["presentation"]["witness"]["frame"] = serde_json::json!([9, 9, 8, 8]);
    }
    let expected = vec![(
        "workspace-mode-float-acquire".to_owned(),
        scenario["steps"][21]["expect"]["assertions"][1]
            .as_str()
            .expect("fixture assertion string")
            .to_owned(),
    )];
    let (run, records) = run_loaded_scenario(&scenario, "persistent-above", usize::MAX, true);
    let failures: Vec<(String, String)> = records
        .iter()
        .filter(|record| !record.passed)
        .map(|record| (record.sid.clone(), record.expression.clone()))
        .collect();
    assert_eq!(
        failures, expected,
        "only step 22's win-mode-float assertion fails in the copy"
    );
    let capacity = records
        .iter()
        .find(|record| record.sid == "capacity-float-acquire")
        .expect("capacity record present");
    assert!(
        capacity.passed,
        "genuine exhaustion satisfies the original capacity oracle"
    );
    assert_eq!(run.steps, 30, "copy runs every ordered step");
    assert_eq!(run.assertions, 41, "copy evaluates every assertion");
}

#[test]
fn persistent_above_workspace_mode_split_sequence() {
    // DIAGNOSTIC ONLY: replays steps 0..=20 byte-identical, then splits
    // step 21 into a command-only flip plus a newcomer-only arrival (no
    // second window, no geometry change). Arrival carries the original
    // conjunction plus clauses pinning floating:false, untouched
    // authorities with empty float_origin, and controlled above.
    // Observed: only the parked capacity assertion and the oracle
    // conjunction fail; mode flip and all three shape clauses pass. Cause
    // is the oracle `floating == true` bit versus recorded floating-scope
    // untouched-row semantics (`world_publish.rs` floating-scope arm;
    // `world_above.rs::workspace_floating_scope_rows_acquire_above`
    // asserts `!row.floating`); W1 above-eligibility holds. Needs a user
    // decision, not a product fix.
    let mut scenario = load_scenario_value("persistent-above");
    {
        let steps = scenario
            .get("steps")
            .and_then(|value| value.as_array())
            .expect("scenario steps array");
        assert_eq!(
            steps[21].get("id").and_then(serde_json::Value::as_str),
            Some("workspace-mode-float-acquire"),
            "split step"
        );
    }
    let steps = scenario
        .get_mut("steps")
        .and_then(|value| value.as_array_mut())
        .expect("scenario steps array");
    let original = steps[21].clone();
    let original_message = original
        .get("input")
        .expect("step input")
        .get("message")
        .expect("step message")
        .clone();
    let mode_assertion = original["expect"]["assertions"][0]
        .as_str()
        .expect("mode assertion string")
        .to_owned();
    let win_assertion = original["expect"]["assertions"][1]
        .as_str()
        .expect("win assertion string")
        .to_owned();
    let mut flip_message = original_message.clone();
    flip_message["windows"] = serde_json::json!([]);
    flip_message["focused"] = serde_json::json!("");
    flip_message["presentation"]["foreground"] = serde_json::json!("");
    flip_message["presentation"]["witness"] = serde_json::Value::Null;
    let flip = serde_json::json!({
        "id": "workspace-mode-flip-only",
        "input": {"kind": "wire", "to": "core", "message": flip_message},
        "expect": {"status": "settled", "assertions": [mode_assertion]},
    });
    let mut arrival_message = original_message.clone();
    arrival_message["closed"] = serde_json::json!([]);
    arrival_message["cmdSeq"] = serde_json::Value::Null;
    arrival_message["command"] = serde_json::Value::Null;
    arrival_message["obs"] = serde_json::json!("21");
    arrival_message["reqSeq"] = serde_json::json!("21");
    arrival_message["req"] = serde_json::json!("request-21");
    arrival_message["presentation"]["witness"]["obs"] = serde_json::json!("21");
    let floating_false =
        ".output.message.windows | any(.win == \"win-mode\" and .floating == false)".to_owned();
    let untouched = ".output.message.windows | any(.win == \"win-mode\" and .frame_auth == \"untouched\" and .member_auth == \"untouched\" and .float_origin == \"\")".to_owned();
    let above_only = ".output.message.windows | any(.win == \"win-mode\" and any(.state[]; .property == \"above\" and .value == true and .auth == \"controlled\"))".to_owned();
    let arrival_assertions = vec![win_assertion, floating_false, untouched, above_only];
    let arrival = serde_json::json!({
        "id": "workspace-mode-newcomer-only",
        "input": {"kind": "wire", "to": "core", "message": arrival_message},
        "expect": {"status": "settled", "assertions": arrival_assertions},
    });
    let mut rebuilt: Vec<serde_json::Value> = Vec::new();
    for (index, step) in steps.clone().iter().enumerate() {
        if index == 21 {
            rebuilt.push(flip.clone());
            rebuilt.push(arrival.clone());
        } else {
            rebuilt.push(step.clone());
        }
    }
    scenario["steps"] = serde_json::Value::Array(rebuilt);
    let (run, records) = run_loaded_scenario(&scenario, "persistent-above", usize::MAX, true);
    assert_eq!(run.steps, 31, "split runs every ordered step plus one");
    assert_eq!(run.assertions, 44, "split evaluates every assertion");
    let failures: Vec<(String, String)> = records
        .iter()
        .filter(|record| !record.passed)
        .map(|record| (record.sid.clone(), record.expression.clone()))
        .collect();
    let capacity_assertion = scenario["steps"][20]["expect"]["assertions"][0]
        .as_str()
        .expect("capacity assertion string")
        .to_owned();
    let win_conjunction = scenario["steps"][22]["expect"]["assertions"][0]
        .as_str()
        .expect("arrival conjunction string")
        .to_owned();
    assert_eq!(
        failures,
        vec![
            ("capacity-float-acquire".to_owned(), capacity_assertion),
            ("workspace-mode-newcomer-only".to_owned(), win_conjunction),
        ],
        "only the parked capacity assertion and the oracle conjunction fail"
    );
    let shape_passed = |sid: &str, expression: &str| {
        records
            .iter()
            .any(|record| record.sid == sid && record.expression == expression && record.passed)
    };
    for clause in scenario["steps"][22]["expect"]["assertions"]
        .as_array()
        .expect("arrival assertions")
        .iter()
        .skip(1)
    {
        let expression = clause.as_str().expect("clause string");
        assert!(
            shape_passed("workspace-mode-newcomer-only", expression),
            "shape clause passes: {expression}"
        );
    }
    println!(
        "DIAG summary: steps={} assertions={} failures=2 (capacity, oracle conjunction); \
         flip-only mode and all three newcomer shape clauses pass",
        run.steps,
        records.len(),
    );
}

#[test]
fn persistent_above_workspace_mode_bundled_shape_prefix() {
    // DIAGNOSTIC ONLY: test-local copy of the immutable bundled step 21
    // with three extra labelled clauses (floating:false, untouched
    // authorities with empty float_origin, controlled above); prefix run of
    // steps 0..=21 only. Observed: the two original assertions fail exactly
    // as in the unmodified run while all three shape clauses pass, pinning
    // the bundled row to the same untouched shape the split arrival shows.
    let mut scenario = load_scenario_value("persistent-above");
    let extra = vec![
        ".output.message.windows | any(.win == \"win-mode\" and .floating == false)".to_owned(),
        ".output.message.windows | any(.win == \"win-mode\" and .frame_auth == \"untouched\" and .member_auth == \"untouched\" and .float_origin == \"\")".to_owned(),
        ".output.message.windows | any(.win == \"win-mode\" and any(.state[]; .property == \"above\" and .value == true and .auth == \"controlled\"))".to_owned(),
    ];
    {
        let steps = scenario
            .get_mut("steps")
            .and_then(|value| value.as_array_mut())
            .expect("scenario steps array");
        assert_eq!(
            steps[21].get("id").and_then(serde_json::Value::as_str),
            Some("workspace-mode-float-acquire"),
            "copied step"
        );
        let assertions = steps[21]
            .pointer_mut("/expect/assertions")
            .and_then(|value| value.as_array_mut())
            .expect("step assertions");
        assert_eq!(
            assertions.len(),
            2,
            "bundled step carries two original assertions"
        );
        for clause in &extra {
            assertions.push(serde_json::Value::String(clause.clone()));
        }
    }
    let (run, records) = run_loaded_scenario(&scenario, "persistent-above", 22, true);
    assert_eq!(run.steps, 22, "prefix runs steps 0..=21");
    assert_eq!(records.len(), 30, "prefix evaluates every assertion");
    let failures: Vec<(String, String)> = records
        .iter()
        .filter(|record| !record.passed)
        .map(|record| (record.sid.clone(), record.expression.clone()))
        .collect();
    assert_eq!(
        failures.len(),
        2,
        "only the two original oracle assertions fail in the prefix"
    );
    assert!(
        failures
            .iter()
            .any(|(sid, _)| sid == "capacity-float-acquire"),
        "parked capacity assertion still fails"
    );
    assert!(
        failures
            .iter()
            .any(|(sid, _)| sid == "workspace-mode-float-acquire"),
        "oracle conjunction still fails bundled"
    );
    let bundled_passed = |expression: &str| {
        records.iter().any(|record| {
            record.sid == "workspace-mode-float-acquire"
                && record.expression == expression
                && record.passed
        })
    };
    for clause in &extra {
        assert!(
            bundled_passed(clause),
            "bundled shape clause passes: {clause}"
        );
    }
    println!(
        "DIAG summary: steps={} assertions={} failures=2 (capacity, oracle conjunction); \
         all three bundled shape clauses pass",
        run.steps,
        records.len(),
    );
}

/// The test bridge refuses an unbound engine, which remains usable for
/// the initial handshake afterwards.
#[test]
fn test_bridge_refuses_unbound_engine() {
    let text =
        std::fs::read_to_string(fixture_root().join("scenarios/raw-attempt-settlement.json"))
            .expect("scenario reads");
    let scenario: serde_json::Value = serde_json::from_str(&text).expect("scenario parses");
    let steps = scenario
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .expect("scenario steps array");
    let message_of = |wanted: &str| {
        steps
            .iter()
            .find(|step| step.get("id").and_then(serde_json::Value::as_str) == Some(wanted))
            .unwrap_or_else(|| panic!("{wanted} step present"))
            .get("input")
            .expect("step input")
            .get("message")
            .expect("step message")
            .clone()
    };
    let report = decode_report_v4(
        &serde_json::to_string(&message_of("observe")).expect("message serializes"),
    )
    .expect("report decodes")
    .report;
    let mut engine = WorldEngine::new("generation-a").expect("fresh engine seeds");
    assert!(
        engine.handle_report_v4_for_test(&report).is_err(),
        "unbound engine refuses"
    );
    let handshake = message_of("handshake");
    let request =
        decode_handshake_request(&serde_json::to_string(&handshake).expect("message serializes"))
            .expect("handshake decodes");
    let reply = engine.handshake(&request);
    assert_eq!(reply.generation.as_deref(), Some("generation-a"));
    assert!(!reply.ready);
}

#[test]
fn linked_selectors_warp_runs_ordered_core_scenario() {
    let run = run_ordered_scenario("linked-selectors-warp");
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned(),],
        "generation sequence"
    );
    assert_eq!(
        run.generations_consumed, 2,
        "every generation seed consumed across reconnects"
    );
    assert_eq!(run.steps, 10, "ordered step count");
    assert_eq!(run.wires, 9, "wire step count");
    assert_eq!(run.handshakes, 2, "handshake step count");
    assert_eq!(run.settings, 2, "settings step count");
    assert_eq!(run.reports, 5, "report step count");
    assert_eq!(run.events, 1, "event step count");
    assert_eq!(run.assertions, 16, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
fn visibility_order_failure_prefix_sixteen_steps_pass() {
    // Honest green prefix only: the first 16 ordered steps (one handshake,
    // one settings, four adapter replies with paired prepare-requests, two
    // native outside-changes and four core accounting reports) run the
    // genuine decoder/bridge/driver path and satisfy all 24 literal
    // assertions. This is not a scenario pass.
    let scenario = load_scenario_value("visibility-order-failure");
    let (run, _) = run_loaded_scenario(&scenario, "visibility-order-failure", 16, false);
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned(),],
        "generation sequence"
    );
    assert_eq!(
        run.generations_consumed, 1,
        "reserved generation seed unconsumed"
    );
    assert_eq!(run.steps, 16, "ordered step count");
    assert_eq!(run.wires, 10, "wire step count");
    assert_eq!(run.handshakes, 1, "handshake step count");
    assert_eq!(run.settings, 1, "settings step count");
    assert_eq!(run.reports, 4, "report step count");
    assert_eq!(run.adapter_replies, 4, "adapter reply count");
    assert_eq!(run.prepare_requests, 4, "prepare-request count");
    assert_eq!(run.native_events, 2, "native event count");
    assert_eq!(run.events, 6, "event step count");
    assert_eq!(run.assertions, 24, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
#[ignore = "7f-4 parked oracle: step-17 switchSettled needs shown-member win-a rev4 LastWrite evidence absent from all five core reports; effect rev4 correlates; see docs/changes/v4-phase1-core-protocol.md phase note"]
fn visibility_order_failure_runs_ordered_host_evidence() {
    let run = run_ordered_scenario("visibility-order-failure");
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned(),],
        "generation sequence"
    );
    assert_eq!(
        run.generations_consumed, 1,
        "reserved generation seed unconsumed"
    );
    assert_eq!(run.steps, 17, "ordered step count");
    assert_eq!(run.wires, 11, "wire step count");
    assert_eq!(run.handshakes, 1, "handshake step count");
    assert_eq!(run.settings, 1, "settings step count");
    assert_eq!(run.reports, 5, "report step count");
    assert_eq!(run.adapter_replies, 4, "adapter reply count");
    assert_eq!(run.prepare_requests, 4, "prepare-request count");
    assert_eq!(run.native_events, 2, "native event count");
    assert_eq!(run.events, 6, "event step count");
    assert_eq!(run.assertions, 26, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
fn visibility_order_failure_switch_settled_diagnostic_pins_single_oracle() {
    // DIAGNOSTIC ONLY, not scenario evidence: runs all 17 ordered steps in
    // record-everything mode and pins the exact failing literal assertion.
    // Observed: only step 17 `accounting-arrival-focus-depart-proof`'s
    // switchSettled oracle fails (genuine core returns switchSettled []
    // because every core report carries win-a LastWrite null, so the shown
    // member rev4 verdict never latches while the effect rev4 correlates);
    // the companion win-old ws-1 oracle on the same step passes, as do the
    // other 25 assertions. Parked for a user decision, not a product fix.
    let scenario = load_scenario_value("visibility-order-failure");
    let expected = vec![(
        "accounting-arrival-focus-depart-proof".to_owned(),
        scenario["steps"][16]["expect"]["assertions"][0]
            .as_str()
            .expect("fixture assertion string")
            .to_owned(),
    )];
    let (run, records) =
        run_loaded_scenario(&scenario, "visibility-order-failure", usize::MAX, true);
    assert_eq!(run.steps, 17, "diagnostic runs every ordered step");
    assert_eq!(run.reports, 5, "diagnostic runs every core report");
    assert_eq!(records.len(), 26, "every literal assertion evaluated");
    let failures: Vec<(String, String)> = records
        .iter()
        .filter(|record| !record.passed)
        .map(|record| (record.sid.clone(), record.expression.clone()))
        .collect();
    assert_eq!(
        failures, expected,
        "exactly the step-17 switchSettled oracle fails, identified by step and expression"
    );
    println!(
        "DIAG summary: steps={} reports={} assertions={} failures=1 \
         (accounting-arrival-focus-depart-proof switchSettled); \
         the step-17 ws oracle and steps 1-16 pass",
        run.steps,
        run.reports,
        records.len(),
    );
}

#[test]
fn visibility_cross_output_supersession_runs_ordered_host_evidence() {
    let run = run_ordered_scenario("visibility-cross-output-supersession");
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned(),],
        "generation sequence"
    );
    assert_eq!(
        run.generations_consumed, 1,
        "reserved generation seed unconsumed"
    );
    assert_eq!(run.steps, 18, "ordered step count");
    assert_eq!(run.wires, 8, "wire step count");
    assert_eq!(run.handshakes, 1, "handshake step count");
    assert_eq!(run.settings, 1, "settings step count");
    assert_eq!(run.reports, 0, "report step count");
    assert_eq!(run.adapter_replies, 6, "adapter reply count");
    assert_eq!(run.prepare_requests, 6, "prepare-request count");
    assert_eq!(run.native_events, 4, "native event count");
    assert_eq!(run.events, 10, "event step count");
    assert_eq!(run.assertions, 24, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

fn visibility_test_host(scenario_id: &str) -> VisibilityHostDriver {
    let scenario = load_scenario_value(scenario_id);
    let initial = scenario.get("initial").expect("scenario initial");
    VisibilityHostDriver::new(
        initial.get("native").expect("initial native state"),
        "adapter-a",
        "generation-a",
    )
}

#[test]
#[should_panic(expected = "unsupported refusal kind")]
fn visibility_driver_rejects_unsupported_refusal_kind() {
    let native = serde_json::json!({
        "obs": "1",
        "foreground": "win-old",
        "windows": [{
            "win": "win-old", "out": "out-a", "frame": [8, 8, 800, 600],
            "visible": true, "suppressionOwned": false, "minimized": false,
            "hostHidden": false, "fullscreen": false, "game": false,
            "lifetimeMatched": true, "preimageAvailable": true, "tag": "tag-old",
        }],
        "refusals": [{"kind": "bogus", "win": "win-old"}],
    });
    let _ = VisibilityHostDriver::new(&native, "adapter-a", "generation-a");
}

#[test]
#[should_panic(expected = "unsupported outside window key")]
fn visibility_driver_rejects_unsupported_outside_key() {
    let mut driver = visibility_test_host("visibility-order-failure");
    let event = serde_json::json!({
        "obs": "2",
        "windows": [{"win": "win-old", "bogus": true}],
    });
    driver.apply_outside_change("probe", &event);
}

#[test]
#[should_panic(expected = "unsupported release action")]
fn visibility_driver_rejects_unsupported_release_action() {
    let mut driver = visibility_test_host("visibility-cross-output-supersession");
    let event = serde_json::json!({
        "action": "hide",
        "win": "win-a",
        "setter": "accepted",
        "foreground": "win-a",
        "obs": "2",
    });
    driver.apply_release_callback("probe", &event);
}

#[test]
#[should_panic(expected = "unsupported late path")]
fn visibility_driver_rejects_unsupported_late_path() {
    let mut driver = visibility_test_host("visibility-cross-output-supersession");
    let event = serde_json::json!({
        "win": "win-old",
        "write": {
            "property": "visibility", "target": false, "revision": "3",
            "path": "staged", "setter": "refused",
        },
    });
    driver.apply_late_write_evidence("probe", &event);
}

fn visibility_step_message(scenario_id: &str, step_id: &str) -> serde_json::Value {
    load_scenario_value(scenario_id)["steps"]
        .as_array()
        .expect("scenario steps")
        .iter()
        .find(|step| step.get("id").and_then(serde_json::Value::as_str) == Some(step_id))
        .unwrap_or_else(|| panic!("{step_id} step present"))["input"]["message"]
        .clone()
}

fn visibility_step_event(scenario_id: &str, step_id: &str) -> serde_json::Value {
    load_scenario_value(scenario_id)["steps"]
        .as_array()
        .expect("scenario steps")
        .iter()
        .find(|step| step.get("id").and_then(serde_json::Value::as_str) == Some(step_id))
        .unwrap_or_else(|| panic!("{step_id} step present"))["input"]["event"]
        .clone()
}

#[test]
#[should_panic(expected = "unprepared")]
fn visibility_driver_rejects_unprepared_reply() {
    let mut driver = visibility_test_host("visibility-order-failure");
    let message = visibility_step_message("visibility-order-failure", "arrive-focus-depart");
    let wire = serde_json::to_string(&message).expect("message serializes");
    let decoded = decode_reply_v4(&wire).expect("reply decodes");
    driver.execute_reply("probe", &decoded.reply);
}

#[test]
#[should_panic(expected = "reused")]
fn visibility_driver_rejects_reused_request() {
    let mut driver = visibility_test_host("visibility-order-failure");
    let prepare = visibility_step_event("visibility-order-failure", "arrive-focus-depart-request");
    driver.prepare_request("probe", &prepare);
    let message = visibility_step_message("visibility-order-failure", "arrive-focus-depart");
    let wire = serde_json::to_string(&message).expect("message serializes");
    let decoded = decode_reply_v4(&wire).expect("reply decodes");
    driver.execute_reply("probe", &decoded.reply);
    driver.execute_reply("probe", &decoded.reply);
}

#[test]
#[should_panic(expected = "obs")]
fn visibility_driver_rejects_stale_obs_reply() {
    let mut driver = visibility_test_host("visibility-order-failure");
    let prepare = visibility_step_event("visibility-order-failure", "arrive-focus-depart-request");
    driver.prepare_request("probe", &prepare);
    let event = visibility_step_event(
        "visibility-order-failure",
        "restore-visible-for-next-switch",
    );
    driver.apply_outside_change("probe", &event);
    let message = visibility_step_message("visibility-order-failure", "arrive-focus-depart");
    let wire = serde_json::to_string(&message).expect("message serializes");
    let decoded = decode_reply_v4(&wire).expect("reply decodes");
    driver.execute_reply("probe", &decoded.reply);
}

#[test]
#[should_panic(expected = "ready")]
fn visibility_driver_rejects_not_ready_reply() {
    let mut driver = visibility_test_host("visibility-order-failure");
    let prepare = visibility_step_event("visibility-order-failure", "arrive-focus-depart-request");
    driver.prepare_request("probe", &prepare);
    let mut message = visibility_step_message("visibility-order-failure", "arrive-focus-depart");
    message["ready"] = serde_json::json!(false);
    let wire = serde_json::to_string(&message).expect("message serializes");
    let decoded = decode_reply_v4(&wire).expect("reply decodes");
    driver.execute_reply("probe", &decoded.reply);
}

#[test]
#[should_panic(expected = "stale visibility rev")]
fn visibility_driver_rejects_stale_visibility_rev() {
    let mut driver = visibility_test_host("visibility-order-failure");
    let prepare = visibility_step_event("visibility-order-failure", "arrive-focus-depart-request");
    driver.prepare_request("probe", &prepare);
    let message = visibility_step_message("visibility-order-failure", "arrive-focus-depart");
    let wire = serde_json::to_string(&message).expect("message serializes");
    let decoded = decode_reply_v4(&wire).expect("reply decodes");
    driver.execute_reply("probe", &decoded.reply);
    // Advance the modeled watermark to the prepared obs so pairing passes
    // and the stale revision itself is what fails.
    let advance = visibility_step_event(
        "visibility-order-failure",
        "restore-visible-for-next-switch",
    );
    driver.apply_outside_change("probe", &advance);
    let prepare2 = visibility_step_event(
        "visibility-order-failure",
        "failed-show-withholds-focus-request",
    );
    driver.prepare_request("probe", &prepare2);
    let mut stale =
        visibility_step_message("visibility-order-failure", "failed-show-withholds-focus");
    stale["windows"][1]["visibility"]["rev"] = serde_json::json!("0");
    stale["req"] = serde_json::json!("apply-2");
    let stale_wire = serde_json::to_string(&stale).expect("message serializes");
    let stale_decoded = decode_reply_v4(&stale_wire).expect("reply decodes");
    driver.execute_reply("probe", &stale_decoded.reply);
}

#[test]
#[should_panic(expected = "duplicate")]
fn visibility_driver_rejects_duplicate_effect_tuple() {
    let mut driver = visibility_test_host("visibility-cross-output-supersession");
    let prepare1 = visibility_step_event(
        "visibility-cross-output-supersession",
        "old-focus-issued-held-request",
    );
    driver.prepare_request("probe", &prepare1);
    let first = visibility_step_message(
        "visibility-cross-output-supersession",
        "old-focus-issued-held",
    );
    let wire1 = serde_json::to_string(&first).expect("message serializes");
    let decoded1 = decode_reply_v4(&wire1).expect("reply decodes");
    driver.execute_reply("probe", &decoded1.reply);
    let prepare2 = visibility_step_event(
        "visibility-cross-output-supersession",
        "new-other-output-focus-request",
    );
    driver.prepare_request("probe", &prepare2);
    let mut second = visibility_step_message(
        "visibility-cross-output-supersession",
        "new-other-output-focus",
    );
    second["effects"][0]["id"] = serde_json::json!("fx-1");
    let wire2 = serde_json::to_string(&second).expect("message serializes");
    let decoded2 = decode_reply_v4(&wire2).expect("reply decodes");
    driver.execute_reply("probe", &decoded2.reply);
}

#[test]
#[should_panic(expected = "unsupported minimized")]
fn visibility_driver_rejects_unsafe_initial_minimized() {
    let native = serde_json::json!({
        "obs": "1",
        "foreground": "win-old",
        "windows": [{
            "win": "win-old", "out": "out-a", "frame": [8, 8, 800, 600],
            "visible": true, "suppressionOwned": false, "minimized": true,
            "hostHidden": false, "fullscreen": false, "game": false,
            "lifetimeMatched": true, "preimageAvailable": true, "tag": "tag-old",
        }],
    });
    let _ = VisibilityHostDriver::new(&native, "adapter-a", "generation-a");
}

#[test]
#[should_panic(expected = "unmatched lifetime")]
fn visibility_driver_rejects_unmatched_initial_lifetime() {
    let native = serde_json::json!({
        "obs": "1",
        "foreground": "win-old",
        "windows": [{
            "win": "win-old", "out": "out-a", "frame": [8, 8, 800, 600],
            "visible": true, "suppressionOwned": false, "minimized": false,
            "hostHidden": false, "fullscreen": false, "game": false,
            "lifetimeMatched": false, "preimageAvailable": true, "tag": "tag-old",
        }],
    });
    let _ = VisibilityHostDriver::new(&native, "adapter-a", "generation-a");
}

#[test]
#[should_panic(expected = "unsupported native event key")]
fn visibility_driver_rejects_unknown_native_event_key() {
    let mut driver = visibility_test_host("visibility-order-failure");
    let event = serde_json::json!({
        "kind": "outside-change",
        "obs": "2",
        "windows": [],
        "bogus": true,
    });
    driver.apply_outside_change("probe", &event);
}

#[test]
fn visibility_driver_arrival_before_after_visible_after_show() {
    // Regression: arrival brackets must be sampled after modeled shows, so
    // every before/after sample is visible at the modeled watermark.
    let mut driver = visibility_test_host("visibility-order-failure");
    let prepare = visibility_step_event("visibility-order-failure", "arrive-focus-depart-request");
    driver.prepare_request("probe", &prepare);
    let message = visibility_step_message("visibility-order-failure", "arrive-focus-depart");
    let wire = serde_json::to_string(&message).expect("message serializes");
    let decoded = decode_reply_v4(&wire).expect("reply decodes");
    let output = driver.execute_reply("probe", &decoded.reply);
    let arrival = output["focusFeedback"][0]["arrival"]
        .as_array()
        .expect("arrival array")
        .clone();
    assert!(!arrival.is_empty(), "accepted focus carries brackets");
    for bracket in &arrival {
        assert_eq!(
            bracket["before"]["visibility"]["visible"],
            serde_json::json!(true),
            "arrival before is post-show visible"
        );
        assert_eq!(
            bracket["after"]["visibility"]["visible"],
            serde_json::json!(true),
            "arrival after is post-show visible"
        );
        assert_eq!(
            bracket["before"]["obs"], bracket["after"]["obs"],
            "before/after share the modeled watermark"
        );
    }
}

#[test]
fn visibility_driver_held_snapshot_preserved_across_revision() {
    // Regression: a held focus retains its issuing tuple, original pre
    // foreground and frozen multi-member arrival set across a later
    // revision and event; release replays them instead of rebuilding.
    let mut driver = visibility_test_host("visibility-cross-output-supersession");
    let prepare1 = visibility_step_event(
        "visibility-cross-output-supersession",
        "old-focus-issued-held-request",
    );
    driver.prepare_request("probe", &prepare1);
    let mut first = visibility_step_message(
        "visibility-cross-output-supersession",
        "old-focus-issued-held",
    );
    first["windows"][1]["visibility"] =
        serde_json::json!({"shown": true, "rev": "1", "fallback": false});
    let wire1 = serde_json::to_string(&first).expect("message serializes");
    let decoded1 = decode_reply_v4(&wire1).expect("reply decodes");
    let held_out = driver.execute_reply("probe", &decoded1.reply);
    assert_eq!(
        held_out["focusFeedback"],
        serde_json::json!([]),
        "held focus emits no feedback yet"
    );
    let prepare2 = visibility_step_event(
        "visibility-cross-output-supersession",
        "new-other-output-focus-request",
    );
    driver.prepare_request("probe", &prepare2);
    let second = visibility_step_message(
        "visibility-cross-output-supersession",
        "new-other-output-focus",
    );
    let wire2 = serde_json::to_string(&second).expect("message serializes");
    let decoded2 = decode_reply_v4(&wire2).expect("reply decodes");
    driver.execute_reply("probe", &decoded2.reply);
    let release = visibility_step_event(
        "visibility-cross-output-supersession",
        "older-focus-finishes",
    );
    let output = driver.apply_release_callback("probe", &release);
    let feedback = output["focusFeedback"][0].clone();
    assert_eq!(feedback["id"], serde_json::json!("fx-1"), "original tuple");
    assert_eq!(
        feedback["target"],
        serde_json::json!("win-a"),
        "original tuple"
    );
    assert_eq!(feedback["rev"], serde_json::json!("1"), "original tuple");
    assert_eq!(
        feedback["pre"],
        serde_json::json!("win-old"),
        "original pre foreground"
    );
    let arrival = feedback["arrival"]
        .as_array()
        .expect("arrival array")
        .clone();
    assert_eq!(arrival.len(), 2, "frozen multi-member arrival set");
    for bracket in &arrival {
        assert_eq!(
            bracket["before"]["visibility"]["visible"],
            serde_json::json!(true),
            "frozen before is visible"
        );
        assert_eq!(
            bracket["after"]["visibility"]["visible"],
            serde_json::json!(true),
            "frozen after is visible"
        );
    }
}

#[test]
fn visibility_driver_arrival_feedback_uses_raw_shape() {
    let mut driver = visibility_test_host("visibility-order-failure");
    let prepare = visibility_step_event("visibility-order-failure", "arrive-focus-depart-request");
    driver.prepare_request("probe", &prepare);
    let message = visibility_step_message("visibility-order-failure", "arrive-focus-depart");
    let wire = serde_json::to_string(&message).expect("message serializes");
    let decoded = decode_reply_v4(&wire).expect("reply decodes");
    let output = driver.execute_reply("probe", &decoded.reply);
    let feedback = output["focusFeedback"][0].clone();
    for key in [
        "id",
        "target",
        "rev",
        "setter",
        "reason",
        "pre",
        "post",
        "foregroundProven",
        "arrival",
        "attemptMs",
    ] {
        assert!(feedback.get(key).is_some(), "feedback carries {key}");
    }
    assert_eq!(
        feedback["rev"],
        serde_json::json!("1"),
        "feedback effect rev"
    );
    assert_eq!(
        feedback["pre"],
        serde_json::json!("win-old"),
        "focus pre image"
    );
    assert_eq!(
        feedback["post"],
        serde_json::json!("win-a"),
        "focus post image"
    );
    let bracket = feedback["arrival"][0].clone();
    for key in ["win", "obs", "before", "kind", "out", "frame", "after"] {
        assert!(bracket.get(key).is_some(), "arrival bracket carries {key}");
    }
    assert!(bracket.get("home").is_none(), "output arm carries no home");
    for sample in [&bracket["before"], &bracket["after"]] {
        assert!(
            sample.get("lifetimeMatched").is_some(),
            "sample carries lifetime"
        );
        assert!(
            sample["visibility"].get("suppressionOwned").is_some(),
            "sample carries suppression ownership"
        );
    }
    let write = output["lastWrites"][0]["write"].clone();
    assert!(write.get("pre").is_some(), "write carries pre snapshot");
    assert!(write.get("post").is_some(), "write carries post snapshot");
}

#[test]
fn visibility_driver_late_evidence_exposes_raw_write() {
    let mut driver = visibility_test_host("visibility-cross-output-supersession");
    let event = visibility_step_event("visibility-cross-output-supersession", "late-old-evidence");
    let output = driver.apply_late_write_evidence("probe", &event);
    assert_eq!(
        output["write"], event["write"],
        "late output exposes raw write"
    );
}
