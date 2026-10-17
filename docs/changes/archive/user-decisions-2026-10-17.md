# User decisions 2026-10-17 (Part A record)

Documentation-only record of the user's 2026-10-17 decisions. No code is
implemented by this note; queued work is marked approved-not-implemented
where applicable. Part B implementation status (2026-10-17, after this
record): item 6 below is now implemented offline (locked four-package gates
2,500 passed, 0 failed, 1 ignored; fmt, strict clippy, diff-check pass);
items 1-3 (13g, D23 item 6, 13f) remain approved-not-implemented per the
brief. This header is the only update to this archive; the numbered items
below preserve the Part A snapshot verbatim.

1. 13g unsafe route option A: one minimal safe FFI module on `windows-sys`;
   keep the module minimal.
   [Research](../research/windows-port/safe-win32-wrappers.md).
   P1 Windows unsafe FFI consolidation unblocked: route chosen,
   implementation queued, not started.
2. D23 item 6 option B: forget the source slot on confirmed rehome; a later
   return is admitted fresh.
   [Research](../research/d23-minimized-rehome.md). Shared-core
   implementation (affects KDE) approved, not implemented.
3. 13f option A: preserve the durable tiled base beneath native maximize
   under either born-maximized setting, including hint loss.
   [Research](../research/fixed-override-born-maximized.md). Approved shared
   policy, not implemented.
4. Hidden-workspace presentation: keep `SW_HIDE` for 0.1. Post-0.1 research
   investigates trivial configurability (e.g. off-screen parking for shell
   inclusion); switch-versus-pull remains open in post-0.1 research.
   [Research](../research/windows-port/alt-tab-hidden-workspaces.md).
5. Restart hydration: shared-core duplicate-window rejection unchanged
   unless KDE shows the same harm; the user will test KDE dogfooding.
   [Record](windows-restart-hydration-rejection.md).
6. Warnings only on provably present conflict (supersedes the 2026-10-16
   conservative-warning decisions for BOTH Game Bar and GamingHomeApp).
   Game Bar: positive installed query only; failure/unknown same as absence,
   no notes/conflict rows/badges. GamingHomeApp: per-user config positively
   present/configured only; unknown/absent/unreadable no amber/tray/Settings
   warning. Honest normal-level probe-outcome logs retained. Code
   implementation pending Part B.
7. Pager taskbar auto-hide: hide throughout auto-hide as shipped CONFIRMED,
   no longer tentative.
8. Shortcut Settings dialogs kept as shipped: advisory Apply/Cancel preview,
   no extra Apply confirmation, whole-draft discard. Accepted-for-now; a
   later UX review note is nonblocking.
9. D27 5b folded into the planned post-0.1 `Activation request` setting item
   (P2 0.1 triage settings follow-up); the standalone pending Windows
   implementation entry is removed.
10. Calculator/UWP pager miniature kept as implemented (plain gray, no
    packaged artwork) for now; the user reviews dogfooding.

Consistency: decisions recorded in [decisions](../decisions.md) (correct
sections, superseding tentative/conflicting earlier wording), [backlog](../backlog.md),
[functional spec](../spec/functional-spec.md),
[reference outcomes](../spec/reference-outcomes.md) and the relevant
reference-outcome subfiles and research status lines. The active
[Windows gaming coexistence](../changes/windows-gaming-coexistence.md) record
stays with Part B for warning-code implementation.
