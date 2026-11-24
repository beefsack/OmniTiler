#![forbid(unsafe_code)]
use super::*;

// ---------------------------------------------------------------------------
// v4 world connection: genuine handshake plus revisioned raw settings.
//
// Windows keeps its own in-process connection primitive (there is no
// `world_connection` module): the typed request is built from native truth
// here and handed directly to [`WorldEngine::handshake`] and
// [`WorldEngine::handle_settings`] (no in-process JSON encode/route/decode
// round trip). Reports keep the checked admission route
// (`handle_report_checked`) elsewhere. The core-minted generation binds the
// adapter [`WorldSequencer`](super::world_sequencer::WorldSequencer);
// reports and later settings gate on that bound generation. The engine is
// owned by the sequencer: no second engine owner exists.
// ---------------------------------------------------------------------------

use tiler_core::connection::{
    ApplicationIdentity, FactName, HandshakeRequest, HostCapabilities, NativeSelectorScope,
    RawSettingValue, SettingsDiagnostic, SettingsReply, SettingsRequest, SettingsStatus,
};
use tiler_core::world_engine::WorldEngine;

/// Whether the retained cursor-warp actuator is proven on this adapter.
/// `true` on the clean-HEAD live receipt from fdead7d45e: the ignored probe
/// `live_set_cursor_pos_does_not_advance_raw_input_generation` printed
/// `SetCursorPos gen 0 -> 0 (unchanged); synthetic move inserted=1,
/// gen 0 -> 1` and passed, so the adapter's own setter does not advance the
/// raw-input witness while delivered mouse input does. The handshake and the
/// apply-lane gate read this single value; the `cursorWarp` setting itself
/// remains off by default.
pub(super) const WARP_SUPPORTED: bool = true;

/// Windows host capabilities: the exact port-notes values for this slice.
///
/// - `host_owned: false`: Windows owns no native workspace domains.
/// - `equal_count: false`, `birth_clear: false`,
///   `shared_native_roster: false`: no native workspaces, no automatic birth
///   clears, no shared native roster.
/// - `native_selector_scope: PerOutput`: each output carries its own view;
///   Windows has no native selector, so no cross-output selector scope is
///   claimed.
/// - `window_suppression: true`: the product hide/restore lane exists.
/// - `maximize: true`, `fullscreen: true`: native actuators exist for each.
/// - `above: false`: no persistent stacking actuator exists yet; R14's
///   `StateTarget`/R12 crash-safe preimage lane has not landed, so a
///   truth-claiming window would have no honest topmost actuation or
///   recovery. Flip to `true` when that task proves both.
/// - `warp`: the retained exact-once cursor-warp lane, sourced from
///   [`WARP_SUPPORTED`]. It is true after the adapter lane and the
///   clean-HEAD live Raw Input non-increment receipt; the `cursorWarp`
///   setting still defaults off, so no warp is requested until enabled.
/// - `native_sticky: false`: no native sticky mechanism.
/// - `tiled_float_transfer: false`: no professed cross-output float remap.
/// - `unavailable_facts` is the precise not-yet-produced catalog set in
///   [`FactName`] order: `managed`, `acceptsInput`, `activationInhibited`,
///   `transient`, `modal`, `resizable`, `above`, `below`, `owner`, `parent`,
///   `role`, `appId`, `title`, `activationPolicy`, `layer`, `privileged`,
///   and `class`. The remaining four lanes (`toolWindow`, `owned`,
///   `executable`, `processId`) are the exact capture-emitted producer set;
///   an available fact may still be `None` when a single read is honestly
///   unknown, so availability is a producer claim, never a value claim.
///   `executable`/`processId` also stay available because [`own_identity`]
///   supplies both (the support-consistency gate). `privileged` is not
///   emitted (observe hardcodes `false` as a projected negative) and
///   `class` is read but not carried, so neither may be declared available.
#[must_use]
pub(super) fn windows_capabilities() -> HostCapabilities {
    HostCapabilities {
        host_owned: false,
        equal_count: false,
        shared_native_roster: false,
        birth_clear: false,
        native_selector_scope: NativeSelectorScope::PerOutput,
        window_suppression: true,
        maximize: true,
        fullscreen: true,
        native_sticky: false,
        above: false,
        warp: WARP_SUPPORTED,
        tiled_float_transfer: false,
        unavailable_facts: vec![
            FactName::Privileged,
            FactName::Managed,
            FactName::AcceptsInput,
            FactName::ActivationInhibited,
            FactName::Transient,
            FactName::Modal,
            FactName::Resizable,
            FactName::Above,
            FactName::Below,
            FactName::Owner,
            FactName::Parent,
            FactName::Role,
            FactName::AppId,
            FactName::Class,
            FactName::Title,
            FactName::ActivationPolicy,
            FactName::Layer,
        ],
    }
}

/// Own process identity from native truth only.
///
/// - `app_id`: `None` (Windows has no bundle identity source; no support
///   coupling exists for `appId`).
/// - `executable`: the own executable path, normalized by slash replacement
///   plus ASCII lowercase (the same identity rule as
///   [`crate::lifecycle`]'s path equality); `None` when the identity read
///   fails, which refuses the handshake instead of claiming support.
/// - `process_id`: the own pid as a decimal string.
fn own_identity() -> Option<ApplicationIdentity> {
    let own = crate::native::current_identity().ok()?;
    Some(ApplicationIdentity {
        app_id: None,
        executable: Some(own.exe_path.replace('/', "\\").to_ascii_lowercase()),
        process_id: Some(own.pid.to_string()),
    })
}

/// Typed handshake request for one adapter incarnation. `None` when own
/// identity cannot be read: a handshake that cannot prove its own executable
/// and pid refuses rather than declaring them unavailable while the window
/// producer emits them.
#[must_use]
fn build_handshake_request(adapter: &str) -> Option<HandshakeRequest> {
    Some(build_handshake_request_with_identity(
        adapter,
        own_identity()?,
    ))
}

#[must_use]
fn build_handshake_request_with_identity(
    adapter: &str,
    application_identity: ApplicationIdentity,
) -> HandshakeRequest {
    HandshakeRequest {
        adapter: adapter.to_owned(),
        request: "handshake-1".to_owned(),
        application_identity,
        capabilities: windows_capabilities(),
    }
}

/// Settings request at one revision: the supplied raw acquisition values.
/// Core validation/defaulting owns every key; the adapter never clamps or
/// defaults core-owned values.
#[must_use]
fn build_settings_request(
    adapter: &str,
    generation: &str,
    revision: u64,
    values: Vec<(String, RawSettingValue)>,
) -> SettingsRequest {
    SettingsRequest {
        adapter: adapter.to_owned(),
        generation: generation.to_owned(),
        request: format!("settings-{revision}"),
        revision,
        values,
    }
}

/// Outcome of one genuine handshake plus its initial settings exchange.
pub(super) struct ConnectOutcome {
    /// Core-minted generation binding the adapter sequencer.
    pub(super) generation: String,
    /// Accepted core reply for the initial revision.
    pub(super) settings: SettingsReply,
}

/// Run one revisioned settings exchange against `engine` through the typed
/// [`WorldEngine::handle_settings`] entry (no in-process JSON round trip).
/// Returns the core reply, or `None` when the request is unbound (foreign
/// adapter/generation or malformed correlation). Never retries on a bound
/// wait: the caller decides whether a later revision supersedes.
#[must_use]
pub(super) fn apply_settings_revision(
    engine: &mut WorldEngine,
    adapter: &str,
    generation: &str,
    revision: u64,
    values: &[(String, RawSettingValue)],
) -> Option<SettingsReply> {
    let request = build_settings_request(adapter, generation, revision, values.to_vec());
    engine.handle_settings(&request)
}

/// Run one genuine handshake plus the initial (revision 1) settings exchange
/// against `engine` with the supplied raw acquisition values. Returns the
/// core-minted generation plus the settings reply on success, `None` when
/// the handshake refuses (no generation minted, no state mutated), the
/// settings exchange refuses, or any codec leg fails.
pub(super) fn connect(
    engine: &mut WorldEngine,
    adapter: &str,
    values: &[(String, RawSettingValue)],
) -> Option<ConnectOutcome> {
    connect_with_handshake(engine, adapter, values, build_handshake_request(adapter)?)
}

/// Test-only connection with an explicit owner identity. Live windows are
/// owned by the test process, so the real own-pid handshake would exclude
/// every fixture as own-process; tests bind a synthetic owner instead.
#[cfg(test)]
pub(super) fn connect_with_identity(
    engine: &mut WorldEngine,
    adapter: &str,
    values: &[(String, RawSettingValue)],
    application_identity: ApplicationIdentity,
) -> Option<ConnectOutcome> {
    connect_with_handshake(
        engine,
        adapter,
        values,
        build_handshake_request_with_identity(adapter, application_identity),
    )
}

fn connect_with_handshake(
    engine: &mut WorldEngine,
    adapter: &str,
    values: &[(String, RawSettingValue)],
    handshake_request: HandshakeRequest,
) -> Option<ConnectOutcome> {
    let handshake = engine.handshake(&handshake_request);
    if handshake.kind.is_some() {
        return None;
    }
    let generation = handshake.generation?;
    let settings = apply_settings_revision(engine, adapter, &generation, 1, values)?;
    if settings.status != SettingsStatus::Accepted {
        return None;
    }
    Some(ConnectOutcome {
        generation,
        settings,
    })
}

/// Wire token for a settings reply status (log shape).
#[must_use]
pub(super) fn settings_status_token(status: SettingsStatus) -> &'static str {
    match status {
        SettingsStatus::Accepted => "accepted",
        SettingsStatus::Stale => "stale",
        SettingsStatus::RevisionReused => "revision-reused",
    }
}

/// Every settings diagnostic as `{key, kind}` rows (log shape). Callers log
/// the whole list so a `restart-required`, `unknown-field`, or `missing`
/// row is never silently dropped.
#[must_use]
pub(super) fn settings_diagnostics_json(
    diagnostics: &[SettingsDiagnostic],
) -> Vec<serde_json::Value> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            serde_json::json!({
                "key": diagnostic.key,
                "kind": diagnostic.kind.as_str(),
            })
        })
        .collect()
}

/// Native foreground veto read: one fresh pass over the live foreground with
/// exact desktop identity, fresh visibility, caption, DWM frame, and DWM
/// cloak facts fed into the portable [`crate::native_guards::classify_foreground`] policy.
/// No titles, paths, PIDs, or content leave this function; the caller logs
/// only the returned reason plus booleans. Pure reads, never IO.
pub(super) struct ForegroundRead {
    pub(super) veto: crate::native_guards::ForegroundVeto,
    pub(super) desktop: bool,
    pub(super) visible: bool,
    pub(super) captioned: bool,
    pub(super) dwm_readable: bool,
    pub(super) cloaked: bool,
}

fn foreground_read(foreground: u64, fulls: &[Rect]) -> ForegroundRead {
    use crate::native_guards::{ForegroundFacts, ForegroundVetoReason, classify_foreground};
    let settled = |veto: crate::native_guards::ForegroundVeto,
                   desktop: bool,
                   visible: bool,
                   captioned: bool,
                   dwm_readable: bool,
                   cloaked: bool| {
        ForegroundRead {
            veto,
            desktop,
            visible,
            captioned,
            dwm_readable,
            cloaked,
        }
    };
    let allow = |reason: ForegroundVetoReason,
                 desktop: bool,
                 visible: bool,
                 captioned: bool,
                 dwm_readable: bool,
                 cloaked: bool| {
        settled(
            crate::native_guards::ForegroundVeto {
                block: false,
                reason,
            },
            desktop,
            visible,
            captioned,
            dwm_readable,
            cloaked,
        )
    };
    if foreground == 0 {
        // No foreground window: nothing covering.
        return allow(
            ForegroundVetoReason::None,
            false,
            false,
            false,
            false,
            false,
        );
    }
    if !crate::ffi::is_window(foreground) {
        // `IsWindowVisible` returns false for invalid handles, so validity is
        // checked first and fails closed.
        return settled(
            crate::native_guards::ForegroundVeto {
                block: true,
                reason: ForegroundVetoReason::Invalid,
            },
            false,
            false,
            false,
            false,
            false,
        );
    }
    // Exact known desktop shell handles are the desktop, not a fullscreen
    // application. Narrow API identity only: no class-name, exe, tool/popup,
    // or unmanaged exceptions.
    let shell = crate::ffi::shell_window();
    let desktop_handle = crate::ffi::desktop_window();
    let is_desktop = (shell != 0 && shell == foreground)
        || (desktop_handle != 0 && desktop_handle == foreground);
    // Fresh unambiguous visibility read on the valid handle: invisible
    // windows cannot be covering fullscreen.
    let visible = crate::ffi::is_visible(foreground);
    let style = crate::ffi::window_style_checked(foreground);
    let style_ok = style.is_some();
    let style = style.unwrap_or(0);
    let captioned = style & WS_CAPTION != 0;
    let visible_raw = crate::ffi::extended_frame_bounds(foreground);
    let dwm_ok = visible_raw.is_some();
    // Fresh cloak fact on the same valid handle: a cloaked foreground is
    // invisible to the compositor and never a covering fullscreen, even
    // with a monitor-spanning frame. Query failure fails closed (unreadable)
    // like the frame read, matching the admission and hold gates.
    let cloak = crate::ffi::cloaked(foreground);
    let cloak_ok = cloak.is_some();
    let cloak_val = cloak.is_some_and(|cloaked| cloaked);
    if !crate::ffi::is_window(foreground) {
        return settled(
            crate::native_guards::ForegroundVeto {
                block: true,
                reason: ForegroundVetoReason::Invalid,
            },
            false,
            false,
            false,
            false,
            false,
        );
    }
    let readable = dwm_ok && style_ok;
    let covers_monitor = readable
        && visible_raw
            .and_then(rect_from_win)
            .is_some_and(|visible| is_borderless_fullscreen(true, visible, fulls));
    let veto = classify_foreground(ForegroundFacts {
        valid: true,
        is_desktop,
        visible,
        captioned,
        dwm_readable: readable,
        cloak_readable: cloak_ok,
        cloaked: cloak_val,
        covers_monitor,
    });
    settled(veto, is_desktop, visible, captioned, readable, cloak_val)
}

/// Managed-aware suspend read: the raw foreground veto, except a real
/// fullscreen foreground verified as a managed member or a born-held
/// first-seen fullscreen rides a retained overlay with no geometry writes
/// and never suspends the workspace (`ManagedOverlay`, like a managed
/// maximize which keeps its caption and never vetoes). Unverified fullscreen
/// still suspends, and unreadable/invalid foregrounds always stay blocked:
/// a non-covering or unreadable window is never labeled a managed overlay.
/// The managed branch reuses the single `holdable_key` verification
/// (identity, session, medium integrity, safety gates, scope, hosted child,
/// proof ownership) plus membership and the live lifetime tag, so no gate
/// is duplicated. Born tracking honors the first-seen gate: a managed
/// overlay transition (slotted member or lifetime-known non-fullscreen)
/// never re-enters the hold.
/// Shared managed verification for one captured foreground HWND: the
/// single `holdable_key` check (identity, session, medium integrity,
/// safety gates, scope, hosted child, proof ownership) plus membership and
/// the live lifetime tag. Callers capture the foreground once per suspend
/// read and thread it through, so the veto read, this verification, and
/// the returned HWND never race across separate native captures.
pub(super) fn suspend_read_inner(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    foreground: u64,
) -> ForegroundRead {
    use crate::native_guards::ForegroundVetoReason;
    let raw = foreground_read(foreground, fulls);
    if !raw.veto.block {
        return raw;
    }
    if raw.veto.reason != ForegroundVetoReason::Fullscreen {
        return raw;
    }
    if foreground == 0 {
        return raw;
    }
    let managed = || ForegroundRead {
        veto: crate::native_guards::ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::ManagedOverlay,
        },
        desktop: raw.desktop,
        visible: raw.visible,
        captioned: raw.captioned,
        dwm_readable: raw.dwm_readable,
        cloaked: raw.cloaked,
    };
    let Some(held_key) = holdable_key(state, me, foreground) else {
        return raw;
    };
    let tag_ok = {
        let live_tag = crate::product_hide::sys::read_member_tag(foreground);
        state.member_tags.get(&held_key).is_some_and(|stored| {
            crate::native_guards::visible_lifetime_ok(stored, live_tag.as_deref())
        })
    };
    if state.member_home(&held_key).is_some()
        && state.member_tokens.contains_key(&held_key)
        && tag_ok
    {
        return managed();
    }
    let known_slot = state
        .member_tokens
        .get(&held_key)
        .is_some_and(|token| state.member_rects.contains_key(token));
    if !tiler_core::window_policy::should_hold_born_fullscreen(
        true,
        known_slot,
        state.seen_nonfullscreen.contains(&held_key),
    ) {
        return raw;
    }
    managed()
}

pub(super) fn suspend_read(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
) -> ForegroundRead {
    #[cfg(test)]
    if let Some(blocked) = INJECTED_SUSPEND_SEQ.with(|slot| slot.borrow_mut().pop_front()) {
        use crate::native_guards::ForegroundVetoReason;
        let _ = (state, me, fulls);
        if blocked {
            return ForegroundRead {
                veto: crate::native_guards::ForegroundVeto {
                    block: true,
                    reason: ForegroundVetoReason::Fullscreen,
                },
                desktop: false,
                visible: true,
                captioned: false,
                dwm_readable: true,
                cloaked: false,
            };
        }
        return ForegroundRead {
            veto: crate::native_guards::ForegroundVeto {
                block: false,
                reason: ForegroundVetoReason::None,
            },
            desktop: false,
            visible: false,
            captioned: false,
            dwm_readable: false,
            cloaked: false,
        };
    }
    #[cfg(test)]
    if let Some(blocked) = INJECTED_SUSPEND.with(|slot| slot.get()) {
        use crate::native_guards::ForegroundVetoReason;
        let _ = (state, me, fulls);
        if blocked {
            return ForegroundRead {
                veto: crate::native_guards::ForegroundVeto {
                    block: true,
                    reason: ForegroundVetoReason::Fullscreen,
                },
                desktop: false,
                visible: true,
                captioned: false,
                dwm_readable: true,
                cloaked: false,
            };
        }
        return ForegroundRead {
            veto: crate::native_guards::ForegroundVeto {
                block: false,
                reason: ForegroundVetoReason::None,
            },
            desktop: false,
            visible: false,
            captioned: false,
            dwm_readable: false,
            cloaked: false,
        };
    }
    suspend_read_inner(state, me, fulls, crate::ffi::foreground_window())
}

/// True when the suspend read is an unmanageable fullscreen foreground:
/// a blocking veto with the `Fullscreen` reason. Managed overlays report
/// `ManagedOverlay` (non-blocking) and every other blocking shape
/// (`Invalid`/`Unreadable`) stays a full suspension. Only this shape may
/// take the limited lane (geometry for other windows, focus/overlays off).
pub(super) fn is_limited_fullscreen(read: &ForegroundRead) -> bool {
    read.veto.block && read.veto.reason == crate::native_guards::ForegroundVetoReason::Fullscreen
}

/// Non-activating geometry gate for one window under a suspend read: when
/// not suspended everything writes through revalidation; under a full
/// suspension nothing writes; under a limited unmanageable fullscreen only
/// windows other than the foreground write. An unknown foreground (`0`)
/// fails closed to no writes.
#[cfg(test)]
pub(super) fn geometry_write_allowed(
    read: &ForegroundRead,
    window_hw: u64,
    foreground_hw: u64,
) -> bool {
    if !read.veto.block {
        return true;
    }
    if !is_limited_fullscreen(read) {
        return false;
    }
    if foreground_hw == 0 {
        return false;
    }
    window_hw != foreground_hw
}

/// One suspend read plus its limited-mode foreground from the same native
/// capture: the veto read, the managed holdable verification, and the
/// returned HWND share one `foreground_window` value, so a foreground
/// change mid-read cannot mix verdicts. (Per-write resnapshots in the
/// geometry pass stay fresh by design: each write re-reads.) The HWND is
/// nonzero only for a limited unmanageable fullscreen; every other shape
/// returns 0 and stays fully suspended.
pub(super) fn suspend_limited(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
) -> (ForegroundRead, u64) {
    #[cfg(test)]
    if INJECTED_SUSPEND.with(|slot| slot.get()).is_some()
        || INJECTED_SUSPEND_SEQ.with(|slot| !slot.borrow().is_empty())
    {
        // Offline suspension fixtures stay native-free: no foreground HWND
        // is knowable without a native capture, so the limited lane reads
        // unknown (0, fail closed to full suspension) here.
        let read = suspend_read(state, me, fulls);
        return (read, 0);
    }
    let foreground = crate::ffi::foreground_window();
    let read = suspend_read_inner(state, me, fulls, foreground);
    let fg = if is_limited_fullscreen(&read) {
        foreground
    } else {
        0
    };
    (read, fg)
}

/// Owned inputs for one `run_tile_loop` invocation. Bundled so the loop
/// entry keeps a narrow signature as keyboard policy joins the run.
/// `mouse_snap` arms session-only `SPI_SETWINARRANGING FALSE` prevention for
/// product runs; proof runs always pass false (no settings changes).
pub(super) struct TileRun {
    pub(super) seconds: Option<u64>,
    pub(super) trace: bool,
    pub(super) allowlist: Option<Vec<AllowEntry>>,
    pub(super) proof: bool,
    /// `shortcut-proof` only: proof geometry gate stays on, but the owner
    /// installs the hook with test-only marker acceptance and drives the same
    /// keyboard dispatcher plus session-only mouse routines. Product `tile`
    /// and `tile-proof` always pass false.
    pub(super) shortcut_proof: bool,
    /// `workspace-proof` only: like `shortcut-proof` plus the workspace
    /// dispatcher (select/send/follow with hide/reveal) for exactly the
    /// frozen allowlist. Never enabled on the product path.
    pub(super) workspace_proof: bool,
    pub(super) raw_argv: Vec<String>,
    pub(super) keyboard: KeyboardConfig,
    pub(super) mouse_snap: bool,
    /// First-run note for the `tile-start` log (`None` on proof paths, which
    /// never prompt): the chosen preset plus whether it persisted.
    pub(super) first_run: Option<String>,
    /// Normal owner only (`None` on proof paths): the parsed options plus the
    /// pre-lease settings state, resolved into the effective options under
    /// the owner lease before any effect or hook (so a refused second owner
    /// never prompts and a mid-prompt file race never overwrites).
    pub(super) first_run_pending: Option<FirstRunPending>,
    /// Live inner/outer gaps for this run (settings base, CLI overrides).
    pub(super) inner_gap: i32,
    pub(super) outer_gap: i32,
    /// Settings directory for live polling (`None` disables polling: proof
    /// owners never load the user's store).
    pub(super) settings_dir: Option<std::path::PathBuf>,
    /// Initialized last-good live settings (`None` disables polling).
    pub(super) settings_live: Option<LiveSettings>,
    /// Explicit startup CLI switches, authoritative for the whole run (empty
    /// on proof paths, which never poll the file).
    pub(super) cli_overrides: crate::tiling::CliOverrides,
    /// Explicit normal-mode scope filter (empty means no filter). Proof runs
    /// always pass empty (frozen allowlist is the gate there).
    pub(super) scope: Vec<String>,
    /// Explicit host-to-child scope pairs (empty means no child constraint).
    /// Proof runs always pass empty.
    pub(super) scope_hosts: Vec<ScopeHostChild>,
    pub(super) border: ActiveBorderOptions,
    pub(super) pager_highlight: crate::pager_highlight::PagerHighlightStyle,
    pub(super) underlay: GroupUnderlayOptions,
}

/// Apply validated live settings through the owner's pump (normal owner
/// only). Returns true when owner state changed, so the caller wakes the pump
/// and gap adoption reconciles promptly. Gaps flip immediately and adopt
/// through the retained update-gaps path in `reconcile_tick` (plain reconcile
/// would refuse the drift); border/underlay assign directly for the next
/// overlay refresh; keyboard plus rebound/suppression chords route through the
/// hook's `apply_live_config` (in-flight holds keep their down-time verdict
/// and mask, the stale queue drains, nothing dispatches under the new
/// config); mouse prevention toggles through the existing conditional
/// restore (off) and fresh-preimage disable (on) routines, flag-only while
/// suspended so the resume transition drives the effect.
///
/// Status semantics are honest by construction: `saved:{revision}` means the
/// file revision is assigned to owner state (per-lane evidence rides the
/// `settings-applied` lanes plus the `settings-keyboard` row); the mouse
/// effect rides the existing suspend/resume outcome logs (`mouse-pending`
/// while suspended defers it to resume); gap adoption rides the Engine
/// update-gaps trace. Invalid files keep last-good with a `degraded:{reason}`
/// status and are never rewritten here.
///
/// Explicit startup CLI switches stay authoritative: the file base applies
/// underneath [`crate::tiling::CliOverrides`], so an unrelated file edit
/// never drops a CLI lane.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_live_settings(
    state: &mut TileLoop,
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    snap_want: &mut bool,
    settings: crate::settings::Settings,
    mtime: Option<std::time::SystemTime>,
) -> bool {
    let log_path = state.log_path.clone();
    let mut base = crate::settings::tile_options_from_settings(&settings);
    crate::tiling::apply_cli_overrides(&mut base, &state.cli_overrides);
    let mut lanes: Vec<&'static str> = Vec::new();
    let mut changed = false;
    if base.inner_gap != state.inner_gap || base.outer_gap != state.outer_gap {
        state.inner_gap = base.inner_gap;
        state.outer_gap = base.outer_gap;
        lanes.push("gaps");
        changed = true;
    }
    // Live default edits seed future workspaces only (the core reads the
    // default from every report ingress): existing logical workspaces keep
    // their retained modes. No adapter session state exists to set.
    if state.default_tiled_seed() != settings.core.workspace.default_tiled {
        lanes.push("workspace-default");
        changed = true;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "workspace-default",
                "default_tiled": settings.core.workspace.default_tiled,
                "outcome": "adopted",
                "scope": "future-workspaces",
            }),
        );
    }
    // Item 12 stage B: a saved scope-mode change never adopts live. The
    // pending choice stays in the saved document (assigned to `live.settings`
    // below) and takes effect only on the next owner restart; the drift lane
    // is informational and never wakes the pump. The core reads the mode
    // from every report ingress, so dispatch needs no mutation here.
    {
        let saved = crate::settings::saved_mode(&settings);
        if saved != state.scope_adopted {
            lanes.push("workspace-scope");
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-scope",
                    "adopted": tiler_core::workspace_scope::mode_token(state.scope_adopted),
                    "saved": tiler_core::workspace_scope::mode_token(saved),
                    "revision": settings.revision,
                    "outcome": "restart-required",
                    "scope": "live-pending",
                }),
            );
        }
    }
    if base.border != state.border {
        state.border = base.border;
        lanes.push("border");
        changed = true;
    }
    if base.pager_highlight != state.pager_highlight {
        state.pager_highlight = base.pager_highlight;
        lanes.push("pager-highlight");
        changed = true;
    }
    if base.underlay != state.underlay {
        state.underlay = base.underlay;
        lanes.push("underlay");
        changed = true;
    }
    let keyboard = KeyboardConfig {
        takeover: !base.no_keyboard_snap_takeover,
        allow_win_l: base.allow_win_l,
    };
    let remap = crate::settings::build_remap(&settings);
    let disabled = crate::settings::build_disabled(&settings);
    if keyboard != state.keyboard || remap != state.last_remap || disabled != state.last_disabled {
        state.keyboard = keyboard;
        state.last_remap = remap.clone();
        state.last_disabled = disabled.clone();
        if state.input_bridge.is_some() {
            // Product path: the policy rides the next versioned snapshot
            // (generations assigned at offer time) and the pump applies it in
            // FIFO order; the true stale-drained count arrives with the
            // `settings-keyboard-ack` event. No direct hook call: input TLS
            // lives on the pump thread. Dispatch of superseded batches is
            // gated owner-side by generation until the ack lands.
            lanes.push("keyboard");
            changed = true;
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "settings-keyboard",
                    "takeover": keyboard.takeover,
                    "allow_win_l": keyboard.allow_win_l,
                    "remap": remap.len(),
                    "disabled": disabled.len(),
                    "drained": 0,
                    "ack_pending": true,
                }),
            );
        } else {
            let drained = crate::snapkey::sys::apply_live_config(keyboard, &remap, &disabled);
            lanes.push("keyboard");
            changed = true;
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "settings-keyboard",
                    "takeover": keyboard.takeover,
                    "allow_win_l": keyboard.allow_win_l,
                    "remap": remap.len(),
                    "disabled": disabled.len(),
                    "drained": drained,
                }),
            );
        }
    }
    let want_snap = !base.no_mouse_snap_prevention;
    if want_snap != *snap_want {
        // Mirror the suspend/resume fencing: while suspended the ledger
        // already holds the restored state, so only the flag flips and the
        // resume transition drives the effect.
        if want_snap {
            if !state.suspended {
                snap_resume(dir, me, store);
            }
        } else if !state.suspended {
            snap_suspend(dir, me, store);
        }
        *snap_want = want_snap;
        lanes.push(if state.suspended {
            "mouse-pending"
        } else {
            "mouse"
        });
        changed = true;
    }
    let revision = settings.revision;
    // Taskbar pager opt-out (default on): hot-applies like the border and
    // underlay flags; the next pump shows or hides the strips.
    if settings.core.taskbar_pager != state.taskbar_pager {
        state.taskbar_pager = settings.core.taskbar_pager;
        lanes.push("taskbar-pager");
        changed = true;
    }
    // R-MOV-03: the same-axis mode adopts here with no lane and no rebuild:
    // assigning `live.settings` below is enough, since `keyboard_tick` reads
    // the mode per move. A change to only this field returns false, so no
    // tree rebuild or resync follows; the next move simply uses the new mode.
    // R-MAX-06 rides the same assignment: `admit_slotless_maximized` binds
    // the born-maximized policy per fresh lifetime at admission, so a
    // setting-only change adopts for subsequent first admissions with no
    // rebuild, and already-bound lifetimes plus previously admitted clients
    // keep their slots and topology.
    // R-SPC-04 D1/D2: the fixed-size predicate adopts here for subsequent
    // admissions only. The core reads it from every report ingress without
    // touching retained topology or existing marks (admission-only
    // classification). A change to only this field returns false, so no
    // rebuild or resync follows.
    let predicate = settings.core.fixed_size_predicate_mode();
    let last_predicate = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.fixed_size_predicate_mode());
    if last_predicate != Some(predicate) {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "settings-predicate",
                "predicate": predicate.as_wire_str(),
                "outcome": "adopted",
                "scope": "subsequent-admissions",
            }),
        );
    }
    // Stage 2c cursor-first targeting: switch target and fresh placement
    // adopt for subsequent selections and fresh admissions only. The core
    // reads both from every report ingress without touching retained views
    // or members (settings changes never rehome). A change to only these
    // fields returns false, so no rebuild follows.
    let switch_target = settings.core.switch_target_mode();
    let last_switch = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.switch_target_mode());
    if last_switch != Some(switch_target) {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "settings-switch-target",
                "switch_target": switch_target.as_wire_str(),
                "outcome": "adopted",
                "scope": "subsequent-selections",
            }),
        );
    }
    let placement = settings.core.fresh_placement_mode();
    let last_placement = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.fresh_placement_mode());
    if last_placement != Some(placement) {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "settings-fresh-placement",
                "fresh_placement": placement.as_wire_str(),
                "outcome": "adopted",
                "scope": "subsequent-admissions",
            }),
        );
    }
    // Stage 2c retained settings: focus-follows-cursor and cursor-warp are
    // reported on every capture but actuate nothing until stage 4. Adoption
    // only updates the retained document below, like the predicate lanes.
    if state.settings_live.as_ref().is_none_or(|live| {
        live.settings.core.focus_follows_cursor != settings.core.focus_follows_cursor
    }) {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "settings-focus-follows-cursor",
                "focus_follows_cursor": settings.core.focus_follows_cursor,
                "outcome": "adopted",
                "scope": "retained-until-stage-4",
            }),
        );
    }
    if state
        .settings_live
        .as_ref()
        .is_none_or(|live| live.settings.core.cursor_warp != settings.core.cursor_warp)
    {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "settings-cursor-warp",
                "cursor_warp": settings.core.cursor_warp,
                "outcome": "adopted",
                "scope": "retained-until-stage-4",
            }),
        );
    }
    // Core policy follows the document through the bound connection. One
    // full snapshot per revision; a refusal keeps the last good core state
    // and is surfaced rather than silently ignored. Every diagnostic
    // (key + kind, `restart-required` included) is logged so a stale/
    // reused revision or unsupported key can never hide.
    match state
        .world
        .apply_core_settings(&settings, &state.cli_overrides)
    {
        Some(reply) => {
            let accepted = reply.status == SettingsStatus::Accepted;
            if accepted {
                lanes.push("core");
                changed = true;
            } else {
                lanes.push("core-refused");
            }
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "settings-core",
                    "outcome": if accepted { "accepted" } else { "refused" },
                    "scope": "live",
                    "revision": reply.revision,
                    "status": settings_status_token(reply.status),
                    "diagnostics": settings_diagnostics_json(&reply.diagnostics),
                }),
            );
        }
        None => {
            // Unbound correlation (foreign generation/adapter): nothing was
            // accepted, so the last good core state stands. Surface the
            // attempted file revision rather than dropping the refusal.
            lanes.push("core-refused");
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "settings-core",
                    "outcome": "refused",
                    "scope": "live",
                    "revision": revision,
                    "status": "unbound",
                    "diagnostics": [],
                }),
            );
        }
    }
    if let Some(live) = state.settings_live.as_mut() {
        let saved_scope = crate::settings::saved_mode(&settings);
        let status = if saved_scope == state.scope_adopted {
            format!("saved:{revision}")
        } else {
            format!(
                "saved:{revision}+scope-restart:{}",
                tiler_core::workspace_scope::mode_token(saved_scope)
            )
        };
        live.settings = settings;
        live.mtime = mtime;
        live.status = status;
    }
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "settings-applied",
            "revision": revision,
            "lanes": lanes,
        }),
    );
    changed
}

/// Poll the settings file once per pump (normal owner only; proof owners
/// never poll). Changed files validate then apply (returns true when owner
/// state changed); invalid files keep the last-good values live with a
/// degraded status and are never rewritten here (no blind reset). Degraded
/// transitions log once per status.
pub(super) fn poll_live_settings(
    state: &mut TileLoop,
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    snap_want: &mut bool,
) -> bool {
    let Some(settings_dir) = state.settings_dir.clone() else {
        return false;
    };
    let Some(last) = state.settings_live.clone() else {
        return false;
    };
    match crate::settings::poll_for_change(&settings_dir, &last) {
        crate::settings::PollOutcome::Unchanged => false,
        crate::settings::PollOutcome::Changed(settings) => {
            let mtime = std::fs::metadata(settings_dir.join(crate::settings::SETTINGS_FILE_NAME))
                .and_then(|meta| meta.modified())
                .ok();
            apply_live_settings(state, dir, me, store, snap_want, settings, mtime)
        }
        crate::settings::PollOutcome::InvalidKept(reason) => {
            let degraded = format!("degraded:{reason}");
            let changed = state
                .settings_live
                .as_ref()
                .is_some_and(|live| live.status != degraded);
            if changed {
                if let Some(live) = state.settings_live.as_mut() {
                    live.status = degraded.clone();
                }
                log_json_at(
                    &state.log_path.clone(),
                    serde_json::json!({
                        "event": "settings-degraded",
                        "status": degraded,
                    }),
                );
            }
            false
        }
    }
}

/// Startup/live settings-sync proof through the real typed connection.
///
/// These exercise the production handshake/settings entries (no codec round
/// trip): `bootstrap_with_settings` accepts revision 1 exactly like the
/// runtime startup, and later revisions ride `apply_settings_values`.
#[cfg(test)]
mod tiling_sync_tests {
    use super::super::world_sequencer::WorldSequencer;
    use crate::tiling::CliOverrides;
    use tiler_core::connection::{
        RawSettingValue, SettingsDiagnostic, SettingsDiagnosticKind, SettingsStatus,
    };
    use tiler_core::settings_catalog::{
        FIELD_BORDER_WIDTH, FIELD_INNER_GAP, FIELD_OUTER_GAP, FIELD_UNDERLAY_EXTENSION,
    };
    use tiler_core::settings_state::EffectiveSetting;
    use tiler_core::workspace_scope::{WORKSPACE_MODE_TOKENS, WorkspaceMode};

    const ADAPTER: &str = "adapter-settings-sync";
    const GENERATION: &str = "gen-settings-sync";

    const ALL_WIRE_KEYS: [&str; 18] = [
        "gap",
        "outer",
        "defaultTiled",
        "mode",
        "sameAxis",
        "bornMaximized",
        "predicate",
        "refill",
        "switchTarget",
        "freshPlacement",
        "focusFollowsCursor",
        "cursorWarp",
        "borderEnabled",
        "underlayEnabled",
        "borderWidth",
        "borderGap",
        "borderRadius",
        "underlayExtension",
    ];

    fn bootstrap(values: &[(String, RawSettingValue)]) -> WorldSequencer {
        WorldSequencer::bootstrap_with_settings(ADAPTER, GENERATION, values)
            .expect("settings bootstrap")
    }

    fn value<'a>(values: &'a [(String, RawSettingValue)], key: &str) -> &'a RawSettingValue {
        &values
            .iter()
            .find(|(candidate, _)| candidate == key)
            .unwrap_or_else(|| panic!("wire key {key}"))
            .1
    }

    fn snapshot(
        settings: &crate::settings::Settings,
        cli: &CliOverrides,
    ) -> Vec<(String, RawSettingValue)> {
        crate::settings::core_setting_values(settings, cli)
    }

    #[test]
    fn all_core_wire_keys_accept_with_zero_diagnostics() {
        let settings = crate::settings::Settings::default();
        let cli = CliOverrides::default();
        let values = snapshot(&settings, &cli);
        assert_eq!(values.len(), 18);
        for key in ALL_WIRE_KEYS {
            value(&values, key);
        }

        // Startup revision 1 (production bootstrap) accepts the full
        // snapshot with no diagnostics.
        let mut seq = bootstrap(&values);
        let initial = seq.take_initial_settings_reply().expect("initial reply");
        assert_eq!(initial.revision, 1);
        assert_eq!(initial.status, SettingsStatus::Accepted);
        assert_eq!(initial.diagnostics, Vec::new());
        assert_eq!(seq.settings_revision(), 2);

        // The next revision accepts the same 18 keys with zero diagnostics.
        let next = seq.apply_settings_values(&values).expect("settings reply");
        assert_eq!(next.revision, 2);
        assert_eq!(next.status, SettingsStatus::Accepted);
        assert_eq!(next.diagnostics, Vec::new());
        assert_eq!(seq.settings_revision(), 3);
    }

    #[test]
    fn cli_overrides_win_in_startup_and_later_snapshots() {
        let cli = CliOverrides {
            inner_gap: Some(4),
            outer_gap: Some(20),
            border_width: Some(7.5),
            underlay_extension: Some(3.5),
            ..CliOverrides::default()
        };
        let mut file = crate::settings::Settings::default();
        let values = snapshot(&file, &cli);
        assert_eq!(value(&values, "gap"), &RawSettingValue::Number(4.0));
        assert_eq!(value(&values, "outer"), &RawSettingValue::Number(20.0));
        assert_eq!(value(&values, "borderWidth"), &RawSettingValue::Number(7.5));
        assert_eq!(
            value(&values, "underlayExtension"),
            &RawSettingValue::Number(3.5)
        );

        let mut seq = bootstrap(&values);
        let initial = seq.take_initial_settings_reply().expect("initial reply");
        assert_eq!(initial.status, SettingsStatus::Accepted);
        assert!(initial.diagnostics.is_empty());
        let settings = seq.engine_ref().settings();
        assert_eq!(
            settings.effective[FIELD_INNER_GAP],
            EffectiveSetting::Int(4)
        );
        assert_eq!(
            settings.effective[FIELD_OUTER_GAP],
            EffectiveSetting::Int(20)
        );
        assert_eq!(
            settings.effective[FIELD_BORDER_WIDTH],
            EffectiveSetting::Float(7.5)
        );
        assert_eq!(
            settings.effective[FIELD_UNDERLAY_EXTENSION],
            EffectiveSetting::Float(3.5)
        );

        // A later file snapshot that edits the same keys underneath the CLI
        // switches must still send (and accept) the CLI values.
        file.core.inner_gap = 12;
        file.core.outer_gap = 13;
        file.core.border.width = 2.0;
        file.core.underlay.extension = 1.0;
        let next_values = snapshot(&file, &cli);
        assert_eq!(value(&next_values, "gap"), &RawSettingValue::Number(4.0));
        assert_eq!(value(&next_values, "outer"), &RawSettingValue::Number(20.0));
        assert_eq!(
            value(&next_values, "borderWidth"),
            &RawSettingValue::Number(7.5)
        );
        assert_eq!(
            value(&next_values, "underlayExtension"),
            &RawSettingValue::Number(3.5)
        );
        let reply = seq
            .apply_settings_values(&next_values)
            .expect("settings reply");
        assert_eq!(reply.status, SettingsStatus::Accepted);
        assert!(reply.diagnostics.is_empty());
        let settings = seq.engine_ref().settings();
        assert_eq!(
            settings.effective[FIELD_INNER_GAP],
            EffectiveSetting::Int(4)
        );
        assert_eq!(
            settings.effective[FIELD_OUTER_GAP],
            EffectiveSetting::Int(20)
        );
        assert_eq!(
            settings.effective[FIELD_BORDER_WIDTH],
            EffectiveSetting::Float(7.5)
        );
    }

    #[test]
    fn refused_revisions_do_not_advance() {
        let settings = crate::settings::Settings::default();
        let cli = CliOverrides::default();
        let values = snapshot(&settings, &cli);
        let mut seq = bootstrap(&values);
        assert_eq!(seq.settings_revision(), 2);

        // Older revision: Stale, no advance and no state mutation.
        let stale = seq
            .apply_settings_at_for_test(0, &values)
            .expect("settings reply");
        assert_eq!(stale.status, SettingsStatus::Stale);
        assert_eq!(seq.settings_revision(), 2);

        // Equal revision with different content: RevisionReused, no advance.
        let mut different = values.clone();
        different
            .iter_mut()
            .find(|(key, _)| key == "gap")
            .expect("gap key")
            .1 = RawSettingValue::Number(9.0);
        let reused = seq
            .apply_settings_at_for_test(1, &different)
            .expect("settings reply");
        assert_eq!(reused.status, SettingsStatus::RevisionReused);
        assert_eq!(seq.settings_revision(), 2);

        // The live path continues from the untouched revision.
        let live = seq.apply_settings_values(&values).expect("settings reply");
        assert_eq!(live.revision, 2);
        assert_eq!(live.status, SettingsStatus::Accepted);
        assert_eq!(seq.settings_revision(), 3);
    }

    #[test]
    fn restart_only_mode_change_reports_restart_required() {
        let mut settings = crate::settings::Settings::default();
        let cli = CliOverrides::default();
        let startup = snapshot(&settings, &cli);
        let mut seq = bootstrap(&startup);
        let initial = seq.take_initial_settings_reply().expect("initial reply");
        assert_eq!(initial.status, SettingsStatus::Accepted);
        assert!(initial.diagnostics.is_empty());
        assert_eq!(
            seq.engine_ref().settings().workspace_mode,
            WorkspaceMode::PerOutputLocal
        );

        settings.core.workspace.mode = WORKSPACE_MODE_TOKENS[2].to_owned();
        let changed = snapshot(&settings, &cli);
        let reply = seq.apply_settings_values(&changed).expect("settings reply");
        assert_eq!(reply.revision, 2);
        assert_eq!(reply.status, SettingsStatus::Accepted);
        assert_eq!(
            reply.diagnostics,
            vec![SettingsDiagnostic {
                key: "mode".to_owned(),
                kind: SettingsDiagnosticKind::RestartRequired,
            }]
        );
        // Restart-only request is retained but the effective mode is unchanged.
        assert_eq!(
            seq.engine_ref().settings().workspace_mode,
            WorkspaceMode::PerOutputLocal
        );
    }

    #[test]
    fn startup_mode_shared_adopts_at_revision_one() {
        let mut settings = crate::settings::Settings::default();
        settings.core.workspace.mode = WORKSPACE_MODE_TOKENS[2].to_owned();
        let cli = CliOverrides::default();
        let values = snapshot(&settings, &cli);
        let mut seq = bootstrap(&values);
        let initial = seq.take_initial_settings_reply().expect("initial reply");
        assert_eq!(initial.revision, 1);
        assert_eq!(initial.status, SettingsStatus::Accepted);
        assert!(
            initial.diagnostics.is_empty(),
            "shared adopts with no diagnostics, got {:?}",
            initial.diagnostics
        );
        assert_eq!(
            seq.engine_ref().settings().workspace_mode,
            WorkspaceMode::Shared
        );
    }
}
