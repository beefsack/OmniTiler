# Persistence

### Orderly restart adoption
- Decision: Orderly owner restart with apps alive retains host-owned workspace set; re-observe membership without moves and restore focus only where provable. Rebuild layout by re-adoption, never saved-layout restore; intentional floats remain floating through existing restart store. End-session restore reloads saved gaps/settings and re-observes host workspace/membership/maximize state, without invented layout restore. Intentional-float memory need not survive logout/login; reopened apps have new identities.
- Why: the M3 amendment preserves existing within-session float intent, consistent with Tier 1 i3 restart behavior; re-identification is port work, not a product-scope cut.
- Assumes / revisit if: cross-login memory is revisited after 0.1; D25/D26 are verification-only and native outcomes pending. M3 retains macOS intentional-float intent across orderly tiler restart within a session: the port must prove within-session re-identification, exploring session-unique CGWindowID plus process/launch-date fencing and a KDE-style atomic private store. Discard only genuinely unverifiable bindings; no title/PID-only guessing or automatic scope reduction.
- Status: active, 2026-10-12; M3 amendment approved, 2026-11-02
- Evidence: [triage approvals](../research/release-0.1-triage.md#user-decisions-2026-10-12); User 2026-11-02; [i3 in-place restart](../spec/reference-outcomes/restart-persistence.md#r-rst-01-orderly-owner-restart-with-apps-kept-alive); [Apple session uniqueness](https://developer.apple.com/documentation/coregraphics/kcgwindownumber), [M3 exploration](../changes/v4-migration.md#m1-m9-technical-resolution-pending-0c-approval).

### Intentional-float restart store
- Decision: KDE stores planner-owned private runtime intent under session-bus + KWin-owner namespace in `$XDG_RUNTIME_DIR`, 0700 directory/0600 file, atomic replace, version/bounds checks, no symlink following; new KWin/login never adopts old namespace. Store membership intent only, no geometry/focus/stacking/pre-sticky history. Persist explicit float/unfloat only after native success; clear on settled unfloat/verified close; prune only after complete inventory. Missing/corrupt/unreadable/mismatched state degrades to empty with bounded diagnostic; no hold/retry. Write failure retains local/native state; next settled membership update rewrites full set.
- Why: not recorded.
- Assumes / revisit if: this is the sole accepted relaxation of planner no-persistence boundary; hydration grants no write/recovery authority.
- Status: active, 2026-10-08
- Evidence: [KDE restart intent](../changes/archive/kde-intentional-float-restart.md).

### Windows restart intent markers
- Decision: Windows per-window `OmniTiler.*.v1` markers restore classification only, never write/recovery authority; written after native success, removed on unfloat/sticky-off/re-float. No separate store file or recovery-ledger change. Unreadable/NULL means no intent; invalid nonzero values diagnose. Batched hydration precedes tiling; only known automatic fixed provenance permits success-only fixed-tile override write.
- Why: not recorded.
- Assumes / revisit if: fixed-anchor rejection remains unchanged; failed hydration protects only exact lifetimes from tiled writes for current observation, then revalidates next observation. Shared-core duplicate-window rejection remains unchanged unless KDE demonstrates the same harm. Post-restart un-stick remains TBD.
- Status: active, 2026-10-14
- Evidence: [Windows restart](../changes/archive/windows-restart-intent.md), [hydration rejection](../changes/archive/windows-restart-hydration-rejection.md).
