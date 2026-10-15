#![forbid(unsafe_code)]

//! Scoped input eligibility: lock-free cached gate before suppression and
//! registration.
//!
//! Normal `tile --input tap` must not suppress 17 chords globally when the
//! foreground is outside the bundle scope or fullscreen, and Carbon must not
//! hold 17 global interceptions there either. The fix is eligibility BEFORE
//! suppression/registration, not merely dropping already-consumed hits:
//! out-of-scope/fullscreen callbacks Keep/noForward and Carbon RAII
//! registrations RELEASE (dropping hits does NOT restore passthrough).
//!
//! Ownership and isolation (no new threads, no new channels):
//! - Each per-app AX owner publishes ONLY its own PID's candidate facts to
//!   the shared [`EligibilityFactsStore`] from snapshot data it ALREADY read
//!   on its own thread (window facts, Space membership, selected focus,
//!   revision). No AX runs on the input owner or the production main thread:
//!   a hung app stalls at most its own publisher (its record goes stale and
//!   denies) and can never stall keyboard/tap delivery. Records are keyed by
//!   PID, so one background app cannot overwrite the foreground PID's
//!   record.
//! - The input owner (which already owns its Carbon/tap guard) attests from
//!   cheap live reads OUTSIDE the callback (session verdict, Secure Input,
//!   workspace frontmost, SLPS front) plus a plain store snapshot, and
//!   publishes to a lock-free [`EligibilityCache`].
//! - The tap callback reads ONLY atomics plus the monotonic clock (no
//!   `Mutex`, no allocation, no OS/AX/NS calls, no logging) and
//!   Keeps/noForwards while ineligible.
//! - The Carbon owner releases its whole registration while ineligible and
//!   re-arms on return; the tap owner drops its old guard and installs a
//!   FRESH guard on return to eligible (new epoch) to avoid a user-disabled
//!   sticky cycle. A genuine user disable still records a truthful refusal
//!   and is never auto-overridden.
//! - Scoped sources start ineligible until attested facts. The
//!   `input-session` fixture uses an explicit unrestricted test gate for its
//!   known post targets (strict per-edge oracle still enforced in
//!   `gated_post`); normal tile stays scoped by default.
//!
//! Facts (all plain data, unknown/stale denies):
//! - exact scope: frontmost bundle must pass `allows_effect` (Ghostty, own
//!   bundle, protected terminals, unknown never pass);
//! - live foreground: SLPS front PID and workspace frontmost PID must both
//!   equal the frontmost PID (cheap reads outside the callback;
//!   unknown/stale denies);
//! - per-PID record: the foreground PID's publisher record must name the
//!   same bundle, the same topology revision, and a fresh timestamp, with
//!   the selected direct window agreeing with the app-local focused window;
//! - current user Spaces: one set of current user Space IDs across displays
//!   (multi-display supported: each display contributes its own current).
//!   The focused window must sit exactly on one of them
//!   (`spaces == [sid]` with `sid` in the set; sticky/other-Space/unknown/
//!   non-user currents deny);
//! - show state: non-fullscreen/non-minimized/non-hidden (owned native
//!   fullscreen explicitly allowed only via `allow_fullscreen`, e.g. the
//!   proven helper independent-exit leg; normal tile denies fullscreen);
//! - session: unlocked, Secure Input not active, frontmost readable and not
//!   the lock screen.
//!
//! Staleness bounds: per-app snapshots refresh on notifications with a ~1s
//! backstop, so publisher records carry a [`FACTS_TTL`] of ~2s; the input
//! cache gate ([`STALE_AFTER`]) stays 500ms with a 250ms owner attest
//! cadence ([`ATTEST_CADENCE`]). Initially deny; unknown reads deny;
//! termination and topology-revision changes invalidate per-PID/context
//! records. The callback never blocks; production main performs no AX.

use std::collections::BTreeMap;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

/// How often the owner re-attests (upper bound between cheap attest reads).
pub const ATTEST_CADENCE: Duration = Duration::from_millis(250);
/// Past this age without a fresh attest, the input cache denies (stale).
pub const STALE_AFTER: Duration = Duration::from_millis(500);
/// Past this age without a fresh publisher record, attest denies (stale).
/// Per-app snapshots refresh on notifications with a ~1s backstop, so ~2s
/// keeps one missed backstop from flapping while a hung publisher (no
/// record) denies promptly without blocking the input gate.
pub const FACTS_TTL: Duration = Duration::from_millis(2000);

/// Immutable monotonic origin for all eligibility millisecond clocks (cache
/// deadlines and store timestamps share it, so fake-clock tests stay honest
/// without sleeps).
fn origin() -> &'static Instant {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now)
}

/// Monotonic milliseconds since the eligibility origin. Callback-safe:
/// atomics plus a clock read only (no `Mutex`, no allocation, no OS calls).
#[must_use]
pub fn now_ms() -> u64 {
    origin().elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

/// Pure freshness check over explicit millisecond clocks (fake-clock
/// testable, no sleeps): true only when `now_ms` is at or past `updated_ms`
/// (never the future) and within `ttl_ms`.
#[must_use]
pub fn is_fresh_at(updated_ms: u64, now_ms: u64, ttl_ms: u64) -> bool {
    now_ms >= updated_ms && now_ms.saturating_sub(updated_ms) <= ttl_ms
}

/// Plain eligibility facts (no OS touch here; the owner performs the cheap
/// live reads plus a plain store snapshot and passes the outcomes).
/// Unknown/missing facts deny.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EligibilityFacts {
    /// Frontmost PID from workspace (None when unreadable).
    pub frontmost_pid: Option<u32>,
    /// Frontmost bundle (None when unreadable).
    pub frontmost_bundle: Option<String>,
    /// SLPS front PID (None when unreadable).
    pub slps_pid: Option<u32>,
    /// Workspace frontmost PID (None when unreadable; usually equals
    /// `frontmost_pid` — kept separate so disagreement denies).
    pub workspace_pid: Option<u32>,
    /// Whether the frontmost bundle passes the exact scope allowlist
    /// (`allows_effect`; Ghostty/own/protected/unknown already false).
    pub scope_allowed: bool,
    /// Session unlocked (false denies).
    pub session_unlocked: bool,
    /// Secure Input active (true denies; unknown proceeds per contract).
    pub secure_active: Option<bool>,
    /// Lock screen frontmost (true denies).
    pub lock_screen: bool,
    /// Focused window Spaces (empty = unknown, denies).
    pub window_spaces: Vec<u64>,
    /// Current user Space IDs across displays (empty = unknown, denies).
    /// Multi-display supported: each display contributes its own current
    /// user Space; the focused window must sit exactly on one of them.
    /// Never invented from geometry — published from snapshot/SLS rows with
    /// user-type filtering (see [`select_user_currents`]).
    pub current_spaces: Vec<u64>,
    /// Show state (None = unknown, denies).
    pub fullscreen: Option<bool>,
    pub minimized: Option<bool>,
    pub hidden_app: Option<bool>,
    /// Explicit user allow for owned native fullscreen (helper
    /// independent-exit leg only; normal tile false).
    pub allow_fullscreen: bool,
}

/// Pure eligibility verdict over plain facts. `Ok(())` means eligible;
/// `Err(reason)` names the denial with a static token. Unknown/stale
/// denies; out-of-scope, fullscreen (unless explicitly allowed),
/// minimized, hidden, wrong-Space, lock, Secure Input, and foreground
/// disagreement all deny. No titles, keys, or content anywhere.
pub fn eligibility_verdict(facts: &EligibilityFacts) -> Result<(), String> {
    if !facts.session_unlocked {
        return Err("session-not-unlocked".to_owned());
    }
    if facts.secure_active == Some(true) {
        return Err("secure-input-active".to_owned());
    }
    if facts.lock_screen {
        return Err("lock-screen".to_owned());
    }
    let frontmost = facts
        .frontmost_pid
        .ok_or_else(|| "frontmost-unknown".to_owned())?;
    if frontmost <= 1 {
        return Err("frontmost-unknown".to_owned());
    }
    if facts.workspace_pid != Some(frontmost) || facts.slps_pid != Some(frontmost) {
        return Err("foreground-disagreement".to_owned());
    }
    if !facts.scope_allowed {
        return Err("out-of-scope".to_owned());
    }
    if facts.window_spaces.len() != 1 || !facts.current_spaces.contains(&facts.window_spaces[0]) {
        return Err("not-on-current-user-space".to_owned());
    }
    match facts.minimized {
        Some(false) => {}
        _ => return Err("minimized-or-unknown".to_owned()),
    }
    match facts.hidden_app {
        Some(false) => {}
        _ => return Err("hidden-or-unknown".to_owned()),
    }
    match facts.fullscreen {
        Some(false) => {}
        Some(true) if facts.allow_fullscreen => {}
        _ => return Err("fullscreen-or-unknown".to_owned()),
    }
    Ok(())
}

/// One per-PID publisher record: candidate eligibility facts the owning
/// per-app AX thread derived from snapshot data it ALREADY read (window
/// facts, Space membership, selected focus, topology revision). Plain data
/// only (IDs plus status bits — never titles, content, or paths). Keyed by
/// PID in the store, so one app's publisher cannot overwrite another PID's
/// record; each owner publishes ONLY its own bound PID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEligibilityRecord {
    /// The publishing owner's bound PID (must equal the store key).
    pub pid: u32,
    /// Live bundle observed by the owner (`None` when unreadable).
    pub bundle: Option<String>,
    /// App-local focused window (`None` when none).
    pub focused_wid: Option<u32>,
    /// Bounded selected-oracle focus read taken alongside the snapshot:
    /// `(selected_pid, selected_wid)`. Attest requires it to name exactly
    /// `(pid, focused_wid)`; `None` (unreadable there) denies.
    pub selected: Option<(u32, Option<u32>)>,
    /// Live Space membership of the focused window (empty = unknown).
    pub spaces: Vec<u64>,
    /// Show state of the focused window/app (`None` = unknown).
    pub fullscreen: Option<bool>,
    pub minimized: Option<bool>,
    pub hidden_app: Option<bool>,
    /// Topology revision read with the snapshot (must equal the store's
    /// current context revision or the record is superseded).
    pub revision: u64,
    /// Milliseconds since the eligibility origin when published (freshness
    /// against [`FACTS_TTL`]).
    pub updated_ms: u64,
}

/// Shared plain snapshot consulted by the input owner (outside the
/// callback): per-PID publisher records plus the tile-supplied current
/// context. `Mutex` only — the input owner locks it briefly outside the
/// callback; the tap callback NEVER touches it (it reads only the atomic
/// [`EligibilityCache`]).
#[derive(Debug, Default)]
pub struct EligibilityFactsStore {
    inner: Mutex<StoreInner>,
}

#[derive(Debug, Default)]
struct StoreInner {
    apps: BTreeMap<u32, AppEligibilityRecord>,
    current_spaces: Vec<u64>,
    revision: u64,
}

impl EligibilityFactsStore {
    /// Empty store (initially deny: no records, no context).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current monotonic milliseconds on the shared eligibility clock.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        now_ms()
    }

    /// Publish one PID's candidate record. Each per-app AX owner passes ONLY
    /// its own bound PID; the write touches exactly that PID's key, so a
    /// background publisher can never overwrite the foreground PID's
    /// record. Records with `pid` 0/1 are refused (never stored).
    pub fn publish_app(&self, record: AppEligibilityRecord) {
        if record.pid <= 1 {
            return;
        }
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.apps.insert(record.pid, record);
    }

    /// Publish the tile-supplied current context (current user Space IDs
    /// plus the snapshot-revision fingerprint). Called by the tile main
    /// loop from snapshots it already took (no new OS reads for the store
    /// itself); an empty set denies downstream. The list is sorted and
    /// deduped here so evidence diffs stay deterministic.
    pub fn publish_context(&self, current_spaces: Vec<u64>, revision: u64) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let mut sorted = current_spaces;
        sorted.sort_unstable();
        sorted.dedup();
        inner.current_spaces = sorted;
        inner.revision = revision;
    }

    /// Invalidate the current context (no known current Space) while keeping
    /// the revision: topology/snapshot failures deny until the next good
    /// snapshot republishes.
    pub fn invalidate_context(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.current_spaces = Vec::new();
    }

    /// Drop one PID's record (worker terminate/vanish/lock-stop path):
    /// a terminated PID never authorizes.
    pub fn invalidate_pid(&self, pid: u32) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.apps.remove(&pid);
    }

    /// Cloned plain record for one PID (`None` when never published or
    /// invalidated). Input-owner consult only (outside the callback).
    #[must_use]
    pub fn snapshot_for(&self, pid: u32) -> Option<AppEligibilityRecord> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .apps
            .get(&pid)
            .cloned()
    }

    /// Cloned current context `(current_spaces, revision)`.
    #[must_use]
    pub fn context(&self) -> (Vec<u64>, u64) {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        (inner.current_spaces.clone(), inner.revision)
    }
}

/// Select the current user Space IDs from per-display current candidates
/// (pure, no OS calls): keeps `Some` entries that read user-typed, drops
/// `None`/non-user currents, dedupes, and sorts for deterministic evidence.
/// Non-user (fullscreen/system) currents are excluded here — never admitted
/// as eligibility currents. The caller performs the live type reads and
/// passes the outcomes; an empty result denies downstream.
#[must_use]
pub fn select_user_currents(
    currents: Vec<Option<u64>>,
    is_user_space: &dyn Fn(u64) -> bool,
) -> Vec<u64> {
    let mut out: Vec<u64> = currents
        .into_iter()
        .flatten()
        .filter(|sid| is_user_space(*sid))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Pure attest snapshot (no OS calls anywhere on this path): cheap live
/// foreground/session outcomes plus one plain store read, decided without
/// touching AX. The input owner gathers these outside the callback; tests
/// use fake snapshots (including a fake hung publisher via a stale
/// `record_updated_ms`) with a fake `now_ms` — no blocking, no sleeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestSnapshot {
    pub session_unlocked: bool,
    pub secure_active: Option<bool>,
    pub lock_screen: bool,
    pub frontmost_pid: Option<u32>,
    pub frontmost_bundle: Option<String>,
    pub slps_pid: Option<u32>,
    pub workspace_pid: Option<u32>,
    pub scope_allowed: bool,
    pub record: Option<AppEligibilityRecord>,
    pub record_updated_ms: u64,
    pub context_currents: Vec<u64>,
    pub context_revision: u64,
    pub allow_fullscreen: bool,
    pub now_ms: u64,
}

/// Pure attest verdict over one snapshot. Denies (all with static tokens):
/// locked/unknown session, Secure Input, lock screen, unreadable frontmost,
/// SLPS/workspace disagreement, out-of-scope, missing/stale/mismatched
/// publisher record (wrong PID, bundle drift, revision superseded, older
/// than [`FACTS_TTL`]), selected-window disagreement, and (via
/// [`eligibility_verdict`]) wrong-Space plus minimized/hidden/fullscreen
/// show state. A hung publisher (stale record) denies immediately without
/// blocking the gate.
pub fn attest_verdict(s: &AttestSnapshot) -> Result<(), String> {
    if !s.session_unlocked {
        return Err("session-not-unlocked".to_owned());
    }
    if s.secure_active == Some(true) {
        return Err("secure-input-active".to_owned());
    }
    if s.lock_screen {
        return Err("lock-screen".to_owned());
    }
    let frontmost = s
        .frontmost_pid
        .ok_or_else(|| "frontmost-unknown".to_owned())?;
    if frontmost <= 1 {
        return Err("frontmost-unknown".to_owned());
    }
    if s.workspace_pid != Some(frontmost) || s.slps_pid != Some(frontmost) {
        return Err("foreground-disagreement".to_owned());
    }
    if !s.scope_allowed {
        return Err("out-of-scope".to_owned());
    }
    let record = s
        .record
        .as_ref()
        .ok_or_else(|| "facts-unknown".to_owned())?;
    if record.pid != frontmost {
        return Err("facts-stale".to_owned());
    }
    if record.bundle.is_none() || record.bundle != s.frontmost_bundle {
        return Err("facts-stale".to_owned());
    }
    if record.revision != s.context_revision {
        return Err("revision-stale".to_owned());
    }
    if !is_fresh_at(s.record_updated_ms, s.now_ms, FACTS_TTL.as_millis() as u64) {
        return Err("facts-stale".to_owned());
    }
    let focused = record
        .focused_wid
        .filter(|w| *w != 0)
        .ok_or_else(|| "selected-disagreement".to_owned())?;
    if record.selected != Some((frontmost, Some(focused))) {
        return Err("selected-disagreement".to_owned());
    }
    eligibility_verdict(&EligibilityFacts {
        frontmost_pid: Some(frontmost),
        frontmost_bundle: s.frontmost_bundle.clone(),
        slps_pid: s.slps_pid,
        workspace_pid: s.workspace_pid,
        scope_allowed: true,
        session_unlocked: true,
        secure_active: None,
        lock_screen: false,
        window_spaces: record.spaces.clone(),
        current_spaces: s.context_currents.clone(),
        fullscreen: record.fullscreen,
        minimized: record.minimized,
        hidden_app: record.hidden_app,
        allow_fullscreen: s.allow_fullscreen,
    })
}

/// Lock-free cached eligibility published by the input owner from fresh
/// attests and read by the tap callback. Starts ineligible until attested.
/// `unrestricted_test_gate` (input-session fixture only) bypasses
/// scope/Space/showstate for known post targets with a strict per-edge
/// oracle; normal tile never sets it.
///
/// Callback contract: [`is_eligible`] performs atomics plus one monotonic
/// clock read only — no `Mutex`, no allocation, no OS/AX/NS calls, no
/// logging. Publish ordering is deny-first / release-last: revoking stores
/// `eligible=false` BEFORE the deadline/PID, while granting stores the PID
/// and deadline BEFORE `eligible=true`, so the callback never observes a
/// torn eligible PID/epoch.
#[derive(Debug)]
pub struct EligibilityCache {
    eligible: AtomicBool,
    owner_pid: AtomicU32,
    epoch: AtomicU64,
    /// Absolute monotonic deadline (ms since the eligibility origin) while
    /// granted; 0 while denied. Written before `eligible=true` on grant and
    /// after `eligible=false` on revoke.
    expires_ms: AtomicU64,
    unrestricted_test_gate: bool,
}

impl EligibilityCache {
    /// Normal scoped source: starts denied (deadline 0) until attested facts.
    #[must_use]
    pub fn new_scoped() -> Self {
        Self {
            eligible: AtomicBool::new(false),
            owner_pid: AtomicU32::new(0),
            epoch: AtomicU64::new(0),
            expires_ms: AtomicU64::new(0),
            unrestricted_test_gate: false,
        }
    }

    /// Fixture-only unrestricted test gate (input-session known post targets
    /// with strict per-edge oracle). Starts eligible; normal tile must never
    /// use this.
    #[must_use]
    pub fn new_unrestricted_test_gate() -> Self {
        Self {
            eligible: AtomicBool::new(true),
            owner_pid: AtomicU32::new(0),
            epoch: AtomicU64::new(0),
            // No deadline needed (bypasses staleness), but keep it fresh.
            expires_ms: AtomicU64::new(u64::MAX),
            unrestricted_test_gate: true,
        }
    }

    /// Whether this cache is the fixture unrestricted gate.
    #[must_use]
    pub fn is_unrestricted(&self) -> bool {
        self.unrestricted_test_gate
    }

    /// Publish a fresh attest at the current clock (owner thread only).
    /// Deny-first: revoking clears `eligible` BEFORE the deadline/PID;
    /// granting stores PID and deadline BEFORE `eligible`. Bumps the epoch
    /// on every flip.
    pub fn publish(&self, eligible: bool, pid: u32) {
        self.publish_at_ms(eligible, pid, now_ms());
    }

    /// Publish at an explicit millisecond clock (deterministic tests and
    /// single-publisher owner attests; same deny-first/release-last order).
    pub fn publish_at_ms(&self, eligible: bool, pid: u32, now_ms: u64) {
        let prev = self.eligible.load(Ordering::SeqCst);
        if !eligible {
            self.eligible.store(false, Ordering::SeqCst);
            self.owner_pid.store(pid, Ordering::SeqCst);
            self.expires_ms.store(0, Ordering::SeqCst);
        } else {
            self.owner_pid.store(pid, Ordering::SeqCst);
            self.expires_ms.store(
                now_ms.saturating_add(STALE_AFTER.as_millis() as u64),
                Ordering::SeqCst,
            );
            self.eligible.store(true, Ordering::SeqCst);
        }
        if prev != eligible {
            self.epoch.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Callback read at the current clock: eligible AND unexpired. Atomics
    /// plus one monotonic clock read only. The unrestricted test gate
    /// bypasses the deadline (fixture posts are per-edge oracle-gated).
    #[must_use]
    pub fn is_eligible(&self) -> bool {
        self.is_eligible_at_ms(now_ms())
    }

    /// Eligibility read at an explicit millisecond clock (fake-clock
    /// testable, no sleeps; the callback uses [`is_eligible`]).
    #[must_use]
    pub fn is_eligible_at_ms(&self, now_ms: u64) -> bool {
        if self.unrestricted_test_gate {
            return self.eligible.load(Ordering::SeqCst);
        }
        if !self.eligible.load(Ordering::SeqCst) {
            return false;
        }
        let expires = self.expires_ms.load(Ordering::SeqCst);
        expires != 0 && now_ms <= expires
    }

    /// Fresh-source epoch: bumped on every eligibility flip plus every tap
    /// guard recreation on return to eligible.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Owner PID of the last attest (0 when never attested).
    #[must_use]
    pub fn owner_pid(&self) -> u32 {
        self.owner_pid.load(Ordering::SeqCst)
    }

    /// Bump the epoch on tap guard recreation (owner thread only).
    pub fn bump_epoch(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eligible_facts() -> EligibilityFacts {
        EligibilityFacts {
            frontmost_pid: Some(4242),
            frontmost_bundle: Some("com.apple.TextEdit".to_owned()),
            slps_pid: Some(4242),
            workspace_pid: Some(4242),
            scope_allowed: true,
            session_unlocked: true,
            secure_active: Some(false),
            lock_screen: false,
            window_spaces: vec![1],
            current_spaces: vec![1],
            fullscreen: Some(false),
            minimized: Some(false),
            hidden_app: Some(false),
            allow_fullscreen: false,
        }
    }

    fn record(pid: u32, now_ms: u64) -> AppEligibilityRecord {
        AppEligibilityRecord {
            pid,
            bundle: Some("com.apple.TextEdit".to_owned()),
            focused_wid: Some(7),
            selected: Some((pid, Some(7))),
            spaces: vec![1],
            fullscreen: Some(false),
            minimized: Some(false),
            hidden_app: Some(false),
            revision: 9,
            updated_ms: now_ms,
        }
    }

    fn snapshot(now_ms: u64) -> AttestSnapshot {
        AttestSnapshot {
            session_unlocked: true,
            secure_active: Some(false),
            lock_screen: false,
            frontmost_pid: Some(4242),
            frontmost_bundle: Some("com.apple.TextEdit".to_owned()),
            slps_pid: Some(4242),
            workspace_pid: Some(4242),
            scope_allowed: true,
            record: Some(record(4242, now_ms)),
            record_updated_ms: now_ms,
            context_currents: vec![1],
            context_revision: 9,
            allow_fullscreen: false,
            now_ms,
        }
    }

    #[test]
    fn eligible_when_all_facts_attest() {
        assert!(eligibility_verdict(&eligible_facts()).is_ok());
    }

    #[test]
    fn out_of_scope_unknown_and_fullscreen_deny() {
        // Out of scope denies.
        let mut f = eligible_facts();
        f.scope_allowed = false;
        assert_eq!(eligibility_verdict(&f).unwrap_err(), "out-of-scope");
        // Unknown bundle scope denies (caller passes false for unknown).
        let mut f = eligible_facts();
        f.frontmost_bundle = None;
        f.scope_allowed = false;
        assert!(eligibility_verdict(&f).is_err());
        // Unknown frontmost denies.
        let mut f = eligible_facts();
        f.frontmost_pid = None;
        assert!(eligibility_verdict(&f).is_err());
        // Foreground disagreement denies (stale SLPS/workspace).
        let mut f = eligible_facts();
        f.slps_pid = Some(9999);
        assert_eq!(
            eligibility_verdict(&f).unwrap_err(),
            "foreground-disagreement"
        );
        // Fullscreen denies for normal tile, passes with explicit owned allow.
        let mut f = eligible_facts();
        f.fullscreen = Some(true);
        assert!(eligibility_verdict(&f).is_err());
        f.allow_fullscreen = true;
        assert!(eligibility_verdict(&f).is_ok());
        // Minimized/hidden/unknown show state denies.
        for (mini, hidden, full) in [
            (Some(true), Some(false), Some(false)),
            (Some(false), Some(true), Some(false)),
            (None, Some(false), Some(false)),
            (Some(false), None, Some(false)),
            (Some(false), Some(false), None),
        ] {
            let mut f = eligible_facts();
            f.minimized = mini;
            f.hidden_app = hidden;
            f.fullscreen = full;
            assert!(eligibility_verdict(&f).is_err());
        }
    }

    #[test]
    fn space_refusals_and_scope_switch() {
        // Wrong Space, sticky (multi), unknown, and empty currents all deny.
        for (spaces, currents) in [
            (vec![2], vec![1]),
            (vec![1, 2], vec![1]),
            (vec![], vec![1]),
            (vec![1], vec![]),
        ] {
            let mut f = eligible_facts();
            f.window_spaces = spaces;
            f.current_spaces = currents;
            assert!(eligibility_verdict(&f).is_err());
        }
        // Multi-display currents admit the focused window's own current:
        // [22] with currents [11, 22] is eligible (single case [1]/[1]
        // above is unchanged).
        let mut f = eligible_facts();
        f.window_spaces = vec![22];
        f.current_spaces = vec![11, 22];
        assert!(eligibility_verdict(&f).is_ok());
        // Scope switch: eligible, then out-of-scope denies, then re-enabled
        // eligible again (cached exact chords resume).
        let mut f = eligible_facts();
        assert!(eligibility_verdict(&f).is_ok());
        f.scope_allowed = false;
        assert!(eligibility_verdict(&f).is_err());
        f.scope_allowed = true;
        assert!(eligibility_verdict(&f).is_ok());
    }

    #[test]
    fn cache_starts_denied_and_fake_clock_staleness_denies_without_sleeps() {
        let cache = EligibilityCache::new_scoped();
        assert!(!cache.is_eligible_at_ms(1_000));
        assert_eq!(cache.epoch(), 0);
        cache.publish_at_ms(true, 4242, 1_000);
        assert!(cache.is_eligible_at_ms(1_000));
        assert!(cache.is_eligible_at_ms(1_499));
        // Past the 500ms gate the same publish denies — no sleep needed.
        assert!(!cache.is_eligible_at_ms(1_501));
        assert_eq!(cache.owner_pid(), 4242);
        let epoch = cache.epoch();
        // Same value republishes without an epoch bump; a flip bumps.
        cache.publish_at_ms(true, 4242, 1_100);
        assert_eq!(cache.epoch(), epoch);
        cache.publish_at_ms(false, 4242, 1_100);
        assert!(!cache.is_eligible_at_ms(1_100));
        assert_eq!(cache.epoch(), epoch + 1);
        // Unrestricted test gate starts eligible and bypasses the deadline.
        let gate = EligibilityCache::new_unrestricted_test_gate();
        assert!(gate.is_unrestricted());
        assert!(gate.is_eligible_at_ms(u64::MAX));
    }

    #[test]
    fn freshness_helper_never_goes_backwards() {
        assert!(is_fresh_at(1_000, 1_000, 2_000));
        assert!(is_fresh_at(1_000, 3_000, 2_000));
        assert!(!is_fresh_at(1_000, 3_001, 2_000));
        // A record from the future never authorizes.
        assert!(!is_fresh_at(5_000, 1_000, 2_000));
    }

    #[test]
    fn attest_accepts_fresh_matching_record() {
        assert!(attest_verdict(&snapshot(10_000)).is_ok());
    }

    #[test]
    fn attest_denies_hung_publisher_without_blocking() {
        // Fake hung AX publisher: the record simply never refreshes. The
        // gate denies immediately from the stale timestamp — no blocking
        // call, no sleep, input delivery never waits on AX.
        let mut s = snapshot(10_000);
        s.record_updated_ms = 10_000 - FACTS_TTL.as_millis() as u64 - 1;
        s.record.as_mut().unwrap().updated_ms = s.record_updated_ms;
        assert_eq!(attest_verdict(&s).unwrap_err(), "facts-stale");
        // Missing record (unknown) denies the same way.
        let mut s = snapshot(10_000);
        s.record = None;
        assert_eq!(attest_verdict(&s).unwrap_err(), "facts-unknown");
    }

    #[test]
    fn attest_denies_stale_revision_and_drift() {
        // Topology moved on (revision superseded) denies.
        let mut s = snapshot(10_000);
        s.context_revision = 10;
        assert_eq!(attest_verdict(&s).unwrap_err(), "revision-stale");
        // Another PID's record never authorizes this foreground.
        let mut s = snapshot(10_000);
        s.record = Some(record(9999, 10_000));
        assert_eq!(attest_verdict(&s).unwrap_err(), "facts-stale");
        // Bundle drift (possible PID reuse) denies.
        let mut s = snapshot(10_000);
        s.frontmost_bundle = Some("com.apple.Calculator".to_owned());
        assert_eq!(attest_verdict(&s).unwrap_err(), "facts-stale");
        // Selected window disagreeing with the app-local focused window
        // denies (stale Main/focused leg must not authorize).
        let mut s = snapshot(10_000);
        s.record.as_mut().unwrap().selected = Some((4242, Some(9)));
        assert_eq!(attest_verdict(&s).unwrap_err(), "selected-disagreement");
        // Fullscreen, wrong Space, and out-of-scope deny through the same
        // shared verdict as the tile effect fence.
        let mut s = snapshot(10_000);
        s.record.as_mut().unwrap().fullscreen = Some(true);
        assert!(attest_verdict(&s).is_err());
        let mut s = snapshot(10_000);
        s.record.as_mut().unwrap().spaces = vec![2];
        assert!(attest_verdict(&s).is_err());
        let mut s = snapshot(10_000);
        s.scope_allowed = false;
        assert_eq!(attest_verdict(&s).unwrap_err(), "out-of-scope");
    }

    #[test]
    fn attest_two_display_currents_home_second_display() {
        // Two valid current user Spaces (displays 1 and 2): a focused
        // window homed exactly on [22] is eligible — multi-display must
        // not disable all input. Single-display [1]/[1] above is unchanged.
        let mut s = snapshot(10_000);
        s.context_currents = vec![11, 22];
        s.record.as_mut().unwrap().spaces = vec![22];
        assert!(attest_verdict(&s).is_ok());
        // A stale prior current ([21], superseded by a topology move)
        // denies even though it was once current.
        let mut s = snapshot(10_000);
        s.context_currents = vec![11, 22];
        s.record.as_mut().unwrap().spaces = vec![21];
        assert!(attest_verdict(&s).is_err());
        // Sticky (multi-homed) windows deny on any display set.
        let mut s = snapshot(10_000);
        s.context_currents = vec![11, 22];
        s.record.as_mut().unwrap().spaces = vec![11, 22];
        assert!(attest_verdict(&s).is_err());
    }

    #[test]
    fn select_user_currents_drops_unknown_and_nonuser() {
        let no_fs = |sid: u64| sid != 22;
        // Nones drop, non-user (22 here) is excluded, dupes collapse,
        // order is sorted for deterministic evidence.
        assert_eq!(
            select_user_currents(vec![Some(22), None, Some(11), Some(11)], &no_fs),
            vec![11]
        );
        // Two valid currents both survive (multi-display homing).
        let all_user = |_: u64| true;
        assert_eq!(
            select_user_currents(vec![Some(22), Some(11)], &all_user),
            vec![11, 22]
        );
        // All unknown/non-user: empty denies downstream.
        assert!(select_user_currents(vec![None, Some(22)], &no_fs).is_empty());
    }

    #[test]
    fn store_keeps_per_pid_records_independent() {
        let store = EligibilityFactsStore::new();
        // Initially deny: no records, no context.
        assert!(store.snapshot_for(4242).is_none());
        assert_eq!(store.context(), (Vec::new(), 0));
        store.publish_app(record(4242, 1_000));
        store.publish_app(record(9999, 1_000));
        // A background publisher cannot overwrite the foreground record:
        // republishing 9999 leaves 4242 byte-identical.
        let before = store.snapshot_for(4242).unwrap();
        let mut other = record(9999, 2_000);
        other.spaces = vec![5];
        store.publish_app(other);
        assert_eq!(store.snapshot_for(4242).unwrap(), before);
        assert_eq!(store.snapshot_for(9999).unwrap().spaces, vec![5]);
        // Reserved PIDs are never stored.
        store.publish_app(AppEligibilityRecord {
            pid: 0,
            ..record(4242, 2_000)
        });
        assert!(store.snapshot_for(0).is_none());
        // Termination invalidates exactly that PID.
        store.invalidate_pid(4242);
        assert!(store.snapshot_for(4242).is_none());
        assert!(store.snapshot_for(9999).is_some());
        // Context publish/invalidate round-trips (multi-display sets
        // survive verbatim, sorted).
        store.publish_context(vec![22, 11], 9);
        assert_eq!(store.context(), (vec![11, 22], 9));
        store.invalidate_context();
        assert_eq!(store.context(), (Vec::new(), 9));
    }
}
