//! Request dispatch over a pluggable host.
//!
//! [`Server`] owns caller ids, requested-vs-observed snapshots, and event
//! fan-out. Window effects go through [`Host`], which the live backend
//! implements with winit and tests implement with [`FakeHost`]. This keeps
//! protocol, validation, batch ordering, and unsupported-vs-accepted
//! semantics fully testable without initializing an event loop.

use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc::Sender;

use serde_json::json;

#[cfg(test)]
use crate::protocol::Attention;
use crate::protocol::{
    BatchItem, Event, FieldOutcome, Fields, Observed, Op, ParseFailure, Reply, Request, Snapshot,
    WindowLevel, WindowMode, WindowResult, X11WindowType, parse_request, validate_fields,
    validate_id,
};

/// Per-backend capability flags. Unsupported actions are reported, never faked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Outer/global positioning (`false` on Wayland: no global coordinates).
    pub position: bool,
    /// Un-minimizing (`false` on Wayland: no generic unminimize request).
    pub restore_from_minimize: bool,
    /// Display-mode exclusive fullscreen (`false` where it no-ops).
    pub exclusive_fullscreen: bool,
    /// Creation-only X11 window type (X11 only).
    pub x11_window_type: bool,
    /// Showing/hiding windows (`false` on Wayland: `set_visible` is a no-op
    /// and `is_visible` always returns `None` there).
    pub visible: bool,
    /// Stacking level (`false` on Wayland: `set_window_level` is a no-op).
    pub window_level: bool,
    /// Focus requests (`false` on Wayland: `focus_window` is a no-op).
    pub focus: bool,
    /// Decorated-window buttons (`false` on X11/Wayland: the winit setter
    /// and getter are both unimplemented there).
    pub enabled_buttons: bool,
    /// Resize increments (`false` on Wayland: no protocol support).
    pub resize_increments: bool,
    /// Runtime transparency (`false` on X11: only a creation-time visual).
    pub transparent_runtime: bool,
    /// Blur-behind (`false` on X11/Windows: the winit setter is a no-op).
    pub blur: bool,
    /// Creation-only X11 override-redirect (X11 only).
    pub override_redirect: bool,
}

/// Backend-name to capability mapping shared by every host implementation.
/// Extension-gated entries derive from the [`crate::platform`] registry so
/// the catalog stays the single source for backend-specific actions.
pub fn capabilities_for_backend(backend: &str) -> Capabilities {
    let extensions = crate::platform::supported_actions(backend);
    let ext = |name: &str| extensions.contains(&name);
    match backend {
        "x11" => Capabilities {
            position: true,
            restore_from_minimize: true,
            exclusive_fullscreen: true,
            x11_window_type: ext("x11_window_type"),
            visible: true,
            window_level: true,
            focus: true,
            enabled_buttons: false,
            resize_increments: true,
            transparent_runtime: false,
            blur: false,
            override_redirect: ext("x11_override_redirect"),
        },
        "windows" => Capabilities {
            position: true,
            restore_from_minimize: true,
            exclusive_fullscreen: true,
            x11_window_type: false,
            visible: true,
            window_level: true,
            focus: true,
            enabled_buttons: true,
            resize_increments: true,
            transparent_runtime: true,
            // winit Windows `set_blur` is a documented no-op.
            blur: false,
            override_redirect: false,
        },
        "macos" => Capabilities {
            position: true,
            restore_from_minimize: true,
            exclusive_fullscreen: true,
            x11_window_type: false,
            visible: true,
            window_level: true,
            focus: true,
            enabled_buttons: true,
            resize_increments: true,
            transparent_runtime: true,
            blur: true,
            override_redirect: false,
        },
        // Wayland: no global position, no generic unminimize, exclusive
        // fullscreen is a no-op in the windowing library, visibility
        // requests are ignored by the compositor, stacking/focus requests
        // are no-ops, button hints and resize increments have no protocol
        // support, and override-redirect does not exist.
        _ => Capabilities {
            position: false,
            restore_from_minimize: false,
            exclusive_fullscreen: false,
            x11_window_type: false,
            visible: false,
            window_level: false,
            focus: false,
            enabled_buttons: false,
            resize_increments: false,
            transparent_runtime: true,
            blur: true,
            override_redirect: false,
        },
    }
}

/// Shared restore gate used by the live backend and the fake host alike:
/// leaving `Minimized` for any other mode needs a host unminimize request,
/// which some backends lack. The gate keys off the last *accepted* minimize
/// request, never off `is_minimized()`, which is always `None` on Wayland.
pub fn restore_blocked(caps: &Capabilities, minimized: bool, mode: Option<WindowMode>) -> bool {
    mode.is_some_and(|m| m != WindowMode::Minimized) && !caps.restore_from_minimize && minimized
}

/// Physical size to logical size at a scale factor, for `logical_size_px`.
pub fn logical_size_for(size_px: [u32; 2], scale_factor: f64) -> [u32; 2] {
    [
        (f64::from(size_px[0]) / scale_factor).round() as u32,
        (f64::from(size_px[1]) / scale_factor).round() as u32,
    ]
}

/// A host notification originating from the windowing system itself.
///
/// The live backend reports these through full re-observations; the
/// variants remain the vocabulary tests and embedders construct.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    Resized {
        size_px: [u32; 2],
        scale_factor: f64,
    },
    Moved {
        position_outer_px: [i32; 2],
    },
    ClosedByUser,
}

/// Window created by the host: opaque key plus first observations.
/// `outcomes` reports one entry per requested field so spawn replies carry
/// the same per-field honesty as sets; `unsupported` lists the persistent
/// tags for the snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatedWindow {
    pub key: String,
    pub backend: String,
    pub native_handle: String,
    pub title: String,
    pub observed: Observed,
    pub configured: Fields,
    pub unsupported: Vec<String>,
    pub outcomes: Vec<FieldOutcome>,
}

/// Report for one `apply` call: per-field outcomes plus fresh observations.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyReport {
    pub outcomes: Vec<FieldOutcome>,
    pub observed: Observed,
    pub configured: Fields,
    pub unsupported: Vec<String>,
}

/// Window effects. Implementations must never invent observations: fields the
/// host cannot read back stay `None` (null on the wire).
pub trait Host {
    fn create(
        &mut self,
        id: &str,
        title: &str,
        requested: &Fields,
    ) -> Result<CreatedWindow, String>;
    /// Apply a patch. `full_title` is the server-constructed title when the
    /// patch carries a `title` suffix (`None` otherwise); the host sets it
    /// verbatim and reports the `title` outcome itself.
    fn apply(&mut self, key: &str, patch: &Fields, full_title: Option<&str>) -> ApplyReport;
    fn observe(&mut self, key: &str) -> Option<Observed>;
    fn destroy(&mut self, key: &str);
}

struct LiveEntry {
    host_key: String,
    title: String,
    backend: String,
    native_handle: String,
    requested: Fields,
    configured: Fields,
    observed: Observed,
    unsupported: Vec<String>,
    settled: bool,
    /// Whether `visible` was ever explicitly requested. Spawn defaults inject
    /// `visible:true` into `requested`, but an implicit default must never
    /// become a persistent Wayland `visible` unsupported tag.
    visible_explicit: bool,
}

/// The fixture server: id registry, snapshots, subscriptions, shutdown flag.
///
/// Subscriptions are connection-scoped: `conn` identifies one transport
/// connection (one event queue). A second `subscribe` from the same
/// connection replaces the previous filter instead of duplicating events;
/// `unsubscribe` removes that connection's entry. Disconnects are removed
/// via [`Server::remove_connection`] and lazily when a broadcast fails.
pub struct Server {
    pid: u32,
    run_tag: String,
    nonce: String,
    windows: BTreeMap<String, LiveEntry>,
    subscribers: HashMap<u64, (Sender<String>, Vec<String>, u64)>,
    next_subscriber: u64,
    shutting_down: bool,
}

impl Server {
    pub fn new(pid: u32, run_tag: String, nonce: String) -> Self {
        Self {
            pid,
            run_tag,
            nonce,
            windows: BTreeMap::new(),
            subscribers: HashMap::new(),
            next_subscriber: 1,
            shutting_down: false,
        }
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down
    }

    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    pub fn run_tag(&self) -> &str {
        &self.run_tag
    }

    /// Full title with an optional caller suffix appended after a space.
    /// Identity stays server-controlled: the suffix is test-useful text,
    /// never a replacement for the sentinel/id/run-tag prefix. Call with
    /// `None` for the bare identity title.
    pub fn title_for_suffix(&self, id: &str, suffix: Option<&str>) -> String {
        let base = format!(
            "{}:{id}:{}",
            crate::protocol::TITLE_SENTINEL,
            self.run_tag()
        );
        match suffix {
            Some(text) if !text.is_empty() => format!("{base} {text}"),
            _ => base,
        }
    }

    fn check_nonce(&self, req: &Request) -> Result<(), String> {
        if crate::transport::nonce_ok(&req.nonce, &self.nonce) {
            Ok(())
        } else {
            Err("bad nonce".to_string())
        }
    }

    /// Handle one raw line. Returns exactly one reply line. Broadcast events
    /// are pushed to subscribers separately. `conn` identifies the transport
    /// connection; subscription registration needs the caller's event sender
    /// together with that identity so repeats replace instead of duplicating.
    pub fn handle_line(
        &mut self,
        host: &mut impl Host,
        line: &str,
        conn: u64,
        events_tx: Option<&Sender<String>>,
    ) -> String {
        let reply = match parse_request(line) {
            Ok(req) => self.dispatch(host, &req, conn, events_tx),
            Err(ParseFailure {
                request_id,
                message,
            }) => Reply::fail(&request_id, message),
        };
        crate::protocol::reply_to_string(&reply)
    }

    /// Forget one transport connection's subscription, if any. IO calls this
    /// when a connection closes so a disconnected writer is never retained
    /// waiting for the next broadcast to notice.
    pub fn remove_connection(&mut self, conn: u64) {
        self.subscribers.remove(&conn);
    }

    fn dispatch(
        &mut self,
        host: &mut impl Host,
        req: &Request,
        conn: u64,
        events_tx: Option<&Sender<String>>,
    ) -> Reply {
        if let Err(message) = self.check_nonce(req) {
            return Reply::fail(&req.request_id, message);
        }
        match &req.op {
            Op::Spawn { windows } => {
                let mut results = Vec::with_capacity(windows.len());
                let mut seen = std::collections::HashSet::new();
                for spec in windows {
                    results.push(self.spawn_one(host, spec, &mut seen));
                }
                let ok = results.iter().all(|r: &WindowResult| r.ok);
                Reply {
                    v: 1,
                    request_id: req.request_id.clone(),
                    ok,
                    result: Some(json!({ "windows": results })),
                    error: None,
                }
            }
            Op::Set { ids, patch } => {
                if let Err(message) = validate_fields(patch) {
                    return Reply::fail(&req.request_id, message);
                }
                let mut results = Vec::with_capacity(ids.len());
                for id in ids {
                    results.push(self.set_one(host, id, patch));
                }
                let ok = results.iter().all(|r: &WindowResult| r.ok);
                Reply {
                    v: 1,
                    request_id: req.request_id.clone(),
                    ok,
                    result: Some(json!({ "windows": results })),
                    error: None,
                }
            }
            Op::Get { ids } => {
                let mut results = Vec::with_capacity(ids.len());
                for id in ids {
                    results.push(self.get_one(host, id));
                }
                let ok = results.iter().all(|r: &WindowResult| r.ok);
                Reply {
                    v: 1,
                    request_id: req.request_id.clone(),
                    ok,
                    result: Some(json!({ "windows": results })),
                    error: None,
                }
            }
            Op::Close { ids } => {
                let mut results = Vec::with_capacity(ids.len());
                for id in ids {
                    results.push(self.close_one(host, id));
                }
                let ok = results.iter().all(|r: &WindowResult| r.ok);
                Reply {
                    v: 1,
                    request_id: req.request_id.clone(),
                    ok,
                    result: Some(json!({ "windows": results })),
                    error: None,
                }
            }
            Op::List => {
                let ids: Vec<&String> = self.windows.keys().collect();
                Reply::ok(&req.request_id, json!({ "ids": ids }))
            }
            Op::Batch { items } => {
                let mut results = Vec::with_capacity(items.len());
                // Ordered and explicitly non-atomic: every item runs in order
                // and an item failure never rolls back or skips later items.
                for item in items {
                    results.push(self.batch_item(host, item));
                }
                let ok = results
                    .iter()
                    .all(|v| v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false));
                Reply {
                    v: 1,
                    request_id: req.request_id.clone(),
                    ok,
                    result: Some(json!({ "items": results })),
                    error: None,
                }
            }
            Op::Subscribe { events } => match events_tx {
                Some(tx) => {
                    let filter = events.clone().unwrap_or_else(|| vec!["all".to_string()]);
                    if filter.is_empty() {
                        return Reply::fail(
                            &req.request_id,
                            "subscribe needs a non-empty events filter",
                        );
                    }
                    for name in &filter {
                        if !matches!(
                            name.as_str(),
                            "created" | "state" | "closed" | "error" | "all"
                        ) {
                            return Reply::fail(
                                &req.request_id,
                                format!("unknown event filter '{name}'"),
                            );
                        }
                    }
                    let id = self.next_subscriber;
                    self.next_subscriber += 1;
                    // Connection-scoped: a repeat subscribe from the same
                    // connection replaces the previous filter instead of
                    // duplicating broadcasts.
                    self.subscribers
                        .insert(conn, (tx.clone(), filter.clone(), id));
                    let _ = tx.send(subscription_marker(id, &filter));
                    Reply::ok(
                        &req.request_id,
                        json!({ "subscribed": filter, "subscription": id }),
                    )
                }
                None => Reply::fail(&req.request_id, "subscriptions need a live connection"),
            },
            Op::Unsubscribe => {
                self.subscribers.remove(&conn);
                Reply::ok(&req.request_id, json!({ "subscribed": [] }))
            }
            Op::Shutdown => {
                self.shutting_down = true;
                Reply::ok(
                    &req.request_id,
                    json!({ "shutting_down": true, "open_windows": self.window_count() }),
                )
            }
        }
    }

    fn spawn_one(
        &mut self,
        host: &mut impl Host,
        spec: &crate::protocol::SpawnSpec,
        seen: &mut std::collections::HashSet<String>,
    ) -> WindowResult {
        let fail = |error: String| WindowResult {
            id: spec.id.clone(),
            ok: false,
            error: Some(error),
            snapshot: None,
            fields: vec![],
        };
        if let Err(message) = validate_id(&spec.id) {
            return fail(message);
        }
        if !seen.insert(spec.id.clone()) {
            return fail("duplicate id within this spawn request".to_string());
        }
        if self.windows.contains_key(&spec.id) {
            return fail("id is already in use".to_string());
        }
        if let Err(message) = validate_fields(&spec.fields) {
            return fail(message);
        }
        let title = self.title_for_suffix(&spec.id, title_suffix(&spec.fields));
        match host.create(&spec.id, &title, &spec.fields) {
            Ok(mut created) => {
                // `configured` holds only what the host actually took:
                // strip fields the host reports as unsupported instead of
                // caching the request as if it applied. Description is
                // local-only, never host-sent, so it never enters `configured`.
                // Focus/attention are one-shot actions, never configuration.
                strip_unsupported_from_fields(&mut created.configured, &created.unsupported);
                created.configured.description = None;
                created.configured.focus = None;
                created.configured.attention = None;
                let entry = LiveEntry {
                    host_key: created.key,
                    title: created.title.clone(),
                    backend: created.backend.clone(),
                    native_handle: created.native_handle.clone(),
                    requested: with_defaults(&spec.fields),
                    configured: created.configured,
                    observed: created.observed,
                    unsupported: created.unsupported,
                    settled: false,
                    visible_explicit: spec.fields.visible.is_some(),
                };
                // A fresh window has not yet had host feedback; it settles
                // once host notifications arrive (or a get observes matching
                // values). See `settled` semantics in the crate README.
                self.windows.insert(spec.id.clone(), entry);
                let snapshot = self.snapshot(&spec.id);
                let event = Event::Created {
                    id: spec.id.clone(),
                    snapshot: snapshot.clone(),
                };
                self.broadcast(&event);
                WindowResult {
                    id: spec.id.clone(),
                    ok: true,
                    error: None,
                    snapshot: Some(snapshot),
                    fields: created.outcomes,
                }
            }
            Err(message) => fail(message),
        }
    }

    fn set_one(&mut self, host: &mut impl Host, id: &str, patch: &Fields) -> WindowResult {
        // Borrow the host key and backend first so the host round trip does
        // not overlap a mutable borrow of the entry itself.
        let (host_key, backend, existing) = match self.windows.get(id) {
            Some(entry) => (
                entry.host_key.clone(),
                entry.backend.clone(),
                entry.requested.clone(),
            ),
            None => {
                return WindowResult {
                    id: id.to_string(),
                    ok: false,
                    error: Some("unknown window id".to_string()),
                    snapshot: None,
                    fields: vec![],
                };
            }
        };
        // Reject merged hint contradictions before touching the host.
        if let Err(message) = crate::protocol::validate_merged_hints(&existing, patch) {
            return WindowResult {
                id: id.to_string(),
                ok: false,
                error: Some(message),
                snapshot: None,
                fields: vec![],
            };
        }
        // The server owns the title: resolve the suffix to the full string
        // the host must set verbatim.
        let full_title = patch
            .title
            .as_deref()
            .map(|suffix| self.title_for_suffix(id, Some(suffix)));
        let report = host.apply(&host_key, patch, full_title.as_deref());
        let Some(entry) = self.windows.get_mut(id) else {
            return WindowResult {
                id: id.to_string(),
                ok: false,
                error: Some("unknown window id".to_string()),
                snapshot: None,
                fields: vec![],
            };
        };
        // `requested` is caller intent and may include unsupported values.
        // Focus/attention are one-shot actions: recorded nowhere, so later
        // unrelated patches can never replay them.
        merge_for_host(&mut entry.requested, patch);
        if let Some(title) = &full_title {
            entry.title = title.clone();
        }
        if patch.visible.is_some() {
            entry.visible_explicit = true;
        }
        // `configured` is only what the host actually took: accepted
        // host-sent fields, never local-only or unsupported ones.
        let accepted = accepted_config_subset(patch, &report.outcomes);
        merge_for_host(&mut entry.configured, &accepted);
        merge_observed(&mut entry.observed, &report.observed);
        // Persistent capability limits survive unrelated sets: recompute from
        // the current requested values every set. Transient refusals
        // (creation-only runtime changes, host errors, restore gates) survive
        // via the prior snapshot until their field is accepted, instead of
        // vanishing on the next unrelated patch.
        entry.unsupported = retained_unsupported(
            &entry.unsupported,
            &report.outcomes,
            &persistent_unsupported(
                &entry.requested,
                &capabilities_for_backend(&backend),
                entry.visible_explicit,
            ),
            &report.unsupported,
        );
        // Honest minimal settlement: `settled` reports geometry feedback
        // (size/position) received since the last accepted geometry set.
        // Accepted means the host took the request, never that it settled:
        // only geometry-affecting sets make a window pending, and only host
        // feedback (move/resize notifications or a reconciling get) settles
        // it. Local-only (description), cache-only
        // (decorations/resizable/min/max), creation-only, mode/visible, and
        // unsupported-only patches leave `settled` alone.
        if has_accepted_geometry(patch, &report.outcomes) {
            entry.settled = false;
        }
        let snapshot = self.snapshot(id);
        self.broadcast(&Event::State {
            id: id.to_string(),
            snapshot: snapshot.clone(),
        });
        WindowResult {
            id: id.to_string(),
            ok: true,
            error: None,
            snapshot: Some(snapshot),
            fields: report.outcomes,
        }
    }

    fn get_one(&mut self, host: &mut impl Host, id: &str) -> WindowResult {
        let host_key = match self.windows.get(id) {
            Some(entry) => entry.host_key.clone(),
            None => {
                return WindowResult {
                    id: id.to_string(),
                    ok: false,
                    error: Some("unknown window id".to_string()),
                    snapshot: None,
                    fields: vec![],
                };
            }
        };
        if let Some(observed) = host.observe(&host_key)
            && let Some(entry) = self.windows.get_mut(id)
        {
            entry.observed = observed;
            // Reconcile: a fresh readback whose geometry matches the request
            // settles a pending set. Unknown or mismatched geometry never
            // claims settled.
            if geometry_matches(&entry.requested, &entry.observed) {
                entry.settled = true;
            }
        }
        WindowResult {
            id: id.to_string(),
            ok: true,
            error: None,
            snapshot: Some(self.snapshot(id)),
            fields: vec![],
        }
    }

    fn close_one(&mut self, host: &mut impl Host, id: &str) -> WindowResult {
        let Some(entry) = self.windows.remove(id) else {
            return WindowResult {
                id: id.to_string(),
                ok: false,
                error: Some("unknown window id".to_string()),
                snapshot: None,
                fields: vec![],
            };
        };
        host.destroy(&entry.host_key);
        self.broadcast(&Event::Closed { id: id.to_string() });
        // Closing the selected windows never exits the server while other
        // windows remain, and not even when none remain: only `shutdown`
        // (or process termination) ends the run.
        WindowResult {
            id: id.to_string(),
            ok: true,
            error: None,
            snapshot: None,
            fields: vec![],
        }
    }

    fn batch_item(&mut self, host: &mut impl Host, item: &BatchItem) -> serde_json::Value {
        let op = match crate::protocol::parse_batch_item(item) {
            Ok(op) => op,
            Err(message) => return json!({ "op": item.op, "ok": false, "error": message }),
        };
        match op {
            Op::Spawn { windows } => {
                let mut seen = std::collections::HashSet::new();
                let results: Vec<WindowResult> = windows
                    .iter()
                    .map(|spec| self.spawn_one(host, spec, &mut seen))
                    .collect();
                let ok = results.iter().all(|r| r.ok);
                json!({ "op": "spawn", "ok": ok, "windows": results })
            }
            Op::Set { ids, patch } => {
                if let Err(message) = validate_fields(&patch) {
                    return json!({ "op": "set", "ok": false, "error": message });
                }
                let results: Vec<WindowResult> = ids
                    .iter()
                    .map(|id| self.set_one(host, id, &patch))
                    .collect();
                let ok = results.iter().all(|r| r.ok);
                json!({ "op": "set", "ok": ok, "windows": results })
            }
            Op::Get { ids } => {
                let results: Vec<WindowResult> =
                    ids.iter().map(|id| self.get_one(host, id)).collect();
                let ok = results.iter().all(|r| r.ok);
                json!({ "op": "get", "ok": ok, "windows": results })
            }
            Op::Close { ids } => {
                let results: Vec<WindowResult> =
                    ids.iter().map(|id| self.close_one(host, id)).collect();
                let ok = results.iter().all(|r| r.ok);
                json!({ "op": "close", "ok": ok, "windows": results })
            }
            Op::List => {
                let ids: Vec<&String> = self.windows.keys().collect();
                json!({ "op": "list", "ok": true, "ids": ids })
            }
            _ => json!({ "op": item.op, "ok": false, "error": "not permitted inside batch" }),
        }
    }

    /// Apply a host-originated notification (resize, move, close). Resize and
    /// move report geometry feedback, so they settle a pending geometry set;
    /// they never claim anything about mode or visibility.
    pub fn note_host_event(&mut self, id: &str, event: HostEvent) {
        match event {
            HostEvent::ClosedByUser => {
                if self.windows.remove(id).is_some() {
                    self.broadcast(&Event::Closed { id: id.to_string() });
                }
            }
            _ => {
                let Some(entry) = self.windows.get_mut(id) else {
                    return;
                };
                match event {
                    HostEvent::Resized {
                        size_px,
                        scale_factor,
                    } => {
                        entry.observed.size_px = Some(size_px);
                        entry.observed.scale_factor = Some(scale_factor);
                        entry.observed.logical_size_px =
                            Some(logical_size_for(size_px, scale_factor));
                        entry.settled = true;
                    }
                    HostEvent::Moved { position_outer_px } => {
                        entry.observed.position_outer_px = Some(position_outer_px);
                        entry.settled = true;
                    }
                    HostEvent::ClosedByUser => unreachable!(),
                }
                let snapshot = self.snapshot(id);
                self.broadcast(&Event::State {
                    id: id.to_string(),
                    snapshot,
                });
            }
        }
    }

    /// Apply a full fresh observation from the live backend (read after a
    /// real window notification such as resize, move, focus, or occlusion).
    /// `geometry_seen` is true when the notification itself carries geometry
    /// feedback (resize/move/scale); otherwise settlement only follows from
    /// geometry matching the request, and unknown never settles.
    pub fn note_full_observation(&mut self, id: &str, observed: Observed, geometry_seen: bool) {
        let Some(entry) = self.windows.get_mut(id) else {
            return;
        };
        entry.observed = observed;
        if geometry_seen || geometry_matches(&entry.requested, &entry.observed) {
            entry.settled = true;
        }
        let snapshot = self.snapshot(id);
        self.broadcast(&Event::State {
            id: id.to_string(),
            snapshot,
        });
    }

    pub fn note_error(&mut self, id: Option<&str>, message: String) {
        self.broadcast(&Event::Error {
            id: id.map(str::to_string),
            message,
        });
    }

    fn snapshot(&self, id: &str) -> Snapshot {
        let entry = self.windows.get(id).expect("snapshot of a known window");
        Snapshot {
            id: id.to_string(),
            pid: self.pid,
            backend: entry.backend.clone(),
            native_handle: entry.native_handle.clone(),
            title: entry.title.clone(),
            scale_factor: entry.observed.scale_factor,
            requested: entry.requested.clone(),
            configured: entry.configured.clone(),
            observed: entry.observed.clone(),
            unsupported: entry.unsupported.clone(),
            settled: entry.settled,
        }
    }

    /// Full per-window snapshot as JSON lines: a greppable header, the
    /// complete pretty-printed snapshot (every exposed field, unknown is
    /// null), and the description text. The JSON form avoids manually
    /// maintained per-field lines that silently omit new fields.
    pub fn render_lines(&self, id: &str) -> Vec<String> {
        let snap = match self.windows.get(id) {
            Some(_) => self.snapshot(id),
            None => return vec!["unknown window".to_string()],
        };
        let mut lines = vec![format!(
            "{}  backend={} pid={} settled={}",
            snap.title, snap.backend, snap.pid, snap.settled
        )];
        lines.push("snapshot:".to_string());
        let pretty = serde_json::to_string_pretty(&snap).unwrap_or_else(|_| "{}".to_string());
        lines.extend(pretty.lines().map(str::to_string));
        lines.push("description:".to_string());
        match &snap.requested.description {
            Some(text) if !text.is_empty() => lines.extend(text.lines().map(str::to_string)),
            _ => lines.push("(no description)".to_string()),
        }
        lines
    }

    fn broadcast(&mut self, event: &Event) {
        let line = crate::protocol::event_to_string(event);
        let mut dead = Vec::new();
        for (id, (tx, filter, _)) in &self.subscribers {
            if !filter.iter().any(|f| f == "all" || f == event.name()) {
                continue;
            }
            if tx.send(line.clone()).is_err() {
                dead.push(*id);
            }
        }
        for id in dead {
            self.subscribers.remove(&id);
        }
    }
}

fn subscription_marker(id: u64, filter: &[String]) -> String {
    serde_json::to_string(&json!({ "event": "subscribed", "subscription": id, "filter": filter }))
        .unwrap_or_else(|_| r#"{"event":"subscribed"}"#.to_string())
}

/// Spawn defaults for fields the caller left unset. One-shot actions
/// (`focus`, `attention`) are never stored: the protocol keeps them out of
/// `requested`/`configured` entirely, so stripping them here keeps spawn
/// intent from leaking into persistent state.
fn with_defaults(fields: &Fields) -> Fields {
    let mut full = fields.clone();
    if full.mode.is_none() {
        full.mode = Some(WindowMode::Windowed);
    }
    if full.visible.is_none() {
        full.visible = Some(true);
    }
    if full.x11_window_type.is_none() {
        full.x11_window_type = Some(X11WindowType::Normal);
    }
    full.focus = None;
    full.attention = None;
    full
}

/// Overlay set fields onto the stored configuration. `x11_window_type` and
/// `override_redirect` are creation-only for the host, and focus/attention
/// are one-shot actions: caller runtime intent for the first two is still
/// recorded in `requested`, while actions are recorded nowhere so unrelated
/// later patches never replay them. `configured` never receives any of the
/// four (see [`accepted_config_subset`]).
pub(crate) fn merge_for_host(into: &mut Fields, patch: &Fields) {
    if patch.size_px.is_some() {
        into.size_px = patch.size_px;
    }
    if patch.min_size_px.is_some() {
        into.min_size_px = patch.min_size_px;
    }
    if patch.max_size_px.is_some() {
        into.max_size_px = patch.max_size_px;
    }
    if patch.resizable.is_some() {
        into.resizable = patch.resizable;
    }
    if patch.decorations.is_some() {
        into.decorations = patch.decorations;
    }
    if patch.mode.is_some() {
        into.mode = patch.mode;
    }
    if patch.position_outer_px.is_some() {
        into.position_outer_px = patch.position_outer_px;
    }
    if patch.visible.is_some() {
        into.visible = patch.visible;
    }
    if patch.description.is_some() {
        into.description = patch.description.clone();
    }
    if patch.x11_window_type.is_some() {
        into.x11_window_type = patch.x11_window_type;
    }
    if patch.title.is_some() {
        into.title = patch.title.clone();
    }
    if patch.window_level.is_some() {
        into.window_level = patch.window_level;
    }
    if patch.cursor_visible.is_some() {
        into.cursor_visible = patch.cursor_visible;
    }
    if patch.cursor_hittest.is_some() {
        into.cursor_hittest = patch.cursor_hittest;
    }
    if patch.enabled_buttons.is_some() {
        into.enabled_buttons = patch.enabled_buttons.clone();
    }
    if patch.resize_increments_px.is_some() {
        into.resize_increments_px = patch.resize_increments_px;
    }
    if patch.theme.is_some() {
        into.theme = patch.theme;
    }
    if patch.transparent.is_some() {
        into.transparent = patch.transparent;
    }
    if patch.blur.is_some() {
        into.blur = patch.blur;
    }
    if patch.override_redirect.is_some() {
        into.override_redirect = patch.override_redirect;
    }
}

/// Caller suffix for the server-controlled title: empty means none.
fn title_suffix(fields: &Fields) -> Option<&str> {
    fields.title.as_deref().filter(|s| !s.is_empty())
}

/// Only the host-sent fields a host actually accepted. Local-only
/// (`description`), creation-only (`x11_window_type`, `override_redirect`),
/// and one-shot actions (`focus`, `attention`) never enter `configured`;
/// `requested` may still hold caller intent for the persistent ones.
fn accepted_config_subset(patch: &Fields, outcomes: &[FieldOutcome]) -> Fields {
    let accepted = |name: &str| {
        outcomes
            .iter()
            .any(|o| o.field == name && matches!(o.status, crate::protocol::FieldStatus::Accepted))
    };
    Fields {
        size_px: patch.size_px.filter(|_| accepted("size_px")),
        min_size_px: patch.min_size_px.filter(|_| accepted("min_size_px")),
        max_size_px: patch.max_size_px.filter(|_| accepted("max_size_px")),
        resizable: patch.resizable.filter(|_| accepted("resizable")),
        decorations: patch.decorations.filter(|_| accepted("decorations")),
        mode: patch.mode.filter(|_| accepted("mode")),
        position_outer_px: patch
            .position_outer_px
            .filter(|_| accepted("position_outer_px")),
        visible: patch.visible.filter(|_| accepted("visible")),
        description: None,
        x11_window_type: None,
        title: patch.title.clone().filter(|_| accepted("title")),
        window_level: patch.window_level.filter(|_| accepted("window_level")),
        focus: None,
        attention: None,
        cursor_visible: patch.cursor_visible.filter(|_| accepted("cursor_visible")),
        cursor_hittest: patch.cursor_hittest.filter(|_| accepted("cursor_hittest")),
        enabled_buttons: patch
            .enabled_buttons
            .clone()
            .filter(|_| accepted("enabled_buttons")),
        resize_increments_px: patch
            .resize_increments_px
            .filter(|_| accepted("resize_increments_px")),
        theme: patch.theme.filter(|_| accepted("theme")),
        transparent: patch.transparent.filter(|_| accepted("transparent")),
        blur: patch.blur.filter(|_| accepted("blur")),
        override_redirect: None,
    }
}

/// Whether the patch carries accepted geometry (size/position). Only
/// geometry makes a set pending; cache-only, local-only, mode/visible,
/// creation-only, and unsupported-only patches leave `settled` alone.
fn has_accepted_geometry(patch: &Fields, outcomes: &[FieldOutcome]) -> bool {
    let accepted = |name: &str| {
        outcomes
            .iter()
            .any(|o| o.field == name && matches!(o.status, crate::protocol::FieldStatus::Accepted))
    };
    (patch.size_px.is_some() && accepted("size_px"))
        || (patch.position_outer_px.is_some() && accepted("position_outer_px"))
}

/// Whether the observed geometry matches the request. Unknown (null
/// observed where requested is `Some`) or mismatched geometry never claims
/// settled; mode and visibility are out of scope for settlement.
fn geometry_matches(requested: &Fields, observed: &Observed) -> bool {
    if requested.size_px.is_some() && observed.size_px != requested.size_px {
        return false;
    }
    if requested.position_outer_px.is_some()
        && observed.position_outer_px != requested.position_outer_px
    {
        return false;
    }
    true
}

/// Merge a possibly-partial apply readback into the stored observation:
/// only `Some` fields overwrite, so unrelated sets never clear known values.
fn merge_observed(into: &mut Observed, update: &Observed) {
    if update.size_px.is_some() {
        into.size_px = update.size_px;
    }
    if update.position_outer_px.is_some() {
        into.position_outer_px = update.position_outer_px;
    }
    if update.mode.is_some() {
        into.mode = update.mode;
    }
    if update.visible.is_some() {
        into.visible = update.visible;
    }
    if update.scale_factor.is_some() {
        into.scale_factor = update.scale_factor;
    }
    if update.logical_size_px.is_some() {
        into.logical_size_px = update.logical_size_px;
    }
    if update.focused.is_some() {
        into.focused = update.focused;
    }
}

/// Persistent capability limits derived from current requested values.
/// These recompute every set; transient per-patch refusals persist via
/// [`retained_unsupported`] until their field is accepted.
fn persistent_unsupported(
    requested: &Fields,
    caps: &Capabilities,
    visible_explicit: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    if requested.position_outer_px.is_some() && !caps.position {
        out.push("position_outer_px".to_string());
    }
    if requested.mode == Some(WindowMode::ExclusiveFullscreen) && !caps.exclusive_fullscreen {
        out.push("mode:exclusive_fullscreen".to_string());
    }
    if requested
        .x11_window_type
        .is_some_and(|t| t != X11WindowType::Normal)
        && !caps.x11_window_type
    {
        out.push("x11_window_type".to_string());
    }
    if visible_explicit && requested.visible.is_some() && !caps.visible {
        out.push("visible".to_string());
    }
    if requested
        .window_level
        .is_some_and(|l| l != WindowLevel::Normal)
        && !caps.window_level
    {
        out.push("window_level".to_string());
    }
    if requested.enabled_buttons.is_some() && !caps.enabled_buttons {
        out.push("enabled_buttons".to_string());
    }
    if requested.resize_increments_px.is_some() && !caps.resize_increments {
        out.push("resize_increments_px".to_string());
    }
    if requested.blur == Some(true) && !caps.blur {
        out.push("blur".to_string());
    }
    if requested.override_redirect == Some(true) && !caps.override_redirect {
        out.push("override_redirect".to_string());
    }
    out
}

/// Retained `unsupported` tags across sets. Static capability limits
/// recompute from `requested` every set and transient refusals arrive via
/// `transient`; prior tags survive an unrelated patch until their field
/// receives a newly accepted outcome, so a refused intent is not forgotten
/// just because the next patch touches something else. Tags name their
/// field before an optional `:` detail suffix (`mode:restore` is the `mode`
/// field). One-shot action tags (`focus`, `attention*`) are never retained:
/// actions are not configuration and must not linger.
fn retained_unsupported(
    prior: &[String],
    outcomes: &[FieldOutcome],
    persistent: &[String],
    transient: &[String],
) -> Vec<String> {
    let accepted = |field: &str| {
        outcomes
            .iter()
            .any(|o| o.field == field && matches!(o.status, crate::protocol::FieldStatus::Accepted))
    };
    let mut out = Vec::new();
    for tag in prior {
        let field = tag.split(':').next().unwrap_or(tag);
        if field == "focus" || field == "attention" {
            continue;
        }
        if accepted(field) {
            continue;
        }
        if !out.contains(tag) {
            out.push(tag.clone());
        }
    }
    for tag in persistent.iter().chain(transient.iter()) {
        if !out.contains(tag) {
            out.push(tag.clone());
        }
    }
    out
}

/// Strip unsupported fields from a `configured` cache so it never claims the
/// host took a value the host reported as unsupported.
fn strip_unsupported_from_fields(fields: &mut Fields, unsupported: &[String]) {
    if unsupported.contains(&"position_outer_px".to_string()) {
        fields.position_outer_px = None;
    }
    if unsupported.contains(&"visible".to_string()) {
        fields.visible = None;
    }
    if unsupported.contains(&"window_level".to_string()) {
        fields.window_level = None;
    }
    if unsupported.contains(&"enabled_buttons".to_string()) {
        fields.enabled_buttons = None;
    }
    if unsupported.contains(&"resize_increments_px".to_string()) {
        fields.resize_increments_px = None;
    }
    if unsupported.contains(&"blur".to_string()) {
        fields.blur = None;
    }
    if unsupported.contains(&"override_redirect".to_string()) {
        fields.override_redirect = None;
    }
    if unsupported.contains(&"cursor_hittest".to_string()) {
        fields.cursor_hittest = None;
    }
    if unsupported
        .iter()
        .any(|t| t == "x11_window_type" || t.starts_with("mode"))
    {
        // Covers "mode:exclusive_fullscreen" and "mode:restore": the mode
        // request did not apply, so the cache must not keep it.
        if unsupported.iter().any(|t| t.starts_with("mode")) {
            fields.mode = None;
        }
        if unsupported.contains(&"x11_window_type".to_string()) {
            fields.x11_window_type = None;
        }
    }
    if unsupported.contains(&"host-unreachable".to_string()) {
        *fields = Fields::default();
    }
}

/// Offline test double: canned backend, capabilities and observations, no
/// windowing system involved.
#[cfg(test)]
pub mod test_host {
    use super::*;

    pub struct FakeHost {
        pub backend: String,
        pub fail_create: bool,
        /// When true, `cursor_hittest` fails like a host without shape/input
        /// support, so tests can exercise transient runtime-error retention.
        pub fail_hittest: bool,
        /// Ids the fake pretends to hold.
        pub live: std::collections::HashSet<String>,
        pub applied: Vec<(String, Fields)>,
        state: std::collections::HashMap<String, Observed>,
    }

    impl FakeHost {
        pub fn x11() -> Self {
            Self::for_backend("x11")
        }

        pub fn wayland() -> Self {
            Self::for_backend("wayland")
        }

        pub fn for_backend(backend: &str) -> Self {
            Self {
                backend: backend.to_string(),
                fail_create: false,
                fail_hittest: false,
                live: Default::default(),
                applied: vec![],
                state: Default::default(),
            }
        }
    }

    /// Shared per-field honesty for the fake: one outcome per requested
    /// field, accepted unless the backend cannot honor it. `runtime`
    /// distinguishes spawn (creation hints apply) from set (creation-only
    /// fields refuse, transparency is fixed on X11). `minimized` drives the
    /// same restore gate the live backend uses. `hittest_error` simulates a
    /// host that rejects cursor hit-test changes at runtime.
    fn fake_outcomes(
        backend: &str,
        fields: &Fields,
        runtime: bool,
        minimized: bool,
        hittest_error: bool,
    ) -> (Vec<FieldOutcome>, Vec<String>) {
        let caps = capabilities_for_backend(backend);
        let mut outcomes = Vec::new();
        let mut unsupported = Vec::new();
        if fields.size_px.is_some() {
            outcomes.push(FieldOutcome::accepted("size_px"));
        }
        if fields.min_size_px.is_some() {
            outcomes.push(FieldOutcome::accepted("min_size_px"));
        }
        if fields.max_size_px.is_some() {
            outcomes.push(FieldOutcome::accepted("max_size_px"));
        }
        if fields.resizable.is_some() {
            outcomes.push(FieldOutcome::accepted("resizable"));
        }
        if fields.decorations.is_some() {
            outcomes.push(FieldOutcome::accepted("decorations"));
        }
        if let Some(mode) = fields.mode {
            if runtime && restore_blocked(&caps, minimized, Some(mode)) {
                outcomes.push(FieldOutcome::unsupported(
                    "mode",
                    "restoring a minimized window is not supported on this backend",
                ));
                unsupported.push("mode:restore".to_string());
            } else if matches!(mode, WindowMode::ExclusiveFullscreen) && !caps.exclusive_fullscreen
            {
                outcomes.push(FieldOutcome::unsupported(
                    "mode",
                    "exclusive fullscreen is not supported on this backend",
                ));
                unsupported.push("mode:exclusive_fullscreen".to_string());
            } else {
                outcomes.push(FieldOutcome::accepted("mode"));
            }
        }
        if fields.position_outer_px.is_some() {
            if caps.position {
                outcomes.push(FieldOutcome::accepted("position_outer_px"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "position_outer_px",
                    "global position is not supported on Wayland",
                ));
                unsupported.push("position_outer_px".to_string());
            }
        }
        if fields.visible.is_some() {
            if caps.visible {
                outcomes.push(FieldOutcome::accepted("visible"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "visible",
                    "showing/hiding windows is not supported on Wayland",
                ));
                unsupported.push("visible".to_string());
            }
        }
        if fields.description.is_some() {
            outcomes.push(FieldOutcome::accepted("description"));
        }
        if let Some(kind) = fields.x11_window_type {
            if !runtime && (caps.x11_window_type || kind == X11WindowType::Normal) {
                outcomes.push(FieldOutcome::accepted("x11_window_type"));
            } else if !runtime {
                outcomes.push(FieldOutcome::unsupported(
                    "x11_window_type",
                    "creation-only window type is only supported on X11",
                ));
                unsupported.push("x11_window_type".to_string());
            } else {
                // Creation-only: a runtime change is reported, never applied.
                outcomes.push(FieldOutcome::unsupported(
                    "x11_window_type",
                    format!(
                        "creation-only (window is {kind:?}); recreate via close+spawn to change"
                    ),
                ));
                unsupported.push("x11_window_type".to_string());
            }
        }
        if fields.title.is_some() {
            outcomes.push(FieldOutcome::accepted("title"));
        }
        if let Some(level) = fields.window_level {
            if caps.window_level || level == WindowLevel::Normal {
                outcomes.push(FieldOutcome::accepted("window_level"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "window_level",
                    "stacking level is not supported on Wayland",
                ));
                unsupported.push("window_level".to_string());
            }
        }
        if let Some(focus) = fields.focus {
            if focus && !caps.focus {
                outcomes.push(FieldOutcome::unsupported(
                    "focus",
                    "focus requests are not supported on Wayland",
                ));
                unsupported.push("focus".to_string());
            } else {
                outcomes.push(FieldOutcome::accepted("focus"));
            }
        }
        if let Some(attention) = fields.attention {
            if attention == Attention::Clear && backend == "wayland" {
                outcomes.push(FieldOutcome::unsupported(
                    "attention",
                    "clearing attention has no effect on Wayland: urgency is compositor-owned and setting it requires xdg_activation_v1",
                ));
                unsupported.push("attention:clear".to_string());
            } else {
                outcomes.push(FieldOutcome::accepted("attention"));
            }
        }
        if fields.cursor_visible.is_some() {
            outcomes.push(FieldOutcome::accepted("cursor_visible"));
        }
        if fields.cursor_hittest.is_some() {
            if runtime && hittest_error {
                outcomes.push(FieldOutcome::unsupported(
                    "cursor_hittest",
                    "host refused cursor hit-test change: shape extension unavailable",
                ));
                unsupported.push("cursor_hittest".to_string());
            } else {
                outcomes.push(FieldOutcome::accepted("cursor_hittest"));
            }
        }
        if fields.enabled_buttons.is_some() {
            if caps.enabled_buttons {
                outcomes.push(FieldOutcome::accepted("enabled_buttons"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "enabled_buttons",
                    "window button hints are not implemented on X11/Wayland",
                ));
                unsupported.push("enabled_buttons".to_string());
            }
        }
        if fields.resize_increments_px.is_some() {
            if caps.resize_increments {
                outcomes.push(FieldOutcome::accepted("resize_increments_px"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "resize_increments_px",
                    "resize increments have no protocol support on Wayland",
                ));
                unsupported.push("resize_increments_px".to_string());
            }
        }
        if fields.theme.is_some() {
            outcomes.push(FieldOutcome::accepted("theme"));
        }
        if fields.transparent.is_some() {
            if !runtime || caps.transparent_runtime {
                outcomes.push(FieldOutcome::accepted("transparent"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "transparent",
                    "transparency is creation-only on X11: close and spawn again to change it",
                ));
                unsupported.push("transparent".to_string());
            }
        }
        if let Some(blur) = fields.blur {
            if caps.blur || !blur {
                outcomes.push(FieldOutcome::accepted("blur"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "blur",
                    "blur is not supported on this backend",
                ));
                unsupported.push("blur".to_string());
            }
        }
        if let Some(redirect) = fields.override_redirect {
            if !runtime && (caps.override_redirect || !redirect) {
                outcomes.push(FieldOutcome::accepted("override_redirect"));
            } else if !runtime {
                outcomes.push(FieldOutcome::unsupported(
                    "override_redirect",
                    "override-redirect is X11-only",
                ));
                unsupported.push("override_redirect".to_string());
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "override_redirect",
                    format!(
                        "creation-only (requested {redirect:?}); close and spawn again to change it"
                    ),
                ));
                unsupported.push("override_redirect".to_string());
            }
        }
        (outcomes, unsupported)
    }

    impl Host for FakeHost {
        fn create(
            &mut self,
            id: &str,
            title: &str,
            requested: &Fields,
        ) -> Result<CreatedWindow, String> {
            if self.fail_create {
                return Err("fake host refuses creation".to_string());
            }
            let caps = capabilities_for_backend(&self.backend);
            self.live.insert(id.to_string());
            let (outcomes, mut unsupported) =
                fake_outcomes(&self.backend, requested, false, false, false);
            // Wayland offers no visibility readback: unknown, never
            // an echo of the request. Focus starts unfocused; an
            // accepted spawn-time focus request focuses immediately.
            let focused = if requested.focus == Some(true) && caps.focus {
                Some(true)
            } else {
                Some(false)
            };
            let observed = Observed {
                size_px: requested.size_px.or(Some([640, 480])),
                position_outer_px: if caps.position {
                    requested.position_outer_px.or(Some([0, 0]))
                } else {
                    None
                },
                mode: Some(WindowMode::Windowed),
                visible: if caps.visible {
                    requested.visible.or(Some(true))
                } else {
                    None
                },
                scale_factor: Some(1.0),
                logical_size_px: None,
                focused,
            };
            self.state.insert(id.to_string(), observed.clone());
            // `configured` keeps only what the host took; unsupported fields
            // are stripped by the server as well, but the fake does not cache
            // requests it refused. Actions and local-only text never enter.
            let mut configured = requested.clone();
            strip_unsupported_from_fields(&mut configured, &unsupported);
            configured.description = None;
            configured.focus = None;
            configured.attention = None;
            // Persistent tags the transient report cannot know: creation
            // values on backends that can never honor them.
            for tag in persistent_unsupported(requested, &caps, requested.visible.is_some()) {
                if !unsupported.contains(&tag) {
                    unsupported.push(tag);
                }
            }
            Ok(CreatedWindow {
                key: id.to_string(),
                backend: self.backend.clone(),
                native_handle: format!("fake:{id}"),
                title: title.to_string(),
                observed,
                configured,
                unsupported,
                outcomes,
            })
        }

        fn apply(&mut self, key: &str, patch: &Fields, _full_title: Option<&str>) -> ApplyReport {
            self.applied.push((key.to_string(), patch.clone()));
            let caps = capabilities_for_backend(&self.backend);
            let minimized = self
                .state
                .get(key)
                .is_some_and(|o| o.mode == Some(WindowMode::Minimized));
            let (outcomes, unsupported) =
                fake_outcomes(&self.backend, patch, true, minimized, self.fail_hittest);
            // Honest fake readback: accepted values update the stored full
            // observation; unsupported requests never echo as observed.
            // Unsupported mode stays at the previous mode (or unknown only if
            // the window never had one), never the refused request.
            let mode_refused = patch.mode.is_some()
                && outcomes.iter().any(|o| {
                    o.field == "mode"
                        && matches!(o.status, crate::protocol::FieldStatus::Unsupported)
                });
            let position_blocked = patch.position_outer_px.is_some() && !caps.position;
            let visible_blocked = patch.visible.is_some() && !caps.visible;
            if let Some(entry) = self.state.get_mut(key) {
                if let Some(size) = patch.size_px {
                    entry.size_px = Some(size);
                }
                if patch.position_outer_px.is_some() && !position_blocked {
                    entry.position_outer_px = patch.position_outer_px;
                }
                if let Some(mode) = patch.mode
                    && !mode_refused
                {
                    entry.mode = Some(mode);
                }
                if patch.visible.is_some() && !visible_blocked {
                    entry.visible = patch.visible;
                }
                if patch.focus == Some(true) && caps.focus {
                    entry.focused = Some(true);
                }
            }
            let observed = self.state.get(key).cloned().unwrap_or_default();
            // `configured` is only accepted host-sent fields; the server
            // re-filters as well, but the fake never caches a refusal.
            let configured = accepted_config_subset(patch, &outcomes);
            ApplyReport {
                outcomes,
                observed,
                configured,
                unsupported,
            }
        }

        fn observe(&mut self, key: &str) -> Option<Observed> {
            if !self.live.contains(key) {
                return None;
            }
            let caps = capabilities_for_backend(&self.backend);
            Some(self.state.get(key).cloned().unwrap_or(Observed {
                size_px: Some([640, 480]),
                position_outer_px: None,
                mode: Some(WindowMode::Windowed),
                visible: caps.visible.then_some(true),
                scale_factor: Some(1.0),
                logical_size_px: None,
                focused: Some(false),
            }))
        }

        fn destroy(&mut self, key: &str) {
            self.live.remove(key);
            self.state.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_host::FakeHost;
    use super::*;
    use crate::protocol::SpawnSpec;

    fn server() -> (Server, FakeHost) {
        (
            Server::new(4242, "run1".to_string(), "secret".to_string()),
            FakeHost::x11(),
        )
    }

    fn line(op: &str) -> String {
        format!(r#"{{"v":1,"request_id":"r","nonce":"secret","op":{op}}}"#)
    }

    fn reply_of(out: &str) -> serde_json::Value {
        serde_json::from_str(out).expect("reply must be JSON")
    }

    #[test]
    fn bad_nonce_is_rejected_without_touching_state() {
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"wrong","op":"list"}"#,
            1,
            None,
        );
        let reply = reply_of(&out);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], "bad nonce");
    }

    #[test]
    fn spawn_many_reports_per_window_errors() {
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"},{"id":"bad id!"},{"id":"a"}]}"#,
            1,
            None,
        );
        let reply = reply_of(&out);
        assert_eq!(reply["ok"], false);
        let windows = reply["result"]["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 3);
        assert!(windows[0]["ok"].as_bool().unwrap());
        assert!(
            windows[1]["error"]
                .as_str()
                .unwrap()
                .contains("id must match")
        );
        assert!(windows[2]["error"].as_str().unwrap().contains("duplicate"));
        // The valid window still exists: errors are per-window, not atomic.
        assert_eq!(server.window_count(), 1);
    }

    #[test]
    fn spawn_duplicate_against_existing_is_per_window_error() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(&mut host, r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"},{"id":"b"}]}"#, 1, None);
        let reply = reply_of(&out);
        let windows = reply["result"]["windows"].as_array().unwrap();
        assert!(
            windows[0]["error"]
                .as_str()
                .unwrap()
                .contains("already in use")
        );
        assert!(windows[1]["ok"].as_bool().unwrap());
    }

    #[test]
    fn unknown_ids_error_per_id() {
        let (mut server, mut host) = server();
        for op in ["get", "set", "close"] {
            let body = match op {
                "set" => r#","patch":{"decorations":false}"#,
                _ => "",
            };
            let req = format!(
                r#"{{"v":1,"request_id":"r","nonce":"secret","op":"{op}","ids":["ghost"]{body}}}"#
            );
            let reply = reply_of(&server.handle_line(&mut host, &req, 1, None));
            assert_eq!(reply["ok"], false, "op {op}");
            assert!(
                reply["result"]["windows"][0]["error"]
                    .as_str()
                    .unwrap()
                    .contains("unknown window id")
            );
        }
    }

    #[test]
    fn set_reports_accepted_not_settled() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"size_px":[800,600]}}"#,
            1,
            None,
        );
        let reply = reply_of(&out);
        let entry = &reply["result"]["windows"][0];
        assert!(entry["ok"].as_bool().unwrap());
        assert_eq!(entry["snapshot"]["requested"]["size_px"], json!([800, 600]));
        // Accepted by the host dispatch, not yet confirmed by host feedback.
        assert_eq!(entry["snapshot"]["settled"], false);
    }

    #[test]
    fn runtime_x11_type_change_is_unsupported_not_applied() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"x11_window_type":"dialog"}}"#,
            1,
            None,
        );
        let reply = reply_of(&out);
        let entry = &reply["result"]["windows"][0];
        assert!(entry["ok"].as_bool().unwrap());
        let field = &entry["fields"][0];
        assert_eq!(field["field"], "x11_window_type");
        assert_eq!(field["status"], "unsupported");
    }

    #[test]
    fn wayland_position_is_unsupported_not_faked() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"position_outer_px":[10,20]}}"#,
            1,
            None,
        );
        let reply = reply_of(&out);
        let entry = &reply["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "unsupported");
        assert_eq!(
            entry["snapshot"]["observed"]["position_outer_px"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn batch_runs_in_order_and_is_not_atomic() {
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"batch","items":[{"op":"spawn","windows":[{"id":"a"}]},{"op":"set","ids":["a","ghost"],"patch":{"decorations":false}},{"op":"close","ids":["a"]}]}"#,
            1,
            None,
        );
        let reply = reply_of(&out);
        assert_eq!(reply["ok"], false);
        let items = reply["result"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert!(items[0]["ok"].as_bool().unwrap());
        // The set ran after the spawn and reported the unknown id per entry.
        assert_eq!(items[1]["windows"][0]["fields"][0]["field"], "decorations");
        assert!(
            items[1]["windows"][1]["error"]
                .as_str()
                .unwrap()
                .contains("unknown")
        );
        assert!(items[2]["ok"].as_bool().unwrap());
        assert_eq!(server.window_count(), 0);
    }

    #[test]
    fn close_selected_keeps_server_and_others_alive() {
        let (mut server, mut host) = server();
        server.handle_line(&mut host, r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"},{"id":"b"}]}"#, 1, None);
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"close","ids":["a"]}"#,
            1,
            None,
        );
        assert_eq!(server.window_count(), 1);
        assert!(!server.is_shutting_down());
        let out = server.handle_line(&mut host, &line(r#""list""#), 1, None);
        assert_eq!(reply_of(&out)["result"]["ids"], json!(["b"]));
    }

    #[test]
    fn shutdown_sets_flag_but_close_all_does_not() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"close","ids":["a"]}"#,
            1,
            None,
        );
        assert!(!server.is_shutting_down());
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"shutdown"}"#,
            1,
            None,
        );
        assert_eq!(reply_of(&out)["result"]["shutting_down"], true);
        assert!(server.is_shutting_down());
    }

    #[test]
    fn host_events_update_observed_and_settle() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.note_host_event(
            "a",
            HostEvent::Resized {
                size_px: [800, 600],
                scale_factor: 2.0,
            },
        );
        let text = server.render_lines("a").join("\n");
        // Host-confirmed geometry settles the fresh window and is rendered.
        assert!(
            text.contains("settled=true"),
            "expected settled host state:\n{text}"
        );
        assert!(
            text.contains("800") && text.contains("600"),
            "expected host-confirmed size:\n{text}"
        );
        // The resize also recomputes the logical size (800x600 at scale 2).
        assert!(
            text.contains("400") && text.contains("300"),
            "expected recomputed logical size:\n{text}"
        );
        assert!(text.contains("OmniTilerTestWindow:a:run1"));
    }

    #[test]
    fn snapshot_rendering_shows_every_field() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","description":"hello","decorations":false,"mode":"maximized"}]}"#,
            1,
            None,
        );
        let text = server.render_lines("a").join("\n");
        for needle in [
            "requested",
            "configured",
            "observed",
            "unsupported",
            "logical_size_px",
            "hello",
            "backend=x11",
            "settled=",
        ] {
            assert!(
                text.contains(needle),
                "rendering must show {needle}:\n{text}"
            );
        }
    }

    #[test]
    fn user_closed_windows_publish_closure() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.note_host_event("a", HostEvent::ClosedByUser);
        assert_eq!(server.window_count(), 0);
    }

    #[test]
    fn spawn_spec_validation_rejects_bad_geometry() {
        let spec = SpawnSpec {
            id: "a".to_string(),
            fields: crate::protocol::Fields {
                size_px: Some([0, 10]),
                ..Default::default()
            },
        };
        let (mut server, mut host) = server();
        let mut seen = std::collections::HashSet::new();
        let result = server.spawn_one(&mut host, &spec, &mut seen);
        assert!(!result.ok);
        assert!(result.error.unwrap().contains("positive"));
    }

    #[test]
    fn subscribe_filter_is_honored() {
        let (mut server, mut host) = server();
        let (tx, rx) = std::sync::mpsc::channel();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"subscribe","events":["closed"]}"#,
            1,
            Some(&tx),
        );
        assert_eq!(reply_of(&out)["result"]["subscribed"], json!(["closed"]));
        // Drain the subscription marker.
        let _ = rx.recv().unwrap();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            Some(&tx),
        );
        // Created/state events do not match the "closed" filter.
        assert!(rx.try_recv().is_err());
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"close","ids":["a"]}"#,
            1,
            Some(&tx),
        );
        let event: serde_json::Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
        assert_eq!(event["event"], "closed");
    }

    #[test]
    fn subscribe_rejects_unknown_filters() {
        let (mut server, mut host) = server();
        let (tx, _rx) = std::sync::mpsc::channel();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"subscribe","events":["bogus"]}"#,
            1,
            Some(&tx),
        );
        let reply = reply_of(&out);
        assert_eq!(reply["ok"], false);
        assert!(
            reply["error"]
                .as_str()
                .unwrap()
                .contains("unknown event filter")
        );
    }

    #[test]
    fn batch_preserves_host_application_order() {
        let (mut server, mut host) = server();
        server.handle_line(&mut host, r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"},{"id":"b"}]}"#, 1, None);
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"batch","items":[{"op":"set","ids":["a"],"patch":{"size_px":[800,600]}},{"op":"set","ids":["b"],"patch":{"size_px":[1024,768]}},{"op":"close","ids":["a"]}]}"#,
            1,
            None,
        );
        let applied: Vec<(String, Option<[u32; 2]>)> = host
            .applied
            .iter()
            .map(|(id, patch)| (id.clone(), patch.size_px))
            .collect();
        assert_eq!(
            applied,
            vec![
                ("a".to_string(), Some([800, 600])),
                ("b".to_string(), Some([1024, 768]))
            ]
        );
        assert_eq!(server.window_count(), 1);
    }

    #[test]
    fn unsubscribe_suppresses_close_event() {
        let (mut server, mut host) = server();
        let (tx, rx) = std::sync::mpsc::channel();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"subscribe"}"#,
            7,
            Some(&tx),
        );
        let _ = rx.recv().unwrap();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"unsubscribe"}"#,
            7,
            Some(&tx),
        );
        assert_eq!(reply_of(&out)["ok"], true);
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        // Drain the created broadcast consumed by no one on conn 7: the
        // unsubscribed queue must stay empty.
        assert!(rx.try_recv().is_err());
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"close","ids":["a"]}"#,
            1,
            None,
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn repeat_subscribe_does_not_duplicate() {
        let (mut server, mut host) = server();
        let (tx, rx) = std::sync::mpsc::channel();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"subscribe"}"#,
            9,
            Some(&tx),
        );
        let _ = rx.recv().unwrap();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"subscribe"}"#,
            9,
            Some(&tx),
        );
        let _ = rx.recv().unwrap();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        // Exactly one created event: the repeat replaced, not duplicated.
        let first = rx.recv().unwrap();
        assert!(rx.try_recv().is_err(), "duplicate broadcast: {first}");
    }

    #[test]
    fn disconnect_forgets_subscription() {
        let (mut server, mut host) = server();
        let (tx, rx) = std::sync::mpsc::channel();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"subscribe"}"#,
            11,
            Some(&tx),
        );
        let _ = rx.recv().unwrap();
        drop(tx);
        server.remove_connection(11);
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn subscribe_rejects_empty_filter() {
        let (mut server, mut host) = server();
        let (tx, _rx) = std::sync::mpsc::channel();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"subscribe","events":[]}"#,
            1,
            Some(&tx),
        );
        assert_eq!(reply_of(&out)["ok"], false);
    }

    #[test]
    fn mixed_unsupported_mode_still_applies_geometry() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"mode":"exclusive_fullscreen","size_px":[800,600],"decorations":false}}"#,
            1,
            None,
        );
        let reply = reply_of(&out);
        let entry = &reply["result"]["windows"][0];
        assert!(entry["ok"].as_bool().unwrap());
        // Mode refused, geometry and decorations accepted.
        let statuses: Vec<(String, String)> = entry["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["field"].as_str().unwrap().to_string(),
                    f["status"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert!(statuses.contains(&("mode".to_string(), "unsupported".to_string())));
        assert!(statuses.contains(&("size_px".to_string(), "accepted".to_string())));
        assert!(statuses.contains(&("decorations".to_string(), "accepted".to_string())));
        // Requested keeps the refused mode as intent; configured does not.
        assert_eq!(
            entry["snapshot"]["requested"]["mode"],
            serde_json::json!("exclusive_fullscreen")
        );
        assert!(
            entry["snapshot"]["configured"]["mode"].is_null()
                || entry["snapshot"]["configured"].get("mode").is_none()
        );
        assert_eq!(
            entry["snapshot"]["requested"]["size_px"],
            serde_json::json!([800, 600])
        );
        assert_eq!(
            entry["snapshot"]["configured"]["size_px"],
            serde_json::json!([800, 600])
        );
    }

    #[test]
    fn unsupported_exclusive_is_not_fabricated_as_observed() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"mode":"exclusive_fullscreen"}}"#,
            1,
            None,
        );
        let observed = &reply_of(&out)["result"]["windows"][0]["snapshot"]["observed"]["mode"];
        assert!(
            observed.is_null() || observed.as_str() != Some("exclusive_fullscreen"),
            "refused fullscreen must never echo as observed: {observed}"
        );
    }

    #[test]
    fn description_only_does_not_make_pending() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.note_host_event(
            "a",
            HostEvent::Resized {
                size_px: [640, 480],
                scale_factor: 1.0,
            },
        );
        let settled_before = server.snapshot("a").settled;
        assert!(settled_before);
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"description":"just a note"}}"#,
            1,
            None,
        );
        assert!(server.snapshot("a").settled);
    }

    #[test]
    fn unsupported_only_does_not_make_pending() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.note_host_event(
            "a",
            HostEvent::Resized {
                size_px: [640, 480],
                scale_factor: 1.0,
            },
        );
        assert!(server.snapshot("a").settled);
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"position_outer_px":[10,20]}}"#,
            1,
            None,
        );
        assert!(server.snapshot("a").settled);
    }

    #[test]
    fn get_reconciles_pending_geometry() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"size_px":[800,600]}}"#,
            1,
            None,
        );
        assert!(!server.snapshot("a").settled);
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"get","ids":["a"]}"#,
            1,
            None,
        );
        // The fake tracks the accepted size, so the readback knows every
        // requested observable and settles.
        assert!(server.snapshot("a").settled);
    }

    #[test]
    fn wayland_position_survives_unrelated_set() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","position_outer_px":[10,20]}]}"#,
            1,
            None,
        );
        assert!(
            server
                .snapshot("a")
                .unsupported
                .contains(&"position_outer_px".to_string())
        );
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        assert!(
            server
                .snapshot("a")
                .unsupported
                .contains(&"position_outer_px".to_string()),
            "persistent Wayland limit must survive unrelated sets"
        );
    }

    #[test]
    fn restore_gate_blocks_wayland_but_not_x11() {
        // Same shared gate, both backends: minimize, then request windowed
        // plus an unrelated size in the same patch.
        for (mut server, mut host, blocked) in [
            (
                Server::new(4242, "run1".to_string(), "secret".to_string()),
                FakeHost::wayland(),
                true,
            ),
            (
                Server::new(4242, "run1".to_string(), "secret".to_string()),
                FakeHost::x11(),
                false,
            ),
        ] {
            server.handle_line(
                &mut host,
                r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
                1,
                None,
            );
            server.handle_line(
                &mut host,
                r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"mode":"minimized"}}"#,
                1,
                None,
            );
            let out = server.handle_line(
                &mut host,
                r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"mode":"windowed","size_px":[800,600]}}"#,
                1,
                None,
            );
            let reply = reply_of(&out);
            let entry = &reply["result"]["windows"][0];
            let statuses: Vec<(String, String)> = entry["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| {
                    (
                        f["field"].as_str().unwrap().to_string(),
                        f["status"].as_str().unwrap().to_string(),
                    )
                })
                .collect();
            if blocked {
                assert!(
                    statuses.contains(&("mode".to_string(), "unsupported".to_string())),
                    "Wayland restore must be unsupported: {statuses:?}"
                );
                assert_eq!(entry["snapshot"]["observed"]["mode"], json!("minimized"));
            } else {
                assert!(
                    statuses.contains(&("mode".to_string(), "accepted".to_string())),
                    "X11 restore must be accepted: {statuses:?}"
                );
                assert_eq!(entry["snapshot"]["observed"]["mode"], json!("windowed"));
            }
            // The unrelated geometry still applies on both backends.
            assert!(
                statuses.contains(&("size_px".to_string(), "accepted".to_string())),
                "{statuses:?}"
            );
            assert_eq!(entry["snapshot"]["requested"]["size_px"], json!([800, 600]));
        }
    }

    #[test]
    fn runtime_x11_type_records_requested_intent() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"x11_window_type":"dialog"}}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        // Intent is recorded; the cache keeps the creation value.
        assert_eq!(snap["requested"]["x11_window_type"], json!("dialog"));
        assert!(
            snap["configured"].get("x11_window_type").is_none()
                || snap["configured"]["x11_window_type"].is_null()
        );
    }

    #[test]
    fn merged_min_over_existing_max_rejected_before_host() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","max_size_px":[640,480]}]}"#,
            1,
            None,
        );
        let applied_before = host.applied.len();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"min_size_px":[800,600]}}"#,
            1,
            None,
        );
        let reply = reply_of(&out);
        assert_eq!(reply["ok"], false);
        assert!(
            reply["result"]["windows"][0]["error"]
                .as_str()
                .unwrap()
                .contains("min_size_px must not exceed max_size_px")
        );
        assert_eq!(
            host.applied.len(),
            applied_before,
            "rejected patch must never reach the host"
        );
    }

    #[test]
    fn mode_only_set_leaves_settled_alone() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.note_host_event(
            "a",
            HostEvent::Resized {
                size_px: [640, 480],
                scale_factor: 1.0,
            },
        );
        assert!(server.snapshot("a").settled);
        // Mode/visible are out of scope for geometry settlement.
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"mode":"maximized","visible":false}}"#,
            1,
            None,
        );
        assert!(server.snapshot("a").settled);
    }

    #[test]
    fn wayland_visible_is_unsupported_not_cached() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","visible":false}]}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert!(
            snap["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("visible"))
        );
        assert_eq!(snap["observed"]["visible"], serde_json::Value::Null);
        assert!(
            snap["configured"].get("visible").is_none() || snap["configured"]["visible"].is_null()
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"visible":true}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "unsupported");
        // A visible-only patch never makes a window pending: still unsettled
        // from spawn, not newly pending.
        assert!(!server.snapshot("a").settled);
    }

    #[test]
    fn description_spawn_never_cached_as_configured() {
        for backend in ["x11", "wayland"] {
            let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
            let mut host = if backend == "x11" {
                FakeHost::x11()
            } else {
                FakeHost::wayland()
            };
            let out = server.handle_line(
                &mut host,
                r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","description":"hello"}]}"#,
                1,
                None,
            );
            let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
            assert_eq!(
                snap["requested"]["description"],
                json!("hello"),
                "{backend}"
            );
            assert!(
                snap["configured"].get("description").is_none()
                    || snap["configured"]["description"].is_null(),
                "spawn description must not enter configured on {backend}: {}",
                snap["configured"]
            );
            let out = server.handle_line(
                &mut host,
                r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"description":"updated"}}"#,
                1,
                None,
            );
            let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
            assert_eq!(
                snap["requested"]["description"],
                json!("updated"),
                "{backend}"
            );
            assert!(
                snap["configured"].get("description").is_none()
                    || snap["configured"]["description"].is_null(),
                "stale configured.description must not survive desc set on {backend}: {}",
                snap["configured"]
            );
        }
    }

    #[test]
    fn default_wayland_unrelated_set_keeps_unsupported_empty() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert_eq!(snap["unsupported"], json!([]));
        assert_eq!(snap["requested"]["visible"], json!(true));
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert_eq!(
            snap["unsupported"],
            json!([]),
            "default visible must not become persistent unsupported"
        );
        assert_eq!(snap["requested"]["visible"], json!(true));
    }

    #[test]
    fn explicit_visible_wayland_persists_through_unrelated_sets() {
        // Explicit false at spawn.
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","visible":false}]}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert!(
            snap["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("visible"))
        );
        assert_eq!(snap["requested"]["visible"], json!(false));
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert!(
            snap["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("visible"))
        );
        // Explicit true via set after a default spawn.
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"b"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["b"],"patch":{"visible":true}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "unsupported");
        assert!(
            entry["snapshot"]["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("visible"))
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["b"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert!(
            snap["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("visible"))
        );
        assert_eq!(snap["requested"]["visible"], json!(true));
    }

    #[test]
    fn explicit_visible_x11_stays_supported() {
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","visible":false}]}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert_eq!(snap["unsupported"], json!([]));
        assert_eq!(snap["configured"]["visible"], json!(false));
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        assert_eq!(
            reply_of(&out)["result"]["windows"][0]["snapshot"]["unsupported"],
            json!([])
        );
    }

    fn field_statuses(entry: &serde_json::Value) -> Vec<(String, String)> {
        entry["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["field"].as_str().unwrap().to_string(),
                    f["status"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    fn spawn_new_fields() -> String {
        r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","title":"lab","window_level":"always_on_top","cursor_visible":false,"cursor_hittest":true,"theme":"dark","transparent":true}]}"#.to_string()
    }

    #[test]
    fn spawn_reports_new_field_outcomes_and_configures() {
        let (mut server, mut host) = server();
        let out = server.handle_line(&mut host, &spawn_new_fields(), 1, None);
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert!(entry["ok"].as_bool().unwrap());
        let statuses = field_statuses(entry);
        for field in [
            "title",
            "window_level",
            "cursor_visible",
            "cursor_hittest",
            "theme",
            "transparent",
        ] {
            assert!(
                statuses.contains(&(field.to_string(), "accepted".to_string())),
                "{statuses:?}"
            );
        }
        let snap = &entry["snapshot"];
        assert_eq!(snap["title"], json!("OmniTilerTestWindow:a:run1 lab"));
        assert_eq!(snap["configured"]["window_level"], json!("always_on_top"));
        assert_eq!(snap["configured"]["theme"], json!("dark"));
        assert_eq!(snap["configured"]["title"], json!("lab"));
        assert_eq!(snap["unsupported"], json!([]));
    }

    #[test]
    fn bare_spawn_preserves_previous_defaults() {
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert_eq!(snap["title"], json!("OmniTilerTestWindow:a:run1"));
        for field in [
            "title",
            "window_level",
            "focus",
            "attention",
            "cursor_visible",
            "cursor_hittest",
            "enabled_buttons",
            "resize_increments_px",
            "theme",
            "transparent",
            "blur",
            "override_redirect",
        ] {
            assert!(
                snap["requested"].get(field).is_none() || snap["requested"][field].is_null(),
                "{field}"
            );
        }
        assert_eq!(snap["unsupported"], json!([]));
        assert_eq!(snap["observed"]["focused"], json!(false));
    }

    #[test]
    fn wayland_level_focus_buttons_increments_unsupported_not_cached() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"window_level":"always_on_top","focus":true,"enabled_buttons":["close"],"resize_increments_px":[8,8]}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        let statuses = field_statuses(entry);
        for field in [
            "window_level",
            "focus",
            "enabled_buttons",
            "resize_increments_px",
        ] {
            assert!(
                statuses.contains(&(field.to_string(), "unsupported".to_string())),
                "{statuses:?}"
            );
        }
        let snap = &entry["snapshot"];
        // Caller intent is retained, the host cache is not.
        assert_eq!(snap["requested"]["window_level"], json!("always_on_top"));
        assert!(
            snap["configured"].get("window_level").is_none()
                || snap["configured"]["window_level"].is_null()
        );
        for tag in [
            "window_level",
            "focus",
            "enabled_buttons",
            "resize_increments_px",
        ] {
            assert!(
                snap["unsupported"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(tag)),
                "{}",
                snap["unsupported"]
            );
        }
        // Focus was refused: no observed focus, and the fake never saw it
        // as a stored request.
        assert_eq!(snap["observed"]["focused"], json!(false));
    }

    #[test]
    fn focus_sets_observed_and_is_not_replayed() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"focus":true}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert!(entry["ok"].as_bool().unwrap());
        assert_eq!(entry["fields"][0]["status"], "accepted");
        assert_eq!(entry["snapshot"]["observed"]["focused"], json!(true));
        // One-shot: requested holds no focus, so an unrelated patch cannot
        // replay it.
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        assert_eq!(host.applied.len(), 2);
        assert!(host.applied[1].1.focus.is_none());
        assert!(server.snapshot("a").requested.focus.is_none());
    }

    #[test]
    fn attention_clear_wayland_details_activation_limit() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"attention":"clear"}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "unsupported");
        let detail = entry["fields"][0]["detail"].as_str().unwrap();
        assert!(detail.contains("xdg_activation_v1"), "{detail}");
        assert!(
            entry["snapshot"]["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("attention:clear"))
        );
        // Informational is a forwarded activation request: accepted.
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"attention":"informational"}}"#,
            1,
            None,
        );
        assert_eq!(
            reply_of(&out)["result"]["windows"][0]["fields"][0]["status"],
            "accepted"
        );
    }

    #[test]
    fn attention_clear_x11_accepted_hint_write_only() {
        // X11 `clear` sends the winit urgency-hint clear and reports accepted,
        // but that accepted status is not proof the compositor withdrew
        // `_NET_WM_STATE_DEMANDS_ATTENTION`: KWin keeps it on an inactive
        // window until activation. This locks the reporting contract so a
        // future change cannot silently reinterpret `accepted` as withdrawal.
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::x11();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        for patch in ["critical", "clear"] {
            let out = server.handle_line(
                &mut host,
                &format!(
                    r#"{{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{{"attention":"{patch}"}}}}"#
                ),
                1,
                None,
            );
            let entry = &reply_of(&out)["result"]["windows"][0];
            assert_eq!(entry["fields"][0]["status"], "accepted", "{patch}");
            assert!(
                entry["snapshot"]["unsupported"]
                    .as_array()
                    .unwrap()
                    .is_empty(),
                "{patch}"
            );
        }
    }

    #[test]
    fn title_suffix_round_trip_updates_snapshot_title() {
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","title":"lab"}]}"#,
            1,
            None,
        );
        assert_eq!(
            reply_of(&out)["result"]["windows"][0]["snapshot"]["title"],
            json!("OmniTilerTestWindow:a:run1 lab")
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"title":"next"}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "accepted");
        assert_eq!(
            entry["snapshot"]["title"],
            json!("OmniTilerTestWindow:a:run1 next")
        );
        assert_eq!(entry["snapshot"]["configured"]["title"], json!("next"));
        assert_eq!(entry["snapshot"]["requested"]["title"], json!("next"));
    }

    #[test]
    fn cursor_theme_resize_accepted_on_supporting_backends() {
        for backend in ["x11", "windows", "macos"] {
            let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
            let mut host = FakeHost::for_backend(backend);
            let out = server.handle_line(
                &mut host,
                r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","cursor_visible":false,"cursor_hittest":false,"enabled_buttons":["close","maximize"],"resize_increments_px":[8,8],"theme":"light"}]}"#,
                1,
                None,
            );
            let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
            if backend == "x11" {
                // Button hints have no X11/Wayland implementation.
                assert!(
                    snap["unsupported"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("enabled_buttons")),
                    "{backend}: {}",
                    snap["unsupported"]
                );
                assert!(
                    snap["configured"].get("enabled_buttons").is_none()
                        || snap["configured"]["enabled_buttons"].is_null()
                );
            } else {
                assert_eq!(
                    snap["configured"]["enabled_buttons"],
                    json!(["close", "maximize"]),
                    "{backend}"
                );
            }
            for field in [
                "cursor_visible",
                "cursor_hittest",
                "resize_increments_px",
                "theme",
            ] {
                assert!(
                    !snap["configured"].get(field).is_none()
                        && !snap["configured"][field].is_null(),
                    "{backend} must configure {field}: {}",
                    snap["configured"]
                );
            }
        }
    }

    #[test]
    fn transparent_blur_gates_match_host_support() {
        // X11: transparency is creation-only, blur is a no-op.
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","transparent":true,"blur":true}]}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        let statuses = field_statuses(entry);
        assert!(statuses.contains(&("transparent".to_string(), "accepted".to_string())));
        assert!(statuses.contains(&("blur".to_string(), "unsupported".to_string())));
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"transparent":false}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "unsupported");
        // The honored creation value survives the refused runtime change.
        assert_eq!(entry["snapshot"]["configured"]["transparent"], json!(true));
        // Wayland: both forwarded to the compositor state.
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"b","transparent":true,"blur":true}]}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        let statuses = field_statuses(entry);
        assert!(statuses.contains(&("transparent".to_string(), "accepted".to_string())));
        assert!(statuses.contains(&("blur".to_string(), "accepted".to_string())));
    }

    #[test]
    fn override_redirect_is_x11_creation_only() {
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","override_redirect":true}]}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "accepted");
        assert_eq!(
            entry["snapshot"]["configured"]["override_redirect"],
            json!(true)
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"override_redirect":false}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "unsupported");
        // Intent retained, cache keeps the creation value.
        assert_eq!(
            entry["snapshot"]["requested"]["override_redirect"],
            json!(false)
        );
        assert_eq!(
            entry["snapshot"]["configured"]["override_redirect"],
            json!(true)
        );
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"b","override_redirect":true}]}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "unsupported");
        assert!(
            entry["snapshot"]["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("override_redirect"))
        );
    }

    #[test]
    fn new_field_only_set_leaves_settled_alone() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.note_host_event(
            "a",
            HostEvent::Resized {
                size_px: [640, 480],
                scale_factor: 1.0,
            },
        );
        assert!(server.snapshot("a").settled);
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"cursor_visible":false,"theme":"dark","title":"lab"}}"#,
            1,
            None,
        );
        assert!(server.snapshot("a").settled);
    }

    #[test]
    fn spawn_actions_are_not_stored_or_replayed() {
        let (mut server, mut host) = server();
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","focus":true,"attention":"informational"}]}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert!(entry["ok"].as_bool().unwrap());
        let statuses = field_statuses(entry);
        assert!(statuses.contains(&("focus".to_string(), "accepted".to_string())));
        assert!(statuses.contains(&("attention".to_string(), "accepted".to_string())));
        let snap = &entry["snapshot"];
        for field in ["focus", "attention"] {
            assert!(
                snap["requested"].get(field).is_none() || snap["requested"][field].is_null(),
                "{field} must never enter requested"
            );
            assert!(
                snap["configured"].get(field).is_none() || snap["configured"][field].is_null(),
                "{field} must never enter configured"
            );
        }
        // Unrelated patch: no replay through the host, no lingering tags.
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"size_px":[800,600]}}"#,
            1,
            None,
        );
        assert!(
            reply_of(&out)["result"]["windows"][0]["ok"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(host.applied.len(), 1);
        assert!(host.applied[0].1.focus.is_none());
        assert!(host.applied[0].1.attention.is_none());
        let snap = server.snapshot("a");
        assert!(snap.requested.focus.is_none());
        assert!(snap.requested.attention.is_none());
        assert!(
            !snap
                .unsupported
                .iter()
                .any(|t| t.starts_with("focus") || t.starts_with("attention")),
            "{:?}",
            snap.unsupported
        );
    }

    #[test]
    fn refused_transparent_survives_unrelated_set() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a","transparent":true}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"transparent":false}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert_eq!(entry["fields"][0]["status"], "unsupported");
        // Unrelated patch must not erase the refused intent.
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert!(
            snap["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("transparent")),
            "{}",
            snap["unsupported"]
        );
        assert_eq!(snap["requested"]["transparent"], json!(false));
        assert_eq!(snap["configured"]["transparent"], json!(true));
    }

    #[test]
    fn hittest_error_persists_until_accepted() {
        let (mut server, mut host) = server();
        host.fail_hittest = true;
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"cursor_hittest":true}}"#,
            1,
            None,
        );
        assert_eq!(
            reply_of(&out)["result"]["windows"][0]["fields"][0]["status"],
            "unsupported"
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert!(
            snap["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("cursor_hittest")),
            "refused hit-test must survive an unrelated set: {}",
            snap["unsupported"]
        );
        assert_eq!(snap["requested"]["cursor_hittest"], json!(true));
        assert!(
            snap["configured"].get("cursor_hittest").is_none()
                || snap["configured"]["cursor_hittest"].is_null()
        );
        // Accepting the same field clears its tag.
        host.fail_hittest = false;
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"cursor_hittest":true}}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert!(
            !snap["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("cursor_hittest")),
            "{}",
            snap["unsupported"]
        );
        assert_eq!(snap["configured"]["cursor_hittest"], json!(true));
    }

    #[test]
    fn wayland_restore_refusal_persists() {
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::wayland();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"mode":"minimized"}}"#,
            1,
            None,
        );
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"mode":"windowed","size_px":[800,600]}}"#,
            1,
            None,
        );
        let entry = &reply_of(&out)["result"]["windows"][0];
        assert!(field_statuses(entry).contains(&("mode".to_string(), "unsupported".to_string())));
        let out = server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"decorations":false}}"#,
            1,
            None,
        );
        let snap = &reply_of(&out)["result"]["windows"][0]["snapshot"];
        assert!(
            snap["unsupported"]
                .as_array()
                .unwrap()
                .contains(&json!("mode:restore")),
            "{}",
            snap["unsupported"]
        );
        assert_eq!(snap["requested"]["mode"], json!("windowed"));
        assert_eq!(snap["configured"]["mode"], json!("minimized"));
        assert_eq!(snap["observed"]["mode"], json!("minimized"));
    }

    #[test]
    fn full_observation_without_geometry_keeps_pending_when_unknown() {
        let (mut server, mut host) = server();
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
            1,
            None,
        );
        server.handle_line(
            &mut host,
            r#"{"v":1,"request_id":"r","nonce":"secret","op":"set","ids":["a"],"patch":{"size_px":[800,600]}}"#,
            1,
            None,
        );
        assert!(!server.snapshot("a").settled);
        // Focus-style refresh with unknown geometry must not settle.
        server.note_full_observation("a", Observed::default(), false);
        assert!(!server.snapshot("a").settled);
        // Matching geometry settles even without a resize notification.
        let known = Observed {
            size_px: Some([800, 600]),
            ..Default::default()
        };
        server.note_full_observation("a", known, false);
        assert!(server.snapshot("a").settled);
    }
}
