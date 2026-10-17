#![forbid(unsafe_code)]
use super::*;

/// Native foreground veto read: one fresh pass over the live foreground with
/// exact desktop identity, fresh visibility, caption, DWM frame, and DWM
/// cloak facts fed into the portable [`crate::workspace_owner::classify_foreground`] policy.
/// No titles, paths, PIDs, or content leave this function; the caller logs
/// only the returned reason plus booleans. Pure reads, never IO.
pub(super) struct ForegroundRead {
    pub(super) veto: crate::workspace_owner::ForegroundVeto,
    pub(super) desktop: bool,
    pub(super) visible: bool,
    pub(super) captioned: bool,
    pub(super) dwm_readable: bool,
    pub(super) cloaked: bool,
}

fn foreground_read(fulls: &[Rect]) -> ForegroundRead {
    use crate::workspace_owner::{ForegroundFacts, ForegroundVetoReason, classify_foreground};
    let settled = |veto: crate::workspace_owner::ForegroundVeto,
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
            crate::workspace_owner::ForegroundVeto {
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
    let foreground = crate::ffi::foreground_window();
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
            crate::workspace_owner::ForegroundVeto {
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
            crate::workspace_owner::ForegroundVeto {
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
pub(super) fn suspend_read(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
) -> ForegroundRead {
    #[cfg(test)]
    if let Some(blocked) = INJECTED_SUSPEND.with(|slot| slot.get()) {
        use crate::workspace_owner::ForegroundVetoReason;
        let _ = (state, me, fulls);
        if blocked {
            return ForegroundRead {
                veto: crate::workspace_owner::ForegroundVeto {
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
            veto: crate::workspace_owner::ForegroundVeto {
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
    use crate::workspace_owner::ForegroundVetoReason;
    let raw = foreground_read(fulls);
    if !raw.veto.block {
        return raw;
    }
    if raw.veto.reason != ForegroundVetoReason::Fullscreen {
        return raw;
    }
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 {
        return raw;
    }
    let managed = || ForegroundRead {
        veto: crate::workspace_owner::ForegroundVeto {
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
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        })
    };
    if state.workspaces.member_loc(&held_key).is_some()
        && state.member_tokens.contains_key(&held_key)
        && tag_ok
    {
        return managed();
    }
    let known_slot = state
        .member_tokens
        .get(&held_key)
        .is_some_and(|token| state.member_rects.contains_key(token));
    if !should_hold_born_fullscreen(
        true,
        known_slot,
        state.seen_nonfullscreen.contains(&held_key),
    ) {
        return raw;
    }
    managed()
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
    // Live default edits adopt for future workspaces only: existing session
    // overrides keep their state, exactly like startup seeding.
    if state
        .workspaces
        .set_default_tiled(settings.core.workspace.default_tiled)
    {
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
    // is informational and never wakes the pump.
    if let Some((lane, event)) =
        crate::workspace_scope_owner::pending_scope_lane(&state.scope_owner, &settings)
    {
        log_json_at(&log_path, event);
        lanes.push(lane);
    }
    if base.border != state.border {
        state.border = base.border;
        lanes.push("border");
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
    // admissions only. The Engine propagates the value to every retained
    // session without touching topology, revision, or existing
    // automatic/override marks (admission-only classification); the D4/D5
    // admission gates read it per fresh lifetime through the same
    // assignment. A change to only this field returns false, so no tree
    // rebuild or resync follows.
    let predicate = settings.core.fixed_size_predicate_mode();
    if state.engine.fixed_size_predicate() != predicate {
        state.engine.set_fixed_size_predicate(predicate);
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
    if let Some(live) = state.settings_live.as_mut() {
        let saved_scope = crate::workspace_scope_owner::saved_mode(&settings);
        let status = state.scope_owner.status_for(revision, saved_scope);
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
