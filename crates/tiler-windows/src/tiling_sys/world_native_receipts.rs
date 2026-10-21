#![forbid(unsafe_code)]
//! Truthful applied-receipt capture for the world route.
//!
//! [`attach_workspace_receipts`] and [`observed_view_receipts`] derive the
//! capture's per-window `observed_workspace` and per-screen `observed_views`
//! from fresh visibility plus the owner tables. Only exactly-one-visible-
//! workspace (non-sticky) views prove, plus the memberless-active-with-zero-
//! visible batch-completion case; an empty workspace never proves by echoing
//! desired state. Unknown stays unknown so the core retains pending flights
//! instead of settling on a guess.
//!
//! Owner route: [`capture_world`](super::world_capture::capture_world) calls
//! both functions every turn; the actuator
//! ([`LiveNative`](super::world_native::LiveNative)) never fabricates
//! receipts itself.

use super::world_capture::OwnerInputs;
use super::{ObservedWindow, RetainedRow};
use crate::native_guards::WindowKey;
use std::collections::{HashMap, HashSet};
use tiler_core::world_report::ScreenView;
/// Per-window applied-membership receipt for capture: the stable logical
/// workspace only where fresh identity/visibility proves it. Visible
/// windows prove through a matching live observed row plus a stamped
/// lifetime tag plus screen-affinity arrival on the applied output (plain
/// visibility never proves cross-output arrival); hidden windows prove
/// through the claim association (well formed tag, claim identity equals
/// the member key, tag present) plus continued invisibility. Anything else
/// is unknown (`None`), never a proof of absence. Hidden, fullscreen,
/// minimized, and unreadable truth preserved; omission never a close.
pub(super) fn attach_workspace_receipts(
    inputs: &OwnerInputs<'_>,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[super::MonitorArea],
    facts: &mut [tiler_core::world_report::WindowFact],
) {
    let by_token: HashMap<&str, &WindowKey> = inputs
        .member_tokens
        .iter()
        .map(|(key, token)| (token.as_str(), key))
        .collect();
    let affinity_of = |rect: &tiler_core::geometry::Rect| -> Option<String> {
        let device = super::directional_resolution::output_for_rect(areas, rect);
        inputs.output_tokens.get(&device).cloned()
    };
    for fact in facts.iter_mut() {
        let Some(key) = by_token.get(fact.window.0.as_str()).copied() else {
            fact.observed_workspace = None;
            continue;
        };
        // Retained proven homing: receipts prove this commitment with fresh
        // evidence, never a policy-table read and never core desire.
        let Some((applied_out, applied_ws)) = inputs.applied_homing.get(fact.window.0.as_str())
        else {
            fact.observed_workspace = None;
            continue;
        };
        let hidden = inputs.hidden_claims.contains_key(key);
        let tag_ok = inputs
            .member_tags
            .get(key)
            .is_some_and(|tag| !tag.is_empty());
        if !tag_ok {
            fact.observed_workspace = None;
            continue;
        }
        if hidden {
            let proven = inputs.hidden_claims.get(key).is_some_and(|record| {
                crate::model::valid_claim_tag(&record.claim.tag)
                    && record.claim.hwnd == key.hwnd
                    && record.claim.process.pid == key.pid
                    && record.claim.process.process_creation == key.creation
            });
            // External reveal or HWND-recycle drift (any live observation of
            // the number while logically hidden) proves nothing: unknown,
            // never the stale homing. Same-pid creation drift is drift too,
            // so the check is HWND-only, never identity-gated.
            let drifted = observed.iter().any(|w| w.hwnd == key.hwnd);
            fact.observed_workspace = if proven && !drifted {
                Some(applied_ws.clone())
            } else {
                None
            };
            continue;
        }
        // Visible lane: a matching live observed row with full identity plus
        // a stamped lifetime tag plus affinity on the applied output, or a
        // retained row with a fresh frame on the applied output (overlay
        // members ride retained rows). Floating logical crossings preserve
        // the physical frame, so floating facts skip the affinity check:
        // the identity plus preserved visibility proves the logical
        // receipt while the physical screen stays put (the core homes
        // floating receipts on their owning screen). Stale,
        // tag-mismatched, token-mismatched, or wrong-output rows prove
        // nothing.
        let stored_tag = inputs.member_tags.get(key).cloned().unwrap_or_default();
        let expected = inputs.member_identity.get(key).cloned();
        let live_observed = observed.iter().any(|w| {
            w.hwnd == key.hwnd
                && w.identity.pid == key.pid
                && w.identity.process_creation == key.creation
                && w.token == fact.window.0
                // Membership stamp lane: the observed row must carry the
                // FRESH visible-membership tag read with this observation,
                // equal to the stored stamp. The proof-only owned-helper
                // token (`identity.tag`, empty for generic windows) never
                // proves managed membership here, no matter its value.
                && !stored_tag.is_empty()
                && w.member_tag.as_deref() == Some(stored_tag.as_str())
                && expected.as_ref().is_some_and(|ident| {
                    w.identity.exe_path == ident.exe_path
                        && w.identity.user_sid == ident.user_sid
                        && w.identity.session_id == ident.session_id
                })
                && (fact.floating
                    || affinity_of(&w.visible).as_deref() == Some(applied_out.as_str()))
        });
        // Retained overlay lane: the row must carry FRESH observed facts
        // (frameless known rows prove nothing), match the bound lifetime
        // key plus token, sit on the applied output, and belong to a stamped
        // managed lifetime. Freshness comes from this tick's observation of
        // that key; a recycled HWND fails the key match first.
        let live_retained = !live_observed
            && !stored_tag.is_empty()
            && retained.iter().any(|row| {
                row.key == *key
                    && row.token == fact.window.0
                    && row.facts.is_some()
                    && row.rect.is_some_and(|rect| {
                        affinity_of(&rect).as_deref() == Some(applied_out.as_str())
                    })
            });
        fact.observed_workspace = if live_observed || live_retained {
            Some(applied_ws.clone())
        } else {
            None
        };
    }
}

/// Per-screen applied-view receipts: exactly one non-sticky applied
/// workspace with fresh visible members proves the applied view; a
/// memberless applied view with zero visible members on the output proves
/// batch completion only with explicit prior proof (never inferred over
/// zero windows). Sticky members ride every view and are never evidence.
/// Unknown stays unknown: the core retains pending flights rather than
/// settling on an echo of desire. Proven against the adapter-retained
/// last applied views with native observation revalidation, never a
/// policy-table read and never core desire.
pub(super) fn observed_view_receipts(
    inputs: &OwnerInputs<'_>,
    observed: &[ObservedWindow],
    areas: &[super::MonitorArea],
) -> Vec<ScreenView> {
    // Visible members grouped by (output, applied workspace), gated on
    // screen-affinity arrival: plain visibility never proves cross-output
    // arrival, and minimized/unreadable rows never count.
    let mut visible: HashSet<(String, String)> = HashSet::new();
    for w in observed {
        if w.facts.minimized {
            continue;
        }
        let key = WindowKey {
            hwnd: w.hwnd,
            pid: w.identity.pid,
            creation: w.identity.process_creation.clone(),
        };
        if inputs.sticky.contains_key(&key) {
            continue;
        }
        let Some(token) = inputs.member_tokens.get(&key) else {
            continue;
        };
        if token != &w.token {
            continue;
        }
        // Full creation plus membership-stamp proof: the observed row must
        // carry the fresh visible-membership tag matching the stored stamp,
        // plus the expected process identity, so a same-HWND recycle never
        // proves the prior view. The proof-only owned-helper token
        // (`identity.tag`) is never consulted here.
        let Some(stored) = inputs.member_tags.get(&key) else {
            continue;
        };
        if stored.is_empty() || w.member_tag.as_deref() != Some(stored.as_str()) {
            continue;
        }
        let Some(expected) = inputs.member_identity.get(&key) else {
            continue;
        };
        if w.identity.exe_path != expected.exe_path
            || w.identity.user_sid != expected.user_sid
            || w.identity.session_id != expected.session_id
        {
            continue;
        }
        let Some((applied_out, applied_ws)) = inputs.applied_homing.get(token.as_str()) else {
            continue;
        };
        // Screen-affinity arrival: the live frame must sit on the applied
        // output, or a cross-output visible window would prove the wrong view.
        let device = super::directional_resolution::output_for_rect(areas, &w.visible);
        if inputs.output_tokens.get(&device) != Some(applied_out) {
            continue;
        }
        visible.insert((applied_out.clone(), applied_ws.clone()));
    }
    let mut out = Vec::new();
    for (output, active) in inputs.applied_views {
        let shown: Vec<&(String, String)> =
            visible.iter().filter(|(dev, _)| dev == output).collect();
        if shown.is_empty() {
            // Memberless-applied batch completion only: no visible member on
            // the whole output while the applied view holds nothing.
            let holds_anything = inputs.applied_homing.values().any(|(dev, _)| dev == output);
            if !holds_anything {
                out.push(ScreenView {
                    output: tiler_core::directional::OutputId(output.clone()),
                    view: active.clone(),
                });
            }
            continue;
        }
        if shown.len() == 1 && shown[0].1 == *active {
            out.push(ScreenView {
                output: tiler_core::directional::OutputId(output.clone()),
                view: active.clone(),
            });
        }
    }
    out
}
