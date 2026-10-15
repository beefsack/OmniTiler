#![forbid(unsafe_code)]

//! Typed native fact readers for the scoped tile (read-only, no titles).
//!
//! Threading (fixed architecture):
//! - [`read_app_window_facts`] runs on the owning per-app AX thread: every
//!   `AxElement` is constructed, used, and dropped there. A finite messaging
//!   timeout is set BEFORE every join and read (app element and each window
//!   object); a refused timeout skips the narrow read, never an unbounded one.
//! - [`read_app_facts`] runs on the caller (main) thread over public AppKit
//!   (`NSRunningApplication`, no AX consent involved).
//!
//! Honesty: window reads carry unknowns as `None` (frame plus identity are
//! the only failing legs). Classification never sees a partial window:
//! [`NativeWindowFacts::to_ax_facts`] / [`to_cg_facts`] return `None` while
//! any required fact is unknown, so an incomplete read can never publish as
//! a complete replacement (which would invent destruction downstream).
//! Counts (`requested`/`observed`/`failed` plus the first cause) mirror the
//! existing snapshot contract: any failed leg fails the read, never an empty
//! list standing in for truth.
//!
//! Size hints: only actually reported minimums fill [`WindowSizeHints`]
//! (`AXMinSize`, then `AXMinimumSize` per Rectangle prior art, see
//! [`crate::ffi`]). No AX max size is probed (no surveyed source reads one),
//! so max stays absent — and nothing is ever inferred from `AXSize`
//! settability, which is carried only for the later TBD decision.

use tiler_core::size_hints::WindowSizeHints;

use crate::classifier::{ActivationPolicy, AppFacts, AxFacts, CgFacts};
use crate::ffi::{
    AxElement, ax_bool_attribute, ax_fullscreen_state, ax_get_frame, ax_is_size_settable,
    ax_min_size, ax_set_messaging_timeout, ax_string_attribute, ax_window_id, cg_window_list_basic,
};

/// Typed AX/CG facts for one window. `None` is honest unknown (unreadable or
/// unsupported attribute), never a claim about the window. No titles, no
/// content, no owner names.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeWindowFacts {
    pub wid: u32,
    /// `AXRole` (e.g. `AXWindow`, `AXSheet`).
    pub role: Option<String>,
    /// `AXSubrole` (e.g. `AXStandardWindow`, `AXDialog`).
    pub subrole: Option<String>,
    /// Fullscreen state (`AXFullScreen`, rift-spelled `AXFullscreen`
    /// fallback). Required for classification.
    pub fullscreen: Option<bool>,
    /// `AXMinimized`. Required for classification.
    pub minimized: Option<bool>,
    /// `AXModal`. No surveyed prior art: `None` on any unsupported read.
    /// The classifier fallback stays an explicit caller decision, never a
    /// hidden default here.
    pub modal: Option<bool>,
    /// Whether `AXSize` is settable. Carried only, never consulted (TBD lane).
    pub size_settable: Option<bool>,
    /// Actually reported minimum size in points, where available.
    pub min_size: Option<(f64, f64)>,
    /// Always `None`: no surveyed source reads an AX max size (TBD).
    pub max_size: Option<(f64, f64)>,
    /// `AXPosition` + `AXSize` in screen points.
    pub frame: Option<crate::ffi::AxFrame>,
    /// `kCGWindowLayer` for the joined CG entry.
    pub cg_layer: Option<i32>,
    /// `kCGWindowIsOnscreen` for the joined CG entry.
    pub cg_onscreen: Option<bool>,
}

impl NativeWindowFacts {
    /// Classifier AX inputs. `None` while `fullscreen` or `minimized` is
    /// unknown. HONESTY (whole-snapshot rule): a `None` here must make the
    /// caller refuse the incomplete whole snapshot (see
    /// [`NativeReadOutcome::classify_all`)) — silently omitting the window
    /// and publishing the rest as a complete replacement would invent the
    /// destruction of a live window downstream. `modal_fallback` decides an
    /// unknown `modal` explicitly at the call site (pass `false` until
    /// `AXModal` live calibration lands; no surveyed source proves it).
    /// Unknown `size_settable` maps to `false` with the raw `Option`
    /// retained on `self` for the later TBD decision; the mapping is
    /// behavior-neutral today because classification carries settability
    /// without consulting it.
    #[must_use]
    pub fn to_ax_facts(&self, modal_fallback: bool) -> Option<AxFacts> {
        Some(AxFacts {
            role: self.role.clone(),
            subrole: self.subrole.clone(),
            fullscreen: self.fullscreen?,
            minimized: self.minimized?,
            modal: self.modal.unwrap_or(modal_fallback),
            size_settable: self.size_settable.unwrap_or(false),
            hints: self.size_hints(),
        })
    }

    /// Classifier CG inputs. `None` while layer or on-screen state is
    /// unknown (a missing CG join never defaults to layer 0: that would
    /// invent tileability).
    #[must_use]
    pub fn to_cg_facts(&self) -> Option<CgFacts> {
        Some(CgFacts {
            layer: self.cg_layer?,
            onscreen: self.cg_onscreen?,
        })
    }

    /// Hint vector from actually reported bounds only. Minimum rounds UP to
    /// whole units when positive and finite (ceil: never below the actual
    /// lower bound — flooring would understate the minimum and plan inside
    /// it); anything else is absent. Max is always absent (no probed
    /// source).
    #[must_use]
    pub fn size_hints(&self) -> WindowSizeHints {
        let ceil_positive = |v: f64| {
            if v.is_finite() && v > 0.0 {
                Some(v.ceil() as i32)
            } else {
                None
            }
        };
        WindowSizeHints {
            min_w: self.min_size.and_then(|(w, _)| ceil_positive(w)),
            min_h: self.min_size.and_then(|(_, h)| ceil_positive(h)),
            max_w: None,
            max_h: None,
        }
    }
}

/// Typed owner-app facts. No bundle strings cross (only the owned-bundle
/// verdict); no titles, no content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeAppFacts {
    pub pid: u32,
    /// `NSRunningApplication.isHidden`.
    pub hidden: bool,
    pub activation_policy: ActivationPolicy,
    /// Whether the owner bundle is this crate's own probe bundle. Own windows
    /// are never observed.
    pub is_own_bundle: bool,
}

impl NativeAppFacts {
    #[must_use]
    pub fn to_app_facts(&self) -> AppFacts {
        AppFacts {
            hidden: self.hidden,
            activation_policy: self.activation_policy,
            is_own_bundle: self.is_own_bundle,
        }
    }
}

/// Map a raw `NSApplicationActivationPolicy` discriminant (0=Regular,
/// 1=Accessory, 2=Prohibited) to the classifier policy. Anything else —
/// including unreadable — is `Unknown`, which never excludes on its own.
#[must_use]
pub const fn map_activation_policy(raw: i64) -> ActivationPolicy {
    match raw {
        0 => ActivationPolicy::Regular,
        1 => ActivationPolicy::Accessory,
        2 => ActivationPolicy::Prohibited,
        _ => ActivationPolicy::Unknown,
    }
}

/// One window's classifier inputs with its native facts attached. Returned
/// only by [`NativeReadOutcome::classify_all`], which refuses the whole
/// snapshot while any window is incomplete.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifiedRead {
    pub facts: NativeWindowFacts,
    pub ax: AxFacts,
    pub cg: CgFacts,
}

/// Why whole-snapshot classification refused: the first incomplete window
/// plus what it is missing. The caller preserves retained state (never an
/// empty or partial publish).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncompleteRead {
    pub wid: u32,
    pub missing: &'static str,
}

impl std::fmt::Display for IncompleteRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "wid {} incomplete ({} unknown); refusing whole snapshot",
            self.wid, self.missing
        )
    }
}

impl std::error::Error for IncompleteRead {}

/// Outcome of one bounded per-app native read. `windows` carries every
/// joined window (partial facts included; classification gates
/// completeness separately via [`Self::classify_all`]). Any failed leg fails
/// the whole read with requested/observed/failed counts plus the first
/// cause; truncation past [`crate::config::MAX_WINDOWS`] fails too (a capped
/// subset must never publish as a complete replacement).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeReadOutcome {
    pub pid: u32,
    pub requested: usize,
    pub observed: usize,
    pub failed: usize,
    pub first_cause: String,
    pub windows: Vec<NativeWindowFacts>,
}

impl NativeReadOutcome {
    /// Map every window to classifier inputs, or refuse the WHOLE snapshot
    /// on the first incomplete window. Callers must use this (not a
    /// filter-map over [`NativeWindowFacts::to_ax_facts`]): silently
    /// omitting unreadable-but-live windows and publishing the rest as
    /// complete would invent their destruction downstream.
    pub fn classify_all(
        &self,
        modal_fallback: bool,
    ) -> Result<Vec<ClassifiedRead>, IncompleteRead> {
        let mut out = Vec::with_capacity(self.windows.len());
        for facts in &self.windows {
            let missing = if facts.fullscreen.is_none() {
                Some("fullscreen")
            } else if facts.minimized.is_none() {
                Some("minimized")
            } else if facts.cg_layer.is_none() || facts.cg_onscreen.is_none() {
                Some("cg-layer/onscreen")
            } else {
                None
            };
            if let Some(missing) = missing {
                return Err(IncompleteRead {
                    wid: facts.wid,
                    missing,
                });
            }
            let (Some(ax), Some(cg)) = (facts.to_ax_facts(modal_fallback), facts.to_cg_facts())
            else {
                // Unreachable while the checks above mirror the mapping
                // gates; kept as refusal (never a partial publish) so a
                // future gate drift fails closed.
                return Err(IncompleteRead {
                    wid: facts.wid,
                    missing: "mapping",
                });
            };
            out.push(ClassifiedRead {
                facts: facts.clone(),
                ax,
                cg,
            });
        }
        Ok(out)
    }
}

fn messaging_secs() -> f32 {
    crate::runtime_types::AX_CALL_TIMEOUT.as_secs_f32().max(0.5)
}

/// Read owner-app facts on the caller (main) thread: hidden state plus
/// activation policy over public AppKit plus the own-bundle verdict. No AX,
/// no titles. Unknown pids fail closed.
///
/// The scoped backend also calls this on its owning per-app AX thread (one
/// justified off-main exception): every read underneath is an
/// `NSRunningApplication` model-object lookup with no view, window,
/// runloop, or notification involvement. All other AppKit/UI reads stay
/// main-thread-only.
pub fn read_app_facts(pid: u32) -> Result<NativeAppFacts, String> {
    if pid <= 1 || pid > i32::MAX as u32 {
        return Err(format!("refusing out-of-range pid {pid}"));
    }
    let pid_i32 = pid as i32;
    let hidden = crate::ffi::app_hidden_for_pid(pid_i32)?;
    let policy_raw = crate::ffi::activation_policy_raw_for_pid(pid_i32)?;
    let is_own_bundle = match crate::ffi::bundle_id_for_pid(pid_i32) {
        Some(bundle) => bundle == crate::BUNDLE_ID,
        // Directly spawned helpers have no LaunchServices bundle entry; a
        // live PID with no bundle defers to the record (same shape as the
        // existing owned gates).
        None if crate::ffi::process_exists(pid_i32) => true,
        None => return Err(format!("no live bundle for pid {pid}")),
    };
    Ok(NativeAppFacts {
        pid,
        hidden,
        activation_policy: map_activation_policy(policy_raw),
        is_own_bundle,
    })
}

/// Bound check for a window-list read: a list past
/// [`crate::config::MAX_WINDOWS`] fails outright. A truncated subset must
/// never publish as a complete replacement (the missing tail would invent
/// destruction downstream). Pure: unit-tested without live state.
pub(crate) fn check_window_bound(requested: usize) -> Result<(), String> {
    if requested > crate::config::MAX_WINDOWS {
        return Err(format!(
            "requested {requested} exceeds bound {}: refusing truncated subset",
            crate::config::MAX_WINDOWS
        ));
    }
    Ok(())
}
/// Read one app's window facts on the calling (per-app AX owner) thread.
/// Every `AxElement` is constructed, used, and dropped here; only owned
/// plain data returns. CG owner/layer/on-screen join by exact `(wid, pid)`
/// over the title-free list; a missing join leaves CG facts unknown (the
/// classification gate refuses the snapshot, it never defaults to layer 0).
pub fn read_app_window_facts(pid: u32) -> Result<NativeReadOutcome, String> {
    if pid <= 1 || pid > i32::MAX as u32 {
        return Err(format!("refusing out-of-range pid {pid}"));
    }
    let pid_i32 = pid as i32;
    let Some(app) = AxElement::application(pid_i32) else {
        return Err(format!("no AX element for pid {pid}"));
    };
    if ax_set_messaging_timeout(&app, messaging_secs()) != 0 {
        return Err(format!(
            "pid {pid}: messaging timeout unset before window join"
        ));
    }
    let windows = app
        .windows()
        .map_err(|code| format!("pid {pid}: windows: {}", crate::ffi::ax_error_name(code)))?;
    // Bound BEFORE any join: an over-bound list fails outright (retained
    // state preserved), never a silently truncated subset.
    check_window_bound(windows.len())?;
    // Title-free CG list once per read; an unreadable list leaves every
    // window's CG facts unknown (gated later), it never fails the AX legs.
    let cg_list = cg_window_list_basic().unwrap_or_default();
    let requested = windows.len();
    let mut out = Vec::new();
    let mut failed = 0usize;
    let mut first_cause = String::new();
    let mut fail = |cause: String| {
        failed += 1;
        if first_cause.is_empty() {
            first_cause = cause;
        }
    };
    for window in windows.iter() {
        // Finite timeout before the join and every read on this object; a
        // refusal skips the narrow read, never an unbounded one.
        if ax_set_messaging_timeout(window, messaging_secs()) != 0 {
            fail("messaging timeout unset".to_owned());
            continue;
        }
        let Some(wid) = ax_window_id(window) else {
            fail("window identity join failed".to_owned());
            continue;
        };
        let frame = match ax_get_frame(window) {
            Ok(frame) => Some(frame),
            Err(code) => {
                fail(crate::ffi::ax_error_name(code));
                continue;
            }
        };
        // Fact legs carry unknowns as `None`; only frame/identity fail the
        // window. The min leg distinguishes unsupported (stays `None`,
        // advisory) from transient failures (fail the window, hence the
        // read, preserving retained state); classification gates
        // completeness for the rest via `classify_all`.
        let joined_cg = cg_list.iter().find(|w| w.wid == wid && w.pid == pid);
        let min_size = match ax_min_size(window) {
            Ok(bound) => bound,
            Err(code) => {
                fail(crate::ffi::ax_error_name(code));
                continue;
            }
        };
        out.push(NativeWindowFacts {
            wid,
            role: ax_string_attribute(window, "AXRole").ok().flatten(),
            subrole: ax_string_attribute(window, "AXSubrole").ok().flatten(),
            fullscreen: ax_fullscreen_state(window).ok(),
            minimized: ax_bool_attribute(window, "AXMinimized").ok(),
            modal: ax_bool_attribute(window, "AXModal").ok(),
            size_settable: ax_is_size_settable(window).ok(),
            min_size,
            max_size: None,
            frame,
            cg_layer: joined_cg.map(|w| w.layer),
            cg_onscreen: joined_cg.map(|w| w.onscreen),
        });
    }
    if failed > 0 {
        return Err(format!(
            "pid {pid}: requested {requested} observed {} failed {failed}: {first_cause}",
            out.len()
        ));
    }
    Ok(NativeReadOutcome {
        pid,
        requested,
        observed: out.len(),
        failed,
        first_cause,
        windows: out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classifier::{ClassifiedWindow, Disposition, classify_window};

    fn full_facts() -> NativeWindowFacts {
        NativeWindowFacts {
            wid: 7,
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            fullscreen: Some(false),
            minimized: Some(false),
            modal: Some(false),
            size_settable: Some(true),
            min_size: None,
            max_size: None,
            frame: Some(crate::ffi::AxFrame {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            }),
            cg_layer: Some(0),
            cg_onscreen: Some(true),
        }
    }

    #[test]
    fn incomplete_reads_do_not_publish() {
        // Unknown fullscreen or minimized never maps: the gate returns None
        // so no subset publishes as a complete replacement.
        let mut facts = full_facts();
        facts.fullscreen = None;
        assert!(facts.to_ax_facts(false).is_none());
        facts.fullscreen = Some(false);
        facts.minimized = None;
        assert!(facts.to_ax_facts(false).is_none());
        // Unknown CG join never defaults to layer 0.
        facts.minimized = Some(false);
        facts.cg_layer = None;
        assert!(facts.to_cg_facts().is_none());
        facts.cg_layer = Some(0);
        facts.cg_onscreen = None;
        assert!(facts.to_cg_facts().is_none());
    }

    #[test]
    fn modal_fallback_is_explicit_at_the_call_site() {
        let mut facts = full_facts();
        facts.modal = None;
        let assume_plain = facts.to_ax_facts(false).expect("maps with fallback");
        assert!(!assume_plain.modal);
        let assume_modal = facts.to_ax_facts(true).expect("maps with fallback");
        assert!(assume_modal.modal);
        facts.modal = Some(true);
        assert!(facts.to_ax_facts(false).expect("known modal").modal);
    }

    #[test]
    fn settability_is_carried_never_consulted() {
        // TBD lane pin at the mapping boundary: either settability maps to
        // byte-identical classifier observations (see
        // `classifier::settable_only_lane_tiles_identically_either_way`).
        let mut settable = full_facts();
        let mut unsettable = full_facts();
        settable.size_settable = Some(true);
        unsettable.size_settable = Some(false);
        let cg = CgFacts::normal();
        let app = AppFacts::regular();
        let frame = crate::classifier::WindowFrame {
            wid: 7,
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let a: ClassifiedWindow = classify_window(
            frame,
            &settable.to_ax_facts(false).expect("complete"),
            &cg,
            &app,
        );
        let b: ClassifiedWindow = classify_window(
            frame,
            &unsettable.to_ax_facts(false).expect("complete"),
            &cg,
            &app,
        );
        assert_eq!(a.disposition, Disposition::Tile);
        assert_eq!(a.to_observed(), b.to_observed());
        // Unknown settability maps (documented `false`) while the raw fact
        // stays unknown on the struct for the later decision.
        unsettable.size_settable = None;
        let mapped = unsettable.to_ax_facts(false).expect("maps");
        assert!(!mapped.size_settable);
        assert_eq!(unsettable.size_settable, None);
    }

    #[test]
    fn min_size_fills_hints_max_stays_absent() {
        let mut facts = full_facts();
        // Ceil, never floor: the hint must not understate the actual lower
        // bound (planning inside the true minimum would drift).
        facts.min_size = Some((640.7, 480.2));
        let hints = facts.size_hints();
        assert_eq!((hints.min_w, hints.min_h), (Some(641), Some(481)));
        // Exact integers pass through unchanged.
        facts.min_size = Some((640.0, 480.0));
        let hints = facts.size_hints();
        assert_eq!((hints.min_w, hints.min_h), (Some(640), Some(480)));
        assert_eq!((hints.max_w, hints.max_h), (None, None));
        // Non-positive/non-finite minimums are absent, never zero hints.
        for bad in [
            Some((0.0, 480.0)),
            Some((-1.0, 480.0)),
            Some((f64::NAN, 480.0)),
            Some((f64::INFINITY, 480.0)),
            None,
        ] {
            facts.min_size = bad;
            let hints = facts.size_hints();
            if bad.is_none() {
                assert_eq!(hints, WindowSizeHints::none());
            } else {
                assert_eq!(hints.min_w, None, "bad min {bad:?}");
            }
        }
        // Max is never probed: always absent even with a minimum present.
        facts.min_size = Some((640.0, 480.0));
        facts.max_size = None;
        assert!(facts.size_hints().max_w.is_none());
        assert!(facts.size_hints().max_h.is_none());
    }

    #[test]
    fn activation_policy_mapping_matches_ns_values() {
        assert_eq!(map_activation_policy(0), ActivationPolicy::Regular);
        assert_eq!(map_activation_policy(1), ActivationPolicy::Accessory);
        assert_eq!(map_activation_policy(2), ActivationPolicy::Prohibited);
        for raw in [-1, 3, 99, i64::MIN, i64::MAX] {
            assert_eq!(
                map_activation_policy(raw),
                ActivationPolicy::Unknown,
                "raw {raw}"
            );
        }
    }

    #[test]
    fn app_facts_map_without_bundle_strings() {
        let native = NativeAppFacts {
            pid: 4242,
            hidden: false,
            activation_policy: ActivationPolicy::Regular,
            is_own_bundle: false,
        };
        let app = native.to_app_facts();
        assert!(!app.hidden);
        assert!(!app.is_own_bundle);
        assert_eq!(app.activation_policy, ActivationPolicy::Regular);
    }

    #[test]
    fn readers_refuse_reserved_pids_before_any_native_touch() {
        assert!(read_app_facts(0).is_err());
        assert!(read_app_facts(1).is_err());
        assert!(read_app_window_facts(0).is_err());
        assert!(read_app_window_facts(1).is_err());
        assert!(read_app_window_facts(u32::MAX).is_err());
    }

    #[test]
    fn over_bound_window_lists_refuse_truncation() {
        // A capped subset must never publish as complete: past the bound
        // the read fails outright (retained state preserved downstream).
        assert!(check_window_bound(0).is_ok());
        assert!(check_window_bound(crate::config::MAX_WINDOWS).is_ok());
        let err = check_window_bound(crate::config::MAX_WINDOWS + 1).unwrap_err();
        assert!(err.contains("refusing truncated subset"), "{err}");
    }

    fn complete_outcome() -> NativeReadOutcome {
        NativeReadOutcome {
            pid: 4242,
            requested: 1,
            observed: 1,
            failed: 0,
            first_cause: String::new(),
            windows: vec![full_facts()],
        }
    }

    #[test]
    fn classify_all_maps_complete_snapshots() {
        let outcome = complete_outcome();
        let mapped = outcome.classify_all(false).expect("complete maps");
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].facts.wid, 7);
        assert!(!mapped[0].ax.fullscreen);
        assert_eq!(mapped[0].cg.layer, 0);
    }

    #[test]
    fn classify_all_refuses_whole_snapshot_on_first_gap() {
        // Each required unknown names its window and missing fact; nothing
        // partial publishes (no silent filter-map omission).
        let mut outcome = complete_outcome();
        outcome.windows[0].fullscreen = None;
        let err = outcome.classify_all(false).unwrap_err();
        assert_eq!((err.wid, err.missing), (7, "fullscreen"));

        let mut outcome = complete_outcome();
        outcome.windows[0].minimized = None;
        let err = outcome.classify_all(false).unwrap_err();
        assert_eq!((err.wid, err.missing), (7, "minimized"));

        let mut outcome = complete_outcome();
        outcome.windows[0].cg_layer = None;
        let err = outcome.classify_all(false).unwrap_err();
        assert_eq!((err.wid, err.missing), (7, "cg-layer/onscreen"));
        assert!(err.to_string().contains("refusing whole snapshot"));

        // Unknown modal still maps via the explicit fallback (never a gap).
        let mut outcome = complete_outcome();
        outcome.windows[0].modal = None;
        assert_eq!(outcome.classify_all(false).expect("maps").len(), 1);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn off_macos_readers_fail_closed_without_native_state() {
        // Stub AX builds no elements and the CG list is unsupported: both
        // readers fail with named causes, never empty claims.
        assert!(read_app_window_facts(4242).is_err());
        assert!(read_app_facts(4242).is_err());
    }
}
