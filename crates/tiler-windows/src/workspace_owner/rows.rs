#![forbid(unsafe_code)]
use super::*;

/// One planned geometry write extracted from an Engine reply: the portable
/// shape the native write path consumes. Workspace identity stays with the
/// caller; scoping rides the domain writable set, never a workspace-id
/// partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWrite {
    pub window: WindowId,
    pub rect: Rect,
    pub overconstrained: bool,
    pub client_clamped: bool,
}

/// Portable Engine-reply extraction used by the actual native write path.
/// Returns `None` for non-plan replies so callers serialize null with reason
/// instead of zeros. Both-domain `SendWorkspace`/`SendOutput` plans carry
/// source reflow and target placement together; the domain writable set
/// scopes each pass.
#[must_use]
pub fn planned_writes(reply: &tiler_core::boundary::CoreReply) -> Option<Vec<PlannedWrite>> {
    fn map(geometry: &[tiler_core::session::DesiredGeometry]) -> Vec<PlannedWrite> {
        geometry
            .iter()
            .map(|g| PlannedWrite {
                window: g.window.clone(),
                rect: g.rect,
                overconstrained: g.overconstrained,
                client_clamped: g.client_clamped,
            })
            .collect()
    }
    use tiler_core::boundary::CoreReply as R;
    match reply {
        R::Projection(plan) => Some(map(&plan.geometry)),
        R::Tiled(plan) => Some(map(&plan.geometry)),
        R::Resize(plan) => Some(map(&plan.geometry)),
        R::MoveDirectional(plan) => Some(map(&plan.geometry)),
        R::FocusDirectional(plan) => Some(map(&plan.geometry)),
        R::SendWorkspace(plan) => Some(map(&plan.geometry)),
        R::SendOutput(plan) => Some(map(&plan.geometry)),
        R::MigrateWorkspace(plan) => Some(map(&plan.geometry)),
        _ => None,
    }
}

/// Portable writable-token subset: non-hidden members whose Engine token is
/// present in the fresh eligible observation. Retained members without a
/// fresh frame and hidden members never take geometry writes; hidden
/// workspace geometry waits for reveal. Scope and identity revalidation stay
/// with the native caller; this pins the hidden/retained exclusion.
#[must_use]
pub fn writable_subset(
    members: &BTreeSet<WindowKey>,
    is_hidden: impl Fn(&WindowKey) -> bool,
    token_of: &BTreeMap<WindowKey, String>,
    fresh_tokens: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    members
        .iter()
        .filter(|k| !is_hidden(k))
        .filter_map(|k| token_of.get(k))
        .filter(|t| fresh_tokens.contains(*t))
        .cloned()
        .collect()
}

/// Hidden snapshot rectangle for one member: intentional floats ride the live
/// float snapshot with the tiled allocation as fallback; every other member
/// (including born holds, which never seed a float snapshot) rides the tiled
/// allocation with the float snapshot as fallback. `None` only when both are
/// missing, so row assembly drops nothing it could keep.
#[must_use]
pub const fn hidden_snapshot_rect(
    is_float: bool,
    float_rect: Option<Rect>,
    member_rect: Option<Rect>,
) -> Option<Rect> {
    if is_float {
        match float_rect {
            Some(rect) => Some(rect),
            None => member_rect,
        }
    } else {
        match member_rect {
            Some(rect) => Some(rect),
            None => float_rect,
        }
    }
}

/// Pure owner dispatch gate for one workspace digit, shared by the native
/// loop and offline tests. `takeover` is the `--no-keyboard-snap-takeover`
/// switch (off disables all product interception); `suspended` covers
/// fullscreen suspension, managed gestures, and gesture settle; `send`
/// requires a managed focused member while select also serves empty
/// workspaces and unmanaged foreground.
#[must_use]
pub const fn dispatch_decision(
    takeover: bool,
    suspended: bool,
    send: bool,
    managed_focus: bool,
) -> Option<&'static str> {
    if !takeover {
        return Some("takeover-off");
    }
    if suspended {
        return Some("suspended");
    }
    if send && !managed_focus {
        return Some("unmanaged");
    }
    None
}

/// Membership snapshot helper: group tokens by domain for observation
/// assembly without touching native state.
#[must_use]
pub fn rows_for(
    members: &BTreeMap<String, OwnerRow>,
    workspace_members: &BTreeSet<WindowKey>,
    token_of: &BTreeMap<WindowKey, String>,
) -> Vec<OwnerRow> {
    let mut rows = Vec::new();
    for member in workspace_members {
        if let Some(token) = token_of.get(member)
            && let Some(row) = members.get(token)
        {
            rows.push(row.clone());
        }
    }
    rows.sort_by(|a, b| a.token.cmp(&b.token));
    rows
}

/// One member's portable view for domain-row assembly: stable session key,
/// Engine token, best-known rectangle (fresh visible read, fresh retained
/// frame, or hidden snapshot), fresh minimum-size hint (eligible visible
/// reads plus verified hidden snapshots; retained rows carry no hint), and
/// the slotless floating exception flag (intentional float or born-held
/// fullscreen, never a native write).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberView {
    pub key: WindowKey,
    pub token: String,
    pub rect: Rect,
    pub hints: tiler_core::size_hints::WindowSizeHints,
    pub floating: bool,
}

/// Assemble complete Engine observation rows for one workspace domain from
/// the union of eligible visible views, retained-occupancy views
/// (minimized/maximized/fullscreen with last-known frames), and hidden
/// snapshots. Sorted by token for deterministic fingerprints; hidden Engine
/// membership survives because every retained member rides a row.
///
/// Returns `None` when any member lacks a view (unknown snapshot): the
/// caller must defer every Engine handle path with retained state instead of
/// presenting a falsely complete observation that would drop membership.
#[must_use]
pub fn domain_rows(
    workspace_members: &BTreeSet<WindowKey>,
    views: &[MemberView],
) -> Option<Vec<OwnerRow>> {
    let mut rows = Vec::new();
    for member in workspace_members {
        let view = views.iter().find(|v| v.key == *member)?;
        rows.push(OwnerRow {
            token: view.token.clone(),
            rect: view.rect,
            hints: view.hints,
            floating: view.floating,
        });
    }
    rows.sort_by(|a, b| a.token.cmp(&b.token));
    Some(rows)
}
