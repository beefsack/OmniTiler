#![forbid(unsafe_code)]
use super::*;

/// Deterministic scripted backend for renderer-core tests. No threads, no
/// native calls: freshness facts, anchor resolutions, failures and carrier
/// identities are all scripted, while checksums run the real portable
/// painters so raster equivalence is covered, not stubbed.
#[derive(Debug, Default)]
pub struct FakePresent {
    /// Scripted per-HWND freshness (`None` entry falls back to `live_facts`).
    pub fresh: std::collections::HashMap<u64, FreshFacts>,
    /// Default facts for unscripted HWNDs (live by default).
    pub live_facts: Option<FreshFacts>,
    /// Enforced foreground lifetime (`None` = use scripted facts): when
    /// set, `foreground_ok` compares the query foreground against this
    /// instead of trusting the scripted entry, so drift tests script a
    /// foreground switch rather than a fact flip.
    pub foreground: Option<WindowLifetime>,
    /// HWNDs reading captionless-fullscreen (OR-ed into scripted facts for
    /// non-preview carriers).
    pub fullscreen_hwnds: std::collections::HashSet<u64>,
    /// Scripted per-HWND band reads (`None` entry reads non-topmost).
    pub bands: std::collections::HashMap<u64, bool>,
    /// Candidate frames offered to `resolve_anchor`, in order (evidence).
    pub resolved_with: Vec<UnderlayDesired>,
    /// Shown frames, in order (evidence).
    pub shown: Vec<DesiredFrame>,
    /// Resolved anchors used for underlay shows, in order (evidence).
    pub shown_anchors: Vec<u64>,
    /// Bands used for underlay shows, in order (evidence: the resolved
    /// anchor band, not the frame fallback).
    pub shown_bands: Vec<bool>,
    /// Return the own border as the underlay anchor when set (evidence for
    /// the renderer-owned anchor path without any foreign member tag).
    pub own_border_anchor: bool,
    /// Scripted facts for the own-border anchor check.
    pub own_facts: Option<FreshFacts>,
    /// Scripted carrier visibility (own-anchor eligibility evidence).
    pub visible: [bool; 3],
    /// Scripted carrier tokens (same-group evidence for the own anchor).
    pub carrier_tokens: [Option<String>; 3],
    /// Hide calls, in order (evidence).
    pub hidden: Vec<RenderCarrier>,
    /// Destroy calls so far.
    pub destroys: u32,
    /// Scripted `destroy_all` result.
    pub destroy_ok: bool,
    /// Scripted carrier identities (`None` = not yet created).
    pub hwnds: [Option<u64>; 3],
    /// Scripted `ensure_created` failure detail (`None` = success).
    pub fail_ensure: Option<String>,
    /// `ensure_created` calls so far (evidence).
    pub ensure_calls: u32,
    /// Scripted per-carrier show failure detail (consumed once each).
    pub fail_shows: std::collections::HashMap<RenderCarrier, String>,
    /// Carriers whose next hide reports unverified (consumed once each).
    pub fail_hides: std::collections::HashSet<RenderCarrier>,
    /// Scripted per-show outcome override (consumed in order).
    pub outcome_script: std::collections::VecDeque<RenderOutcome>,
    /// Scripted per-query diagnostics attached to border presents in order
    /// (consumed in order; evidence that every completed query is
    /// preserved, not just the latest).
    pub accent_queries: std::collections::VecDeque<crate::active_border::AccentQueryReport>,
}

impl FakePresent {
    /// New fake with live defaults, successful destroy, and no identities.
    #[must_use]
    pub fn new() -> Self {
        Self {
            live_facts: Some(FreshFacts {
                alive: true,
                identity_ok: true,
                visible: true,
                zoom_ok: true,
                fullscreen: false,
                foreground_ok: true,
                band_ok: true,
            }),
            destroy_ok: true,
            ..Self::default()
        }
    }

    fn facts_for(&self, hwnd: u64) -> FreshFacts {
        self.fresh.get(&hwnd).copied().unwrap_or_else(|| {
            self.live_facts.unwrap_or(FreshFacts {
                alive: false,
                identity_ok: false,
                visible: false,
                zoom_ok: false,
                fullscreen: false,
                foreground_ok: false,
                band_ok: false,
            })
        })
    }

    fn next_outcome(
        &mut self,
        checksum: u64,
        resolved: (u8, u8, u8),
        accent_sourced: bool,
    ) -> PresentOutcome {
        match self.outcome_script.pop_front() {
            Some(outcome) => PresentOutcome {
                outcome,
                checksum,
                error: None,
                resolved: Some(resolved),
                accent_sourced,
                accent_query: None,
            },
            None => PresentOutcome::shown(RenderOutcome::Shown, checksum, resolved, accent_sourced),
        }
    }

    /// Resolve the fake border colour exactly like the native carrier: the
    /// frame's shared owner snapshot wins when the frame requests the theme
    /// and the snapshot is available, else the frame fallback. A missing
    /// snapshot (`None`) always falls back, matching native semantics;
    /// fixtures stage the snapshot on the frame itself. Source reporting
    /// mirrors the native carrier exactly.
    fn resolve_border_color(&self, frame: &BorderDesired) -> ((u8, u8, u8), bool) {
        use crate::active_border::ActiveBorderStyle;
        let style = ActiveBorderStyle {
            width: 0.0,
            gap: 0.0,
            radius: 0.0,
            color: frame.color,
            use_theme: frame.use_theme,
        };
        (
            crate::active_border::effective_color(&style, frame.accent),
            crate::active_border::effective_uses_accent(&style, frame.accent),
        )
    }

    /// FNV-1a checksum over the ring predicate walk: byte-identical in shape
    /// to the native painter (which evaluates the same
    /// [`crate::active_border::ring_covers`] predicate per pixel), so the
    /// core ack path is covered with the real coverage function.
    fn border_checksum(
        outer_w: i32,
        outer_h: i32,
        width_px: i32,
        radius_px: i32,
        color: (u8, u8, u8),
    ) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET;
        let mut mix = |byte: u8| {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        };
        let (r, g, b) = color;
        for y in 0..outer_h {
            for x in 0..outer_w {
                if crate::active_border::ring_covers(x, y, outer_w, outer_h, width_px, radius_px) {
                    for byte in [b, g, r, 255] {
                        mix(byte);
                    }
                } else {
                    for _ in 0..4 {
                        mix(0);
                    }
                }
            }
        }
        hash
    }
}

impl Present for FakePresent {
    fn check_fresh(&self, carrier: RenderCarrier, hwnd: u64, query: &FreshQuery<'_>) -> FreshFacts {
        // Fixture identity, liveness, visibility, zoom, and band stay
        // scripted through `fresh`/`live_facts`; the enforced foreground
        // and fullscreen sets overlay drift on top, so tests script a
        // switch (new identity, new fullscreen HWND) rather than a fact
        // flip, exactly like the native reads observe it. An unreadable
        // post-time foreground (`None`) refuses narrowly per the binding
        // contract, mirroring the native fence: no unprovable stale show.
        let mut facts = self.facts_for(hwnd);
        if query.foreground.is_none() {
            facts.foreground_ok = false;
        } else if let Some(expected) = self.foreground.as_ref() {
            facts.foreground_ok = query.foreground == Some(expected);
        }
        if carrier != RenderCarrier::Preview && self.fullscreen_hwnds.contains(&hwnd) {
            facts.fullscreen = true;
        }
        facts
    }

    fn check_own_anchor(
        &self,
        hwnd: u64,
        _topmost: bool,
        foreground: Option<&WindowLifetime>,
    ) -> FreshFacts {
        // Renderer-owned validation without any foreign member tag: scripted
        // facts plus the enforced-foreground fence, mirroring the native
        // own-anchor read (visible, same-band, foreground unchanged).
        // Unreadable foreground refuses narrowly like the native fence.
        let mut facts = self.own_facts.unwrap_or_else(|| self.facts_for(hwnd));
        if foreground.is_none() {
            facts.foreground_ok = false;
        } else if let Some(expected) = self.foreground.as_ref() {
            facts.foreground_ok = foreground.is_some_and(|query| query == expected);
        }
        facts
    }

    fn resolve_anchor(&self, frame: &UnderlayDesired) -> Option<ResolvedAnchor> {
        // Default fixture resolution: the first candidate with its own
        // lifetime (the owner orders the fresh lowest anchor first).
        // Unreadable-order cases use empty candidate lists, which exercise
        // the owner-fallback path in the core. The own-border path returns
        // the renderer-owned anchor with no foreign lifetime, so tests
        // prove the paired-identity fix instead of hiding it.
        if self.own_border_anchor {
            return self.hwnds[RenderCarrier::Border.index()]
                .map(|hwnd| ResolvedAnchor::OwnBorder { hwnd });
        }
        frame
            .candidates
            .first()
            .map(|candidate| ResolvedAnchor::Foreign {
                hwnd: candidate.hwnd,
                lifetime: candidate.lifetime.clone(),
            })
    }

    fn read_band(&self, hwnd: u64) -> Option<bool> {
        if hwnd == 0 {
            return None;
        }
        Some(*self.bands.get(&hwnd).unwrap_or(&false))
    }

    fn carrier_visible(&self, carrier: RenderCarrier) -> bool {
        self.visible[carrier.index()]
    }

    fn carrier_token(&self, carrier: RenderCarrier) -> Option<String> {
        self.carrier_tokens[carrier.index()].clone()
    }

    fn ensure_created(&mut self) -> Result<(), String> {
        self.ensure_calls += 1;
        match self.fail_ensure.clone() {
            Some(detail) => Err(detail),
            None => Ok(()),
        }
    }

    fn show_border(&mut self, frame: &BorderDesired) -> PresentOutcome {
        self.shown.push(DesiredFrame::Border(frame.clone()));
        if let Some(detail) = self.fail_shows.remove(&RenderCarrier::Border) {
            return PresentOutcome::failed(detail);
        }
        let (resolved, sourced) = self.resolve_border_color(frame);
        let checksum = Self::border_checksum(
            frame.outer.w,
            frame.outer.h,
            frame.style_px.0,
            frame.style_px.2,
            resolved,
        );
        // The shared snapshot (or the scripted stand-in) decides the
        // source exactly like the native carrier: a themed frame with an
        // available snapshot reports the accent source.
        let mut outcome = self.next_outcome(checksum, resolved, sourced);
        outcome.accent_query = self.accent_queries.pop_front();
        outcome
    }

    fn show_underlay(
        &mut self,
        frame: &UnderlayDesired,
        anchor: u64,
        topmost: bool,
    ) -> PresentOutcome {
        self.shown.push(DesiredFrame::Underlay(frame.clone()));
        self.shown_anchors.push(anchor);
        self.shown_bands.push(topmost);
        if let Some(detail) = self.fail_shows.remove(&RenderCarrier::Underlay) {
            return PresentOutcome::failed(detail);
        }
        let mut pixels = vec![0u8; frame.outer.w as usize * frame.outer.h as usize * 4];
        let checksum = crate::group_underlay::paint_fill_argb(
            &mut pixels,
            frame.outer.w,
            frame.outer.h,
            (frame.color.1, frame.color.2, frame.color.3),
            frame.color.0,
        );
        let resolved = (frame.color.1, frame.color.2, frame.color.3);
        self.next_outcome(checksum, resolved, false)
    }

    fn show_preview(&mut self, frame: &PreviewDesired) -> PresentOutcome {
        self.shown.push(DesiredFrame::Preview(frame.clone()));
        if let Some(detail) = self.fail_shows.remove(&RenderCarrier::Preview) {
            return PresentOutcome::failed(detail);
        }
        let mut pixels = vec![0u8; frame.outer.w as usize * frame.outer.h as usize * 4];
        let checksum = crate::group_underlay::paint_fill_argb(
            &mut pixels,
            frame.outer.w,
            frame.outer.h,
            (frame.color.1, frame.color.2, frame.color.3),
            frame.color.0,
        );
        let resolved = (frame.color.1, frame.color.2, frame.color.3);
        self.next_outcome(checksum, resolved, false)
    }

    fn hide(&mut self, carrier: RenderCarrier) -> bool {
        self.hidden.push(carrier);
        !self.fail_hides.remove(&carrier)
    }

    fn destroy_all(&mut self) -> bool {
        self.destroys += 1;
        self.destroy_ok
    }

    fn carrier_hwnd(&self, carrier: RenderCarrier) -> Option<u64> {
        self.hwnds[carrier.index()]
    }
}
