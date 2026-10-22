#![forbid(unsafe_code)]
use super::*;

/// Enumerate top-level HWNDs. `None` on API failure: the caller retains Engine
/// state and skips the tick, never treating a failed enumeration as a
/// complete (possibly empty) observation.
pub(super) fn enumerate_hwnds() -> Option<Vec<isize>> {
    crate::ffi::enumerate_windows().map(|hwnds| {
        hwnds
            .into_iter()
            .map(|hwnd| hwnd as usize as isize)
            .collect()
    })
}

pub(super) fn cursor_pos() -> Option<(i32, i32)> {
    crate::ffi::cursor_pos()
}

/// Normalized per-tick cursor sample in adapter units (same space as
/// screen FULL rectangles and fact frames): `None` when unreadable or
/// outside the carried geometry bounds (fail-closed to the focused
/// fallback, never an invented position). One sample per capture or
/// command snapshot only: no hooks, no polling, and no sampling at all on
/// the focused-fullscreen-game lane (callers pass `None` there).
pub(super) fn sample_cursor() -> Option<(i32, i32)> {
    normalize_cursor(cursor_pos())
}

/// Pure normalization for one cursor read: values outside the carried
/// geometry bounds drop to `None` so an out-of-range read can never
/// refuse a report (`cursor-invalid`) or steer policy.
pub(super) fn normalize_cursor(raw: Option<(i32, i32)>) -> Option<(i32, i32)> {
    raw.filter(|(x, y)| {
        (-tiler_core::bounds::GEOMETRY_BOUND..=tiler_core::bounds::GEOMETRY_BOUND).contains(x)
            && (-tiler_core::bounds::GEOMETRY_BOUND..=tiler_core::bounds::GEOMETRY_BOUND)
                .contains(y)
    })
}

impl TileLoop {
    pub(super) fn correlation(&self) -> CorrelationId {
        CorrelationId::parse(&format!("tick-{}", self.tick))
            .expect("tick correlation is a valid token")
    }

    /// D23 hidden-iconic refresh at the observation boundary:
    /// `HiddenRecord.iconic` freezes at hide time, but a hidden window can
    /// minimize after hide while the raw gate skips it as invisible, which
    /// would otherwise retain its leaf forever. For owned hidden
    /// exact-lifetime members (stored tag reads live, the same bar as the
    /// hide effects), refresh the per-tick omission from the native iconic
    /// state (read-only; fixture seam under test). Only enumerated HWNDs
    /// refresh: destroyed or absent evidence stays with the audit, and
    /// unreadable evidence never fabricates a row either way. Visible
    /// iconic members omit via the real arm below (re-insertion here would
    /// be idempotent); the hide-time record itself is never rewritten, so
    /// the reveal preimage stays intact.
    fn refresh_hidden_iconic(&mut self) {
        let keys: Vec<crate::native_guards::WindowKey> =
            self.hidden_claims.keys().cloned().collect();
        for key in keys {
            if !self.last_hwnds.contains(&key.hwnd) {
                continue;
            }
            if !self.member_tokens.contains_key(&key) {
                continue;
            }
            let owned = self.member_tags.get(&key).is_some_and(|stored| {
                !stored.is_empty()
                    && crate::native_guards::visible_lifetime_ok(
                        stored,
                        live_member_tag(key.hwnd).as_deref(),
                    )
            });
            if !owned {
                continue;
            }
            if live_is_iconic(key.hwnd) {
                self.minimized_omitted.insert(key);
            }
        }
    }

    /// Full enumeration with per-window eligibility. Proof mode prefilters by
    /// frozen-allowlist HWND before any per-window query, so non-owned windows
    /// never mint tokens, churn the map, or enter logs; `EnumWindows` still
    /// yields the complete raw count. Proof mode additionally requires
    /// frozen-allowlist membership plus owned-helper verification (sibling
    /// exe/class/lifetime-tag); malformed allowlist state is unreachable here
    /// because `cmd_tile_proof` refuses it before the loop starts. Invisible
    /// allowlist members stay frozen (not eligible) until an exact-bound
    /// `show` admission. Normal mode with a nonempty scope additionally
    /// reports out-of-scope executables as `scope-excluded` with no
    /// membership, geometry, or focus. Returns `None` when enumeration itself
    /// failed: the tick is skipped with retained Engine state.
    pub(super) fn observe(
        &mut self,
        me: &ProcessIdentity,
        fulls: &[Rect],
        skipped: &mut Vec<(String, String)>,
        retained: &mut Vec<RetainedRow>,
    ) -> Option<Vec<ObservedWindow>> {
        #[cfg(test)]
        if let Some((observed, retained_injected, enum_failed)) =
            INJECTED_OBSERVE.with(|slot| slot.borrow().clone())
        {
            if enum_failed {
                return None;
            }
            // D23 injected fixture contract: the injected observed plus
            // retained frames are the complete inventory (production
            // enumeration yields exactly these windows). The omission set
            // refreshes every pass, never leaking across ticks.
            self.minimized_omitted.clear();
            self.last_enumerated = observed.len() + retained_injected.len();
            self.last_hwnds = observed
                .iter()
                .map(|window| window.hwnd)
                .chain(retained_injected.iter().map(|row| row.key.hwnd))
                .collect();
            // Same hidden-iconic refresh as the real path below, driven by
            // the fixture seam instead of native queries.
            self.refresh_hidden_iconic();
            retained.extend(retained_injected);
            // The real path derives this sequence from its single
            // `EnumWindows` enumeration; fixtures either script it
            // explicitly (retained overlays interleaved in native position)
            // or fall back to observed-then-retained exactly as before.
            let visible: Vec<crate::native_guards::WindowKey> = INJECTED_ZORDER
                .with(|slot| slot.borrow().clone())
                .unwrap_or_else(|| {
                    let mut fallback = Vec::with_capacity(observed.len() + retained.len());
                    for window in observed.iter() {
                        fallback.push(crate::native_guards::WindowKey {
                            hwnd: window.hwnd,
                            pid: window.identity.pid,
                            creation: window.identity.process_creation.clone(),
                        });
                    }
                    for row in retained.iter() {
                        if row.rect.is_some() {
                            fallback.push(row.key.clone());
                        }
                    }
                    fallback
                });
            self.record_pager_live(&observed, retained, &visible);
            let _ = (me, fulls);
            return Some(observed);
        }
        use ObserveFailure::{Known, Unknown};
        let proof_mode = self.allowlist.is_some();
        let hwnds = enumerate_hwnds()?;
        self.last_enumerated = hwnds.len();
        self.last_hwnds = hwnds.iter().map(|raw| *raw as usize as u64).collect();
        // D23: fresh omission set per pass; stale minimized keys must never
        // leak into the next tick's row assembly. Hidden-iconic refresh
        // runs on the enumerated inventory before any per-window query.
        self.minimized_omitted.clear();
        self.refresh_hidden_iconic();
        let mut out = Vec::new();
        let mut unreadable = 0usize;
        // Every resolved identity pair mints or reuses a token; retain all of
        // them below (eligible, skipped, and frame-failed) so known windows
        // keep stable tokens instead of churning one new token per tick.
        let mut enumerated: Vec<(u64, String)> = Vec::new();
        // Verified live-frame member sequence in this same enumeration order
        // (top-to-bottom): observed eligible members plus retained rows with
        // fresh frames. Frameless rows (minimized, hosted-gate refusals) never
        // enter: they keep their retained stacking slot instead. This is the
        // only native order read; the pager never queries z-order itself.
        let mut zorder: Vec<crate::native_guards::WindowKey> = Vec::new();
        for raw in hwnds {
            // The process-owned border overlay is never a tile target, a
            // token source, or an unreadable count: skip it before any query.
            if owned_overlay_hwnd(self, RenderCarrier::Border) == Some(raw as usize as u64) {
                continue;
            }
            // Same for the process-owned group-underlay fill: never a tile
            // target, so its own move/paint events cannot retarget tiling or
            // the underlay itself.
            if owned_overlay_hwnd(self, RenderCarrier::Underlay) == Some(raw as usize as u64) {
                continue;
            }
            let hwnd = raw as HWND;
            if proof_mode {
                // Frozen-HWND prefilter before any per-window query.
                let entries = self.allowlist.as_ref().expect("proof mode has allowlist");
                if !entries
                    .iter()
                    .any(|entry| entry.hwnd == raw as usize as u64)
                {
                    continue;
                }
            }
            let window = match observe_window(hwnd, me, fulls, &mut self.tokens) {
                Ok(window) => window,
                Err(Known(known, reason)) => {
                    // Identity known: retain it and skip with the accurate
                    // reason (minimized needs no frame; unreadable keeps its
                    // stable token instead of counting as changed identity).
                    // D23 minimize release: a minimized window omits its
                    // Engine row (no retained-occupancy allocation) so
                    // convergence releases the leaf, reflows survivors,
                    // resolves MRU-or-none focus, and stores the
                    // `MinimizedSlot` for best-effort rejoin. The token stays
                    // stable via `enumerated` below, so a native restore
                    // re-carries the same Engine identity purely tiled.
                    // Out-of-scope executables never retain membership.
                    enumerated.push((known.identity.hwnd, known.identity.process_creation.clone()));
                    if !scope_allows(&self.scope, &known.identity.exe_path) {
                        skipped.push((known.token.clone(), "scope-excluded".to_owned()));
                        continue;
                    }
                    skipped.push((known.token.clone(), reason.as_str().to_owned()));
                    if reason == SkipReason::Minimized {
                        self.minimized_omitted
                            .insert(crate::native_guards::WindowKey {
                                hwnd: known.identity.hwnd,
                                pid: known.identity.pid,
                                creation: known.identity.process_creation.clone(),
                            });
                        continue;
                    }
                    retained.push(RetainedRow {
                        key: crate::native_guards::WindowKey {
                            hwnd: known.identity.hwnd,
                            pid: known.identity.pid,
                            creation: known.identity.process_creation.clone(),
                        },
                        token: known.token.clone(),
                        rect: None,
                        maximized: false,
                        fullscreen: false,
                        facts: None,
                    });
                    continue;
                }
                Err(Unknown) => {
                    unreadable += 1;
                    continue;
                }
            };
            enumerated.push((window.hwnd, window.identity.process_creation.clone()));
            if proof_mode {
                let entries = self.allowlist.as_ref().expect("proof mode has allowlist");
                let Some(entry) = entries
                    .iter()
                    .find(|entry| allow_match(entry, &window.identity))
                else {
                    continue;
                };
                // Owned-helper gate: non-owned windows never enter management
                // even when otherwise eligible. Invisible frozen members are
                // still verified (passive admission) but stay skipped until
                // shown.
                if verify_proof_owned(window.hwnd, entry, me).is_err() {
                    skipped.push((window.token.clone(), "identity-changed".to_owned()));
                    continue;
                }
            }
            match classify(&window.facts) {
                Ok(()) => {
                    if !scope_allows(&self.scope, &window.identity.exe_path) {
                        skipped.push((window.token.clone(), "scope-excluded".to_owned()));
                        continue;
                    }
                    // Listed hosts pass only with a live matching hosted
                    // child, re-verified every tick: a newly appearing hosted
                    // app can never ride another app's host executable into
                    // observation, membership, writes, or focus. Excluded
                    // members keep their retained snapshot (no membership
                    // loss) and re-enter when the child set is clean again.
                    if !hosted_gate_allows(
                        &window.identity.exe_path,
                        window.hwnd,
                        window.identity.pid,
                        &self.scope_hosts,
                    ) {
                        skipped.push((window.token.clone(), "scope-excluded".to_owned()));
                        retained.push(RetainedRow {
                            key: crate::native_guards::WindowKey {
                                hwnd: window.identity.hwnd,
                                pid: window.identity.pid,
                                creation: window.identity.process_creation.clone(),
                            },
                            token: window.token.clone(),
                            rect: None,
                            maximized: false,
                            fullscreen: false,
                            facts: None,
                        });
                        continue;
                    }
                    zorder.push(crate::native_guards::WindowKey {
                        hwnd: window.hwnd,
                        pid: window.identity.pid,
                        creation: window.identity.process_creation.clone(),
                    });
                    out.push(window);
                }
                Err(reason) => {
                    skipped.push((window.token.clone(), reason.as_str().to_owned()));
                    // Ineligible but fully observed: the fresh frame keeps
                    // retained occupancy (maximized, fullscreen, cloaked)
                    // inside Engine membership with no geometry writes. The
                    // fresh overlay flags ride along so move/pointer routes
                    // refuse before mutation and admission clears once.
                    let retained_key = crate::native_guards::WindowKey {
                        hwnd: window.hwnd,
                        pid: window.identity.pid,
                        creation: window.identity.process_creation.clone(),
                    };
                    zorder.push(retained_key.clone());
                    retained.push(RetainedRow {
                        key: retained_key,
                        token: window.token.clone(),
                        rect: Some(window.visible),
                        maximized: window.facts.maximized,
                        fullscreen: window.facts.captionless_fullscreen,
                        facts: Some(window.facts),
                    });
                }
            }
        }
        // Bounded retain over every enumerated pair, never HWND alone and
        // never eligible-only: skipped identities keep their tokens. Known
        // hidden claims are retained too: they never pass the visible gate
        // but their tokens stay stable across ticks.
        let mut live_refs: Vec<ObservedTargetRef<'_>> = enumerated
            .iter()
            .map(|(hwnd, creation)| ObservedTargetRef {
                hwnd: *hwnd,
                creation,
            })
            .collect();
        for record in self.hidden_claims.values() {
            if !live_refs.iter().any(|t| {
                t.hwnd == record.claim.hwnd && t.creation == record.claim.process.process_creation
            }) {
                live_refs.push(ObservedTargetRef {
                    hwnd: record.claim.hwnd,
                    creation: &record.claim.process.process_creation,
                });
            }
        }
        self.tokens.retain(&live_refs);
        if unreadable > 0 {
            skipped.push((
                format!("unreadable-{unreadable}"),
                SkipReason::Unreadable.as_str().to_owned(),
            ));
        }
        track_fullscreen_holds(self, &out, retained);
        // Live native overlay snapshot per Engine token for the fixed-size
        // admission stamps: observed facts plus retained rows, refreshed on
        // every observation. Stale entries prune below by live membership;
        // recycled HWNDs mint fresh tokens and never inherit.
        for window in &out {
            self.overlay_now.insert(
                window.token.clone(),
                (window.facts.maximized, window.facts.captionless_fullscreen),
            );
        }
        for row in retained.iter() {
            self.overlay_now
                .insert(row.token.clone(), (row.maximized, row.fullscreen));
        }
        let live_tokens: std::collections::HashSet<&String> = out
            .iter()
            .map(|w| &w.token)
            .chain(retained.iter().map(|r| &r.token))
            .collect();
        self.overlay_now
            .retain(|token, _| live_tokens.contains(token));
        self.record_pager_live(&out, retained, &zorder);
        Some(out)
    }

    /// Refresh the taskbar-pager live frames: fresh identity-verified frames
    /// for live managed members (observed `visible` frames plus retained
    /// fresh frames for kept maximized/fullscreen overlays and born holds).
    /// Rebuilt per pass and gated on live membership, so recycled identities
    /// and closed windows never linger; hidden members are absent here by
    /// construction and resolve through their saved tile-slot/float
    /// snapshots in the portable resolver. The retained stacking order
    /// advances from the same single enumeration order: visible members take
    /// the fresh native sequence while still-assigned hidden members keep
    /// their previous relative order and departed identities prune (see
    /// `crate::pager::retain_pager_order`). Observe runs before workspace
    /// assignment, so newly verified clients are not members yet: the
    /// retention roster unions current membership with the fresh visible
    /// keys, making the native order available to the first paint after
    /// admission with no second observation. The frame builder still filters
    /// to actual membership, so unadmitted candidates never render, and the
    /// next observation prunes candidates that never joined and are now
    /// absent. Read-only: never touches windows
    /// and never mutates the tiling `member_rects`/`float_rects` tables.
    fn record_pager_live(
        &mut self,
        observed: &[ObservedWindow],
        retained: &[RetainedRow],
        visible_top_to_bottom: &[crate::native_guards::WindowKey],
    ) {
        let mut live: HashMap<crate::native_guards::WindowKey, Rect> = HashMap::new();
        for window in observed {
            let key = crate::native_guards::WindowKey {
                hwnd: window.hwnd,
                pid: window.identity.pid,
                creation: window.identity.process_creation.clone(),
            };
            if self.member_home(&key).is_some() {
                live.insert(key, window.visible);
            }
        }
        for row in retained {
            if let Some(rect) = row.rect
                && self.member_home(&row.key).is_some()
            {
                live.entry(row.key.clone()).or_insert(rect);
            }
        }
        self.pager_live = live;
        let mut members: BTreeSet<crate::native_guards::WindowKey> = BTreeSet::new();
        for output in self.output_devices() {
            for id in self.workspace_ids(&output) {
                members.extend(self.workspace_members(&output, &id));
            }
        }
        // Union the fresh visible keys: observe runs before assignment, so a
        // newly verified client admitted right after still paints in native
        // order on its first frame. Unadmitted candidates never render (the
        // builder filters to membership) and prune on the next observation
        // once absent.
        members.extend(visible_top_to_bottom.iter().cloned());
        self.pager_z =
            crate::pager::retain_pager_order(&self.pager_z, &members, visible_top_to_bottom);
    }
}
