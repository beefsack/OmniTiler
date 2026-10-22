//! Retained last-known focused output for fresh-window homing.
//!
//! The adapter-observed foreground can go missing exactly when it matters:
//! a newcomer steals it between the foreground sample and the enumeration,
//! or the sample lands in the handoff gap and reports none/unknown. The
//! focused-output placement (i3/sway) would then fall back to primary and
//! mis-home the newcomer onto the cursor output. This module retains the
//! last output that carried genuine managed focus (a previously retained
//! member homing) across accepted reports, so fresh admission prefers that
//! pre-admission context over a brand-new native fact or the primary
//! fallback. Scope is fresh placement only: switch targeting, parked
//! empty-workspace focus, and float-home parking are untouched.

use crate::directional::{OutputId, WindowId};
use crate::world_engine::WorldEngine;
use crate::world_report::WorldReport;

/// Why the fresh focused leg resolved the way it did (trace-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FreshFocusBasis {
    /// Genuine managed focus on this report (previously retained homing).
    Current,
    /// Retained last-known focused output; the current foreground was
    /// missing, unknown, or a brand-new lifetime.
    Retained,
    /// Truthful brand-new native fact; no retained history applied.
    Native,
    /// Safe primary/first fallback; every focus source unknown.
    #[default]
    Primary,
}

impl FreshFocusBasis {
    /// Stable wire token for the trace row.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Retained => "retained",
            Self::Native => "native",
            Self::Primary => "primary",
        }
    }
}

/// Trace-only record of one fresh focused-leg routing decision: stable
/// output ids plus the winning basis. No titles, pids, frames, or input.
/// Recomputed per fresh admission; the last decision stands as a cached
/// historical leg for the adapter trace row, never as the current report's
/// focus fact (the adapter reports the current token alongside it).
#[derive(Debug, Clone, Default)]
pub(crate) struct FreshFocusTrace {
    /// Current managed resolution (retained-homing focus), if any.
    pub current: Option<OutputId>,
    /// Retained last-known output consulted for this decision, if any.
    pub retained: Option<OutputId>,
    /// Chosen focused-leg output, before the primary fallback.
    pub chosen: Option<OutputId>,
    /// Why the leg resolved this way.
    pub basis: FreshFocusBasis,
}

/// Retained last-known focused output plus the last routing trace. The
/// latch updates only from genuine managed focus; closes never clear it
/// (output context survives until topology removal filters it at use),
/// and an adapter incarnation reset clears it outright.
#[derive(Debug, Clone, Default)]
pub(crate) struct FreshFocusMemory {
    pub last_output: Option<OutputId>,
    pub trace: FreshFocusTrace,
}

impl WorldEngine {
    /// Current managed focused output: the adapter-observed foreground,
    /// but only when it names a previously retained member homing. A
    /// brand-new lifetime (no retained homing yet) is not managed focus,
    /// even when its native fact sits on a known screen.
    pub(crate) fn managed_focused_output(&self, report: &WorldReport) -> Option<OutputId> {
        if report.focused_global.is_empty() {
            return None;
        }
        let active = WindowId(report.focused_global.clone());
        self.core_homing(&active).map(|key| key.output)
    }

    /// Fresh-placement focused leg: current managed focus wins; else the
    /// retained last-known output while it still names a live screen;
    /// else the truthful brand-new native fact; else `None` and the
    /// caller falls back to primary. Never invents focus.
    pub(crate) fn fresh_focused_output(
        &self,
        report: &WorldReport,
    ) -> (Option<OutputId>, FreshFocusTrace) {
        let current = self.managed_focused_output(report);
        let retained = self.fresh_focus.last_output.clone();
        if let Some(output) = current.clone() {
            return (
                Some(output.clone()),
                FreshFocusTrace {
                    current: Some(output.clone()),
                    retained,
                    chosen: Some(output),
                    basis: FreshFocusBasis::Current,
                },
            );
        }
        if let Some(output) = retained.clone()
            && report.screens.iter().any(|screen| screen.output == output)
        {
            return (
                Some(output.clone()),
                FreshFocusTrace {
                    current: None,
                    retained: retained.clone(),
                    chosen: Some(output),
                    basis: FreshFocusBasis::Retained,
                },
            );
        }
        let native = self.focused_output(report);
        let basis = if native.is_some() {
            FreshFocusBasis::Native
        } else {
            FreshFocusBasis::Primary
        };
        (
            native.clone(),
            FreshFocusTrace {
                current: None,
                retained,
                chosen: native,
                basis,
            },
        )
    }

    /// Retain the last-known focused output from an accepted report. Only
    /// genuine managed focus (previously retained homing on a live screen)
    /// latches: a stealing newcomer, an empty report, or an unknown token
    /// never overwrites pre-admission context. Call before membership
    /// converges so the latch reflects strictly prior retention.
    pub(crate) fn note_focused_output(&mut self, report: &WorldReport) {
        if let Some(output) = self.managed_focused_output(report)
            && report.screens.iter().any(|screen| screen.output == output)
        {
            self.fresh_focus.last_output = Some(output);
        }
    }
}
