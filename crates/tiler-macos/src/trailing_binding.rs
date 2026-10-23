#![forbid(unsafe_code)]
//! Unit-boundary note: the record surface (`bind_exact_append`,
//! `unbound_trailing_token`) is production-wired through the dispatch
//! verdict (token delivery) and the engine `TrailingProof` lane (proof
//! recording) - armed through the trailing capability gate, so callers
//! record only through the fenced production route. The carry surface
//! (`report_bindings`, `prune_vanished`, `note_ack`) is production wired
//! through [`crate::world_runtime`] and [`crate::world_dispatch`].

//! Explicit logical/native binding for trailing native-created Spaces.
//!
//! The shared core mints a stable pending logical token (`wl-N`) for a
//! trailing creation (see [`tiler_core::world_engine::WorldEngine`]
//! `pending_token`) and publishes it as an unbound workspace row
//! (`native_binding: None`) until the adapter reports the binding that
//! resolves it. This module owns the adapter side of that contract:
//! identifying the actual token from the engine's unbound desire,
//! tying it to the exact same-output native append proof, and carrying
//! the binding in full world reports until the core acknowledges it.
//!
//! Binding rule (no guesses, no arbitrary mapping): a binding records the
//! engine-minted `wl-` token against the SID the native append proof just
//! adopted - the `after` row must be exactly `before` plus the created SID
//! (same display, same current, strict prefix, length + 1, created last).
//! Mapping an already-present Space, a zero SID, a drifted row, or a
//! non-`wl-` token (including the generic native-to-native fallback echo
//! shape, where logical equals native) refuses instead of publishing.
//!
//! Lifetime: bindings persist across reports until the core acknowledges
//! them (the reply publishes the bound row) or the native vanishes from
//! live topology (pruned, never re-removed). Ownership of the underlying
//! Space stays with [`crate::workspace_select::TrailingLifecycle`]; this
//! store only carries the logical/native proof. Capability gating,
//! subject/lifetime/source pinning, and per-edge topology fences stay at
//! the existing drains - this module never actuates.

use std::collections::BTreeMap;

use tiler_core::bounds::is_opaque_id;
use tiler_core::directional::OutputId;
use tiler_core::world_report::{WorkspaceBinding, WorldReply};

/// One proven trailing binding: the engine's pending logical token tied
/// to the exact-append native Space on the same output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailingBinding {
    pub logical_id: String,
    pub output: OutputId,
    pub created_sid: u64,
    pub native_id: String,
}

/// Adapter-side trailing bindings awaiting core acknowledgement. At most
/// one pending creation exists per output core-side, so this stays tiny.
#[derive(Debug, Clone, Default)]
pub struct TrailingBindingStore {
    bindings: Vec<TrailingBinding>,
}

impl TrailingBindingStore {
    /// True when no binding is outstanding. Test-only today (production
    /// carries bindings opaquely through the report/ack path); kept for
    /// the coverage the unit regressions own.
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Number of outstanding bindings. Test-only today (same note as
    /// [`TrailingBindingStore::is_empty`]).
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Record the engine token against the exact-append native proof.
    ///
    /// `logical_id` must be the actual `wl-` token from the engine's
    /// unbound workspace desire (see [`unbound_trailing_token`]), `output`
    /// the same output the desire names, `created_sid` the SID the native
    /// append proof adopted, and `before`/`after` the native rows around
    /// that single append. Any drift - created already present, row not an
    /// exact append, zero SID, duplicate token, or non-token logical -
    /// refuses without mutating the store.
    pub fn bind_exact_append(
        &mut self,
        logical_id: &str,
        output: &OutputId,
        created_sid: u64,
        before: &[u64],
        after: &[u64],
    ) -> Result<(), String> {
        if !is_opaque_id(logical_id) || !logical_id.starts_with("wl-") {
            return Err(
                "trailing binding refuses: logical token is not an engine pending token; refusing"
                    .to_owned(),
            );
        }
        if !is_opaque_id(&output.0) {
            return Err(
                "trailing binding refuses: output identity unprovable; refusing".to_owned(),
            );
        }
        if created_sid == 0 {
            return Err("trailing binding refuses: zero SID unadoptable; refusing".to_owned());
        }
        let native_id = crate::topology::workspace_key(created_sid);
        if logical_id == native_id {
            return Err(
                "trailing binding refuses: fallback echo carries no ownership; refusing".to_owned(),
            );
        }
        if before.is_empty() || before.contains(&0) || before.contains(&created_sid) {
            return Err(
                "trailing binding refuses: created SID is not a fresh append; refusing".to_owned(),
            );
        }
        if after.len() != before.len() + 1
            || !after.starts_with(before)
            || *after.last().expect("appended row") != created_sid
        {
            return Err(
                "trailing binding refuses: create effect is not an exact same-output append; refusing"
                    .to_owned(),
            );
        }
        if self
            .bindings
            .iter()
            .any(|binding| binding.logical_id == logical_id)
        {
            return Err(
                "trailing binding refuses: logical token already bound; refusing".to_owned(),
            );
        }
        if self
            .bindings
            .iter()
            .any(|binding| binding.native_id == native_id)
        {
            return Err(
                "trailing binding refuses: native Space already bound; refusing".to_owned(),
            );
        }
        self.bindings.push(TrailingBinding {
            logical_id: logical_id.to_owned(),
            output: output.clone(),
            created_sid,
            native_id,
        });
        Ok(())
    }

    /// World-report binding rows for every outstanding binding. Carried on
    /// every full report until acknowledgement or pruning: the core clears
    /// its pending token and drains pending sends on the first report that
    /// names the logical token.
    ///
    /// Flags: `managed: true` (proven owned creation, matching the existing
    /// binding fixtures), never pinned or displaced (trailing Spaces stay
    /// eligible for the lifecycle cleanup path, never retained against it).
    #[must_use]
    pub fn report_bindings(&self) -> Vec<WorkspaceBinding> {
        self.bindings
            .iter()
            .map(|binding| WorkspaceBinding {
                logical_id: binding.logical_id.clone(),
                native_id: binding.native_id.clone(),
                output: binding.output.clone(),
                pinned: false,
                displaced: false,
                managed: true,
            })
            .collect()
    }

    /// Forget bindings whose native Space left live topology (externally
    /// removed - never re-removed here). Returns the dropped logical ids
    /// in adoption order. `live` maps each output to its current native
    /// workspace ids; a binding whose output or native is absent drops.
    pub fn prune_vanished(&mut self, live: &BTreeMap<OutputId, Vec<String>>) -> Vec<String> {
        let mut dropped = Vec::new();
        self.bindings.retain(|binding| {
            let alive = live
                .get(&binding.output)
                .is_some_and(|ids| ids.iter().any(|id| id == &binding.native_id));
            if !alive {
                dropped.push(binding.logical_id.clone());
            }
            alive
        });
        dropped
    }

    /// Drop bindings the core acknowledged: the reply publishes the bound
    /// row (same logical token with the bound native). Returns the
    /// acknowledged logical ids in adoption order. An unbound row for the
    /// token (pending still outstanding) never acknowledges.
    pub fn note_ack(&mut self, reply: &WorldReply) -> Vec<String> {
        let mut acked = Vec::new();
        self.bindings.retain(|binding| {
            let bound = reply.workspaces.iter().any(|row| {
                row.logical_id.as_deref() == Some(binding.logical_id.as_str())
                    && row.native_binding.as_deref() == Some(binding.native_id.as_str())
            });
            if bound {
                acked.push(binding.logical_id.clone());
            }
            !bound
        });
        acked
    }
}

/// Identify the actual pending logical token from the engine's unbound
/// workspace desire for `output`: the first row with no native binding
/// carrying a logical id. `None` when the engine desires nothing unbound
/// there (reuse path, no creation). Callers tie the returned token to the
/// exact-append proof through
/// [`TrailingBindingStore::bind_exact_append`] - never a guessed token,
/// never an existing Space.
#[must_use]
pub fn unbound_trailing_token(reply: &WorldReply, output: &OutputId) -> Option<String> {
    reply
        .workspaces
        .iter()
        .find(|row| {
            row.output == *output && row.native_binding.is_none() && row.logical_id.is_some()
        })
        .and_then(|row| row.logical_id.clone())
}

/// Resolve one proven SLS display identifier to exactly one output under
/// the engine's published native context. `None` when the context is
/// absent, the identifier names no adopted domain, or it names domains on
/// several outputs (stale publication or a display swap mid-proof) -
/// never a positional guess, never a first-display fallback. Pure.
#[must_use]
pub(crate) fn resolve_trailing_output(
    ctx: Option<&crate::runtime_types::NativeContext>,
    display_id: &str,
) -> Option<OutputId> {
    let ctx = ctx?;
    let mut outputs = std::collections::BTreeSet::new();
    for domain in &ctx.domains {
        if domain.uuid.as_deref() == Some(display_id) {
            outputs.insert(domain.output.clone());
        }
    }
    (outputs.len() == 1)
        .then(|| outputs.into_iter().next())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> OutputId {
        OutputId(crate::topology::output_key(1))
    }

    fn domain(uuid: Option<&str>, output: &str, space: u64) -> crate::runtime_types::NativeDomain {
        crate::runtime_types::NativeDomain {
            cg_id: 1,
            uuid: uuid.map(str::to_owned),
            output: OutputId(output.to_owned()),
            workspace: tiler_core::directional::WorkspaceId(crate::topology::workspace_key(space)),
            space,
            current: true,
            readable: true,
            bounds: tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            visible: tiler_core::geometry::Rect {
                x: 0,
                y: 25,
                w: 1440,
                h: 875,
            },
        }
    }

    fn make_ctx(
        domains: Vec<crate::runtime_types::NativeDomain>,
    ) -> crate::runtime_types::NativeContext {
        crate::runtime_types::NativeContext {
            revision: 9,
            domains,
            skipped: Vec::new(),
        }
    }

    #[test]
    fn bind_records_exact_append_and_reports_managed_unpinned() {
        let mut store = TrailingBindingStore::default();
        store
            .bind_exact_append("wl-1", &output(), 278, &[1, 277], &[1, 277, 278])
            .expect("exact append binds");
        assert_eq!(store.len(), 1);
        let rows = store.report_bindings();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].logical_id, "wl-1");
        assert_eq!(rows[0].native_id, "space-278");
        assert_eq!(rows[0].output, output());
        assert!(!rows[0].pinned, "trailing stays cleanup-eligible");
        assert!(!rows[0].displaced);
        assert!(rows[0].managed, "proven owned creation");
    }

    #[test]
    fn bind_refuses_drift_guesses_and_duplicates() {
        let mut store = TrailingBindingStore::default();
        // Existing Space is never an append.
        assert!(
            store
                .bind_exact_append("wl-1", &output(), 277, &[1, 277], &[1, 277])
                .is_err()
        );
        // Drifted row (not a strict prefix append).
        assert!(
            store
                .bind_exact_append("wl-1", &output(), 278, &[1, 277], &[1, 279, 278])
                .is_err()
        );
        // Short row (no append ran).
        assert!(
            store
                .bind_exact_append("wl-1", &output(), 278, &[1, 277], &[1, 277])
                .is_err()
        );
        // Zero SID.
        assert!(
            store
                .bind_exact_append("wl-1", &output(), 0, &[1, 277], &[1, 277, 0])
                .is_err()
        );
        // Non-token logical (native id, fallback echo shape, garbage).
        assert!(
            store
                .bind_exact_append("space-278", &output(), 278, &[1, 277], &[1, 277, 278])
                .is_err()
        );
        assert!(
            store
                .bind_exact_append("desk-1", &output(), 278, &[1, 277], &[1, 277, 278])
                .is_err()
        );
        assert!(
            store
                .bind_exact_append("", &output(), 278, &[1, 277], &[1, 277, 278])
                .is_err()
        );
        assert!(store.is_empty(), "refusals mutate nothing");
        // Duplicate token and duplicate native both refuse.
        store
            .bind_exact_append("wl-1", &output(), 278, &[1, 277], &[1, 277, 278])
            .expect("first binds");
        assert!(
            store
                .bind_exact_append("wl-1", &output(), 279, &[1, 277, 278], &[1, 277, 278, 279])
                .is_err(),
            "duplicate token refuses"
        );
        assert!(
            store
                .bind_exact_append("wl-2", &output(), 278, &[1, 277], &[1, 277, 278])
                .is_err(),
            "duplicate native refuses"
        );
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn prune_drops_vanished_natives_and_acked_tokens_only() {
        use tiler_core::world_report::TilingMode;
        use tiler_core::world_report::WorkspaceDesire;

        let mut store = TrailingBindingStore::default();
        store
            .bind_exact_append("wl-1", &output(), 278, &[1, 277], &[1, 277, 278])
            .expect("binds");
        // Still live: nothing prunes.
        let mut live = BTreeMap::new();
        live.insert(
            output(),
            vec![
                "space-1".to_owned(),
                "space-277".to_owned(),
                "space-278".to_owned(),
            ],
        );
        assert!(store.prune_vanished(&live).is_empty());
        assert_eq!(store.len(), 1);
        // Native gone (externally removed): prunes without acknowledgement.
        live.insert(output(), vec!["space-1".to_owned(), "space-277".to_owned()]);
        assert_eq!(store.prune_vanished(&live), vec!["wl-1".to_owned()]);
        assert!(store.is_empty());

        // Acknowledgement needs the bound row, never the unbound desire.
        store
            .bind_exact_append("wl-1", &output(), 278, &[1, 277], &[1, 277, 278])
            .expect("binds again");
        let mut reply = empty_reply();
        reply.workspaces.push(WorkspaceDesire {
            id: "wl-1".to_owned(),
            output: output(),
            native_binding: None,
            logical_id: Some("wl-1".to_owned()),
            scope: "macos-display-1".to_owned(),
            order: 2,
            tiling_mode: TilingMode::Tiled,
            existence: true,
            hold: None,
        });
        assert!(
            store.note_ack(&reply).is_empty(),
            "unbound desire never acks"
        );
        assert_eq!(store.len(), 1);
        reply.workspaces[0].native_binding = Some("space-278".to_owned());
        reply.workspaces[0].id = "space-278".to_owned();
        assert_eq!(store.note_ack(&reply), vec!["wl-1".to_owned()]);
        assert!(store.is_empty());
    }

    fn empty_reply() -> WorldReply {
        WorldReply {
            contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            request_id: "req-0".to_owned(),
            based_on_observation_seq: 0,
            state_revision: 0,
            ready: false,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            command_result: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            cursor_warp: None,
            presentation_bytes: 0,
        }
    }

    #[test]
    fn unbound_token_finds_engine_desire_only() {
        use tiler_core::world_report::TilingMode;
        use tiler_core::world_report::WorkspaceDesire;

        let mut reply = empty_reply();
        assert_eq!(unbound_trailing_token(&reply, &output()), None);
        reply.workspaces.push(WorkspaceDesire {
            id: "space-277".to_owned(),
            output: output(),
            native_binding: Some("space-277".to_owned()),
            logical_id: Some("desk-9".to_owned()),
            scope: "macos-display-1".to_owned(),
            order: 1,
            tiling_mode: TilingMode::Tiled,
            existence: true,
            hold: None,
        });
        assert_eq!(
            unbound_trailing_token(&reply, &output()),
            None,
            "bound rows never match"
        );
        reply.workspaces.push(WorkspaceDesire {
            id: "wl-3".to_owned(),
            output: output(),
            native_binding: None,
            logical_id: Some("wl-3".to_owned()),
            scope: "macos-display-1".to_owned(),
            order: 2,
            tiling_mode: TilingMode::Tiled,
            existence: true,
            hold: None,
        });
        assert_eq!(
            unbound_trailing_token(&reply, &output()),
            Some("wl-3".to_owned())
        );
        assert_eq!(
            unbound_trailing_token(&reply, &OutputId("macos-display-2".to_owned())),
            None,
            "other outputs never match"
        );
    }

    #[test]
    fn resolve_output_names_exactly_one_uuid_homed_output() {
        // No context: unresolvable (never guessed).
        assert_eq!(resolve_trailing_output(None, "uuid-1"), None);
        // Identifier names one adopted domain: its output.
        let ctx = make_ctx(vec![domain(Some("uuid-1"), "macos-display-1", 277)]);
        assert_eq!(
            resolve_trailing_output(Some(&ctx), "uuid-1"),
            Some(output())
        );
        // Unknown identifier: refuses (stale publication, never positional).
        assert_eq!(resolve_trailing_output(Some(&ctx), "uuid-9"), None);
        // Domain without a UUID never resolves (unprovable home).
        let ctx = make_ctx(vec![domain(None, "macos-display-1", 277)]);
        assert_eq!(resolve_trailing_output(Some(&ctx), "uuid-1"), None);
        // Same identifier on two outputs (display swap mid-proof):
        // refuses instead of picking one.
        let ctx = make_ctx(vec![
            domain(Some("uuid-1"), "macos-display-1", 277),
            domain(Some("uuid-1"), "macos-display-2", 278),
        ]);
        assert_eq!(resolve_trailing_output(Some(&ctx), "uuid-1"), None);
    }
}
