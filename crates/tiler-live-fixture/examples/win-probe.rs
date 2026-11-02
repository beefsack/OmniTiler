//! Windows-only fixture-owned restore probe (offline-reviewed, live-gated).
//!
//! Implemented boundary (only what this file does; no wider claim):
//! - Topmost forced-writer-loss with PREIMAGE restoration (the recoverer
//!   restores the recorded preimage bit, never replays the intended effect).
//! - Zero-write preowned branch (acquisition and recovery write nothing
//!   when the window was already topmost at baseline, regardless of live
//!   state). External clear/set is handled by the same preimage rule and
//!   only logged; no ownership or product-latch inference.
//! - Dedicated marker scalar property plus opaque marker-preimage payload
//!   (<=1024 bytes) persisted before the writer mutation; the recoverer
//!   validates the opaque body and reverts the marker to its preimage.
//! - Bounded HWND churn report (counts, distinct HWNDs, repeats, elapsed;
//!   no reuse-interval claims) plus an old-record-after-recreate refusal
//!   (property absence authorizes nothing).
//! - No teardown on failure/ambiguity: preserve process/windows/files and
//!   report explicit manual guarded recovery. No auto-kill of the fixture.
//!
//! Measurement legs (priorities 1-3) live in `measure123 --leg ...`, reusing
//! the private setup/writer/recoverer machinery in this module: `r14`
//! (acquired-vs-first-seen borderless app enter/exit roundtrip with native
//! per-channel compare), `topmost-*` (preowned refusal, forced loss,
//! external clear, external true-true), `storage-*` (HWND churn, stale
//! record), `dxgi` (read-only source/tool capability report, unimplemented).
//! Exclusive selectors stay out: the fixture picks the first video mode
//! (win.rs `exclusive_mode`), never the current one.
//!
//! Roles (exact subcommands, no generic mutation tool):
//! `setup` (fixture spawn + caller creation + baseline + tag/marker
//! bootstrap; `--topmost-spawn` and `--spawn-modes` only extend the spawn
//! specs for measurement legs), `controller` (full journey), `writer`
//! (distinct fresh process: one guarded mutation set, ready, bounded stay),
//! `recoverer` (manual-friendly: exact run dir + record file, no new
//! windows/writer), `measure123` (one bounded leg per invocation, own
//! fixture server, own run dir).
//!
//! Non-Windows builds compile to a stub; all logic below is `cfg(windows)`.

#[cfg(not(windows))]
fn main() {
    eprintln!("win-probe: Windows-only; stub on this platform.");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let code = probe::run(std::env::args().skip(1).collect());
    std::process::exit(code);
}

#[cfg(windows)]
mod probe {
    #[cfg(test)]
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};

    use tiler_windows::native_guards::{WindowKey, member_matches, visible_lifetime_ok};

    // -----------------------------------------------------------------------
    // Constants (mirrors; fixture is a binary crate, so no import possible).
    // TITLE_SENTINEL mirrors tiler-live-fixture src/protocol.rs; PROBE_PROP
    // is dedicated to this probe and never a production/member property.
    // -----------------------------------------------------------------------
    pub const TITLE_SENTINEL: &str = "OmniTilerTestWindow";
    pub const RECORD_SCHEMA: u32 = 1;
    pub const PROBE_PROP: &str = "OmniTilerProbe.Tag.v1";
    /// Dedicated marker scalar property (never a production/member property).
    /// Setup bootstraps it to a nonzero value; the writer records its preimage
    /// in the record opaque body and the recoverer reverts to that preimage.
    pub const PROBE_MARKER_PROP: &str = "OmniTilerProbe.Marker.v1";
    pub const MAX_OPAQUE_BYTES: usize = 1024;
    /// Medium integrity expectation (SECURITY_MANDATORY_MEDIUM_RID): the
    /// probe binds to this exact value, not merely to matching arbitraries.
    pub const MEDIUM_RID: u32 = 0x2000;
    pub const TOPMOST_FALSE_CODE: u64 = 0xFA15_E000_0000_0001;
    pub const TOPMOST_TRUE_CODE: u64 = 0x7A1E_0000_0000_0001;
    const WS_EX_TOPMOST_BIT: u32 = 0x0000_0008;

    // -----------------------------------------------------------------------
    // Titles: server-controlled `sentinel/id/run-tag` plus optional suffix.
    // Joins use EXACT native text equality; trimming is never used to force
    // a match. `parse_title_identity` independently rejoins caller + run-tag
    // from native text so the live side is never a copy of the expected side.
    // -----------------------------------------------------------------------
    #[must_use]
    pub fn expected_title(id: &str, run_tag: &str, suffix: Option<&str>) -> String {
        let base = format!("{TITLE_SENTINEL}:{id}:{run_tag}");
        match suffix {
            Some(text) if !text.is_empty() => format!("{base} {text}"),
            _ => base,
        }
    }

    /// Parse `(caller, run_tag)` from exact native title text
    /// (`sentinel/caller/run-tag [suffix]`); `None` on any shape mismatch.
    #[must_use]
    pub fn parse_title_identity(native_title: &str) -> Option<(String, String)> {
        let rest = native_title.strip_prefix(&format!("{TITLE_SENTINEL}:"))?;
        let (caller, after) = rest.split_once(':')?;
        let run_tag = after.split_whitespace().next()?;
        if caller.is_empty() || run_tag.is_empty() {
            return None;
        }
        Some((caller.to_string(), run_tag.to_string()))
    }

    #[must_use]
    pub fn valid_probe_tag(tag: &str) -> bool {
        tiler_windows::model::valid_claim_tag(tag)
    }

    #[must_use]
    pub fn generate_probe_tag() -> String {
        let mut bytes = [0u8; 8];
        for _ in 0..4 {
            if getrandom::fill(&mut bytes).is_ok() && bytes.iter().any(|b| *b != 0) {
                return hex_bytes(&bytes);
            }
        }
        // Entropy unavailable: fail visibly with a fixed invalid token so no
        // write can mistake it for a bound tag (format check rejects it).
        String::new()
    }

    /// Full process identity snapshot used for exact kill/verify decisions.
    /// Every field is mandatory: an empty expected side never authorizes.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ProcSnapshot {
        pub pid: u32,
        pub exe: String,
        pub creation: String,
        pub user: String,
        pub session: u32,
        pub integrity: u32,
    }

    /// Exact original-identity match: all six fields equal and every expected
    /// field non-empty/nonzero. Any mismatch or empty expected side refuses
    /// (zero kills). Integrity is additionally bound to [`MEDIUM_RID`] at the
    /// call sites that require medium execution.
    #[must_use]
    pub fn exact_identity_match(expected: &ProcSnapshot, live: &ProcSnapshot) -> bool {
        if expected.pid == 0
            || expected.exe.is_empty()
            || expected.creation.is_empty()
            || expected.user.is_empty()
            || expected.integrity == 0
        {
            return false;
        }
        expected == live
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(DIGITS[(b >> 4) as usize] as char);
            out.push(DIGITS[(b & 0x0f) as usize] as char);
        }
        out
    }

    // -----------------------------------------------------------------------
    // Guard facts: every write carries both trusted-original and fresh-live
    // sides; comparisons happen inside the predicate (injectable, no API).
    // -----------------------------------------------------------------------
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum LiveTag {
        Absent,
        Value(u64),
        Unreadable,
    }

    #[derive(Debug, Clone)]
    pub struct GuardFacts {
        pub stored_key: WindowKey, // trusted roster: hwnd/pid/creation
        pub live_hwnd: u64,
        pub live_pid: u32,
        pub live_creation: String,
        pub expected_exe: String,
        pub live_exe: String,
        pub expected_user: String,
        pub live_user: String,
        pub expected_session: u32,
        pub live_session: u32,
        pub expected_integrity: u32,
        pub live_integrity: u32,
        pub expected_caller: String,
        pub live_caller: String,
        pub expected_title: String,
        pub live_title: String,
        pub stored_tag: String,
        pub live_tag: LiveTag,
        pub exstyle_readable: bool,
        /// True only for the single tag-establishment write itself, which by
        /// definition cannot present a live tag yet. Every other write needs
        /// the live tag to equal the stored one.
        pub bootstrap_tag_establishment: bool,
    }

    fn tag_hex(value: u64) -> String {
        format!("{value:016x}")
    }

    /// Single write gate. Reuses `member_matches` (HWND/PID/creation) and
    /// `visible_lifetime_ok` (stored vs live tag) plus exact exe/user/
    /// session/integrity/caller/title equality on independently read live
    /// values. Absent/unreadable tags and unreadable exstyle fail closed
    /// (except the bootstrap exception, which still requires full
    /// process+window identity and an absent live tag).
    #[must_use]
    pub fn guard_allow(f: &GuardFacts) -> bool {
        if f.expected_caller != f.live_caller {
            return false;
        }
        if f.expected_title != f.live_title {
            return false;
        }
        if f.expected_exe != f.live_exe
            || f.expected_user != f.live_user
            || f.expected_session != f.live_session
            || f.expected_integrity != f.live_integrity
        {
            return false;
        }
        if !member_matches(&f.stored_key, f.live_hwnd, f.live_pid, &f.live_creation) {
            return false;
        }
        if !f.exstyle_readable {
            return false;
        }
        if f.bootstrap_tag_establishment {
            return matches!(f.live_tag, LiveTag::Absent) && valid_probe_tag(&f.stored_tag);
        }
        match &f.live_tag {
            LiveTag::Value(v) => {
                *v != 0 && visible_lifetime_ok(&f.stored_tag, Some(tag_hex(*v).as_str()))
            }
            LiveTag::Absent | LiveTag::Unreadable => false,
        }
    }

    // -----------------------------------------------------------------------
    // Effect counts: baseline/tag setup and policy writes are separate
    // counters. `preowned_setups` counts originally-topmost windows preserved
    // with zero writes at setup. "Zero-write preowned" constrains the POLICY
    // branch counter (baseline/tag setup is counted separately), never a
    // global claim. All counts below are measured instrumented sites.
    // -----------------------------------------------------------------------
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct EffectCounts {
        pub baseline_setups: u64,
        pub preowned_setups: u64,
        pub policy_writes: u64,
    }

    // -----------------------------------------------------------------------
    // Topmost preimage/effect codec: false, true, and absence are three
    // distinct states; both codes are nonzero and distinct.
    // -----------------------------------------------------------------------
    #[must_use]
    pub const fn encode_topmost(top: bool) -> u64 {
        if top {
            TOPMOST_TRUE_CODE
        } else {
            TOPMOST_FALSE_CODE
        }
    }

    #[must_use]
    pub fn decode_topmost(code: u64) -> Option<bool> {
        if code == TOPMOST_TRUE_CODE {
            Some(true)
        } else if code == TOPMOST_FALSE_CODE {
            Some(false)
        } else {
            None
        }
    }

    #[must_use]
    #[allow(dead_code)] // live-leg reserved: read by NativeBackend after review
    pub fn topmost_of_exstyle(exstyle: u32) -> bool {
        exstyle & WS_EX_TOPMOST_BIT != 0
    }

    // -----------------------------------------------------------------------
    // Recovery record: immutable per-write typed evidence. All information
    // the recoverer needs is persisted PRE-write; post-write evidence is
    // supplementary. Same-directory atomic publish (tmp + sync + no-replace
    // move) happens before mutation, never overwritten silently.
    // -----------------------------------------------------------------------
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    pub struct RecoveryRecord {
        pub schema: u32,
        pub fixture_exe: String,
        pub fixture_pid: u32,
        pub fixture_creation: String,
        pub fixture_user: String,
        pub fixture_session: u32,
        pub fixture_integrity: u32,
        pub caller: String,
        pub full_title: String,
        pub tag: String,
        pub hwnd: u64,
        pub effect_topmost: bool,
        pub effect_code: u64,
        pub preimage_topmost: Option<bool>,
        pub opaque_hex: String,
        pub checksum_hex: String,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RecordReject {
        Incomplete,
        Corrupt,
        Version,
        Oversize,
        Mismatch,
    }

    impl std::fmt::Display for RecordReject {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Incomplete => write!(f, "incomplete"),
                Self::Corrupt => write!(f, "corrupt"),
                Self::Version => write!(f, "unknown-version"),
                Self::Oversize => write!(f, "oversize"),
                Self::Mismatch => write!(f, "mismatch"),
            }
        }
    }

    fn record_checksum_body(r: &RecoveryRecord) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:?}|{}",
            r.schema,
            r.fixture_exe,
            r.fixture_pid,
            r.fixture_creation,
            r.fixture_user,
            r.fixture_session,
            r.fixture_integrity,
            r.caller,
            r.full_title,
            r.tag,
            r.hwnd,
            r.effect_code,
            r.preimage_topmost,
            r.opaque_hex,
        )
    }

    fn fnv_u64(text: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in text.bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    fn fnv_hex(text: &str) -> String {
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in text.bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }

    /// Marker-preimage opaque body: `{"marker_preimage":<nonzero u64>}` as
    /// bytes (always far below [`MAX_OPAQUE_BYTES`]). The recoverer decodes
    /// and validates this body, then reverts the marker scalar to the
    /// preimage. Anything else in the opaque field rejects the record.
    #[must_use]
    pub fn encode_marker_opaque(marker_preimage: u64) -> Vec<u8> {
        format!(r#"{{"marker_preimage":{marker_preimage}}}"#).into_bytes()
    }

    #[must_use]
    pub fn decode_marker_opaque(opaque: &[u8]) -> Option<u64> {
        let text = std::str::from_utf8(opaque).ok()?;
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let preimage = value.get("marker_preimage")?.as_u64()?;
        if preimage == 0 { None } else { Some(preimage) }
    }

    /// Deterministic nonzero marker effect derived from the preimage. The
    /// writer and the recoverer share this derivation: the recoverer reverts
    /// only live==preimage (skip, no write) or live==effect(preimage)
    /// (revert); any other drift is preserved with zero marker writes.
    #[must_use]
    pub fn marker_effect_for(preimage: u64) -> u64 {
        preimage.wrapping_add(0x9e37_79b9_7f4a_7c15).max(1)
    }

    /// All inputs the recoverer needs, persisted PRE-write.
    pub struct RecordSpec<'a> {
        pub schema: u32,
        pub fixture_exe: &'a str,
        pub fixture_pid: u32,
        pub fixture_creation: &'a str,
        pub fixture_user: &'a str,
        pub fixture_session: u32,
        pub fixture_integrity: u32,
        pub caller: &'a str,
        pub full_title: &'a str,
        pub tag: &'a str,
        pub hwnd: u64,
        pub effect_topmost: bool,
        pub preimage_topmost: Option<bool>,
        pub opaque: &'a [u8],
    }

    pub fn encode_record(spec: &RecordSpec<'_>) -> Result<String, RecordReject> {
        if spec.opaque.len() > MAX_OPAQUE_BYTES {
            return Err(RecordReject::Oversize);
        }
        if spec.schema != RECORD_SCHEMA {
            return Err(RecordReject::Version);
        }
        // The opaque body must be a valid marker preimage at publish time;
        // a stuffed payload never becomes a publishable record.
        if decode_marker_opaque(spec.opaque).is_none() {
            return Err(RecordReject::Mismatch);
        }
        let mut r = RecoveryRecord {
            schema: spec.schema,
            fixture_exe: spec.fixture_exe.to_string(),
            fixture_pid: spec.fixture_pid,
            fixture_creation: spec.fixture_creation.to_string(),
            fixture_user: spec.fixture_user.to_string(),
            fixture_session: spec.fixture_session,
            fixture_integrity: spec.fixture_integrity,
            caller: spec.caller.to_string(),
            full_title: spec.full_title.to_string(),
            tag: spec.tag.to_string(),
            hwnd: spec.hwnd,
            effect_topmost: spec.effect_topmost,
            effect_code: encode_topmost(spec.effect_topmost),
            preimage_topmost: spec.preimage_topmost,
            opaque_hex: hex_bytes(spec.opaque),
            checksum_hex: String::new(),
        };
        r.checksum_hex = fnv_hex(&record_checksum_body(&r));
        serde_json::to_string(&r).map_err(|_| RecordReject::Corrupt)
    }

    pub fn decode_record(json: &str) -> Result<RecoveryRecord, RecordReject> {
        let r: RecoveryRecord = serde_json::from_str(json).map_err(|_| RecordReject::Incomplete)?;
        if r.schema != RECORD_SCHEMA {
            return Err(RecordReject::Version);
        }
        if r.fixture_exe.is_empty()
            || r.fixture_creation.is_empty()
            || r.fixture_user.is_empty()
            || r.caller.is_empty()
            || r.full_title.is_empty()
        {
            return Err(RecordReject::Incomplete);
        }
        if !valid_probe_tag(&r.tag) {
            return Err(RecordReject::Corrupt);
        }
        if decode_topmost(r.effect_code) != Some(r.effect_topmost) {
            return Err(RecordReject::Mismatch);
        }
        let opaque = unhex(&r.opaque_hex).ok_or(RecordReject::Corrupt)?;
        if opaque.len() > MAX_OPAQUE_BYTES {
            return Err(RecordReject::Oversize);
        }
        // The opaque body must decode to a valid marker preimage; an
        // externally stuffed payload authorizes nothing.
        if decode_marker_opaque(&opaque).is_none() {
            return Err(RecordReject::Mismatch);
        }
        if r.checksum_hex != fnv_hex(&record_checksum_body(&r)) {
            return Err(RecordReject::Mismatch);
        }
        Ok(r)
    }

    fn unhex(raw: &str) -> Option<Vec<u8>> {
        if !raw.len().is_multiple_of(2) {
            return None;
        }
        let mut out = Vec::with_capacity(raw.len() / 2);
        for pair in raw.as_bytes().chunks(2) {
            let hi = hex_val(pair[0])?;
            let lo = hex_val(pair[1])?;
            out.push(hi << 4 | lo);
        }
        Some(out)
    }

    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            _ => None,
        }
    }

    /// Fresh observation the recoverer rejoins independently.
    #[derive(Debug, Clone)]
    pub struct FreshObs {
        pub hwnd: u64,
        pub pid: u32,
        pub creation: String,
        pub exe: String,
        pub user: String,
        pub session: u32,
        pub integrity: u32,
        pub caller: String,
        pub title: String,
        pub live_tag: LiveTag,
    }

    /// Record authorizes a restore only while the exact original fixture
    /// launch identity, caller/title, HWND triple, and live tag all still
    /// agree. A stale tag or a same-process recycled HWND (triple agrees,
    /// tag differs) is rejected with no writes.
    #[must_use]
    pub fn record_authorizes(record: &RecoveryRecord, obs: &FreshObs) -> bool {
        if record.hwnd != obs.hwnd || record.caller != obs.caller || record.full_title != obs.title
        {
            return false;
        }
        if record.fixture_pid != obs.pid
            || record.fixture_creation != obs.creation
            || record.fixture_exe != obs.exe
            || record.fixture_user != obs.user
            || record.fixture_session != obs.session
            || record.fixture_integrity != obs.integrity
        {
            return false;
        }
        match &obs.live_tag {
            LiveTag::Value(v) => {
                *v != 0 && visible_lifetime_ok(&record.tag, Some(tag_hex(*v).as_str()))
            }
            LiveTag::Absent | LiveTag::Unreadable => false,
        }
    }

    // -----------------------------------------------------------------------
    // Hide/reveal recovery record (area 5): persists the PRE-hide baseline
    // (visibility, show-state, outer/DWM geometry) plus the marker preimage.
    // Distinct `kind` discriminator so the recoverer routes hide records to
    // the visibility path and topmost records to the existing path.
    // -----------------------------------------------------------------------
    pub const HIDE_RECORD_KIND: &str = "hide-v1";

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    pub struct HideRecord {
        pub kind: String,
        pub schema: u32,
        pub fixture_exe: String,
        pub fixture_pid: u32,
        pub fixture_creation: String,
        pub fixture_user: String,
        pub fixture_session: u32,
        pub fixture_integrity: u32,
        pub caller: String,
        pub full_title: String,
        pub tag: String,
        pub hwnd: u64,
        pub preimage_visible: bool,
        pub preimage_show: Option<u32>,
        pub preimage_outer: Option<[i32; 4]>,
        pub preimage_dwm: Option<[i32; 4]>,
        pub opaque_hex: String,
        pub checksum_hex: String,
    }

    pub struct HideSpec<'a> {
        pub schema: u32,
        pub fixture_exe: &'a str,
        pub fixture_pid: u32,
        pub fixture_creation: &'a str,
        pub fixture_user: &'a str,
        pub fixture_session: u32,
        pub fixture_integrity: u32,
        pub caller: &'a str,
        pub full_title: &'a str,
        pub tag: &'a str,
        pub hwnd: u64,
        pub preimage_visible: bool,
        pub preimage_show: Option<u32>,
        pub preimage_outer: Option<[i32; 4]>,
        pub preimage_dwm: Option<[i32; 4]>,
        pub opaque: &'a [u8],
    }

    fn hide_checksum_body(r: &HideRecord) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:?}|{:?}|{:?}|{}",
            r.kind,
            r.schema,
            r.fixture_exe,
            r.fixture_pid,
            r.fixture_creation,
            r.fixture_user,
            r.fixture_session,
            r.fixture_integrity,
            r.caller,
            r.full_title,
            r.tag,
            r.hwnd,
            r.preimage_visible,
            r.preimage_show,
            r.preimage_outer,
            r.opaque_hex,
        )
    }

    pub fn encode_hide_record(spec: &HideSpec<'_>) -> Result<String, RecordReject> {
        if spec.opaque.len() > MAX_OPAQUE_BYTES {
            return Err(RecordReject::Oversize);
        }
        if spec.schema != RECORD_SCHEMA {
            return Err(RecordReject::Version);
        }
        if decode_marker_opaque(spec.opaque).is_none() {
            return Err(RecordReject::Mismatch);
        }
        let mut r = HideRecord {
            kind: HIDE_RECORD_KIND.to_string(),
            schema: spec.schema,
            fixture_exe: spec.fixture_exe.to_string(),
            fixture_pid: spec.fixture_pid,
            fixture_creation: spec.fixture_creation.to_string(),
            fixture_user: spec.fixture_user.to_string(),
            fixture_session: spec.fixture_session,
            fixture_integrity: spec.fixture_integrity,
            caller: spec.caller.to_string(),
            full_title: spec.full_title.to_string(),
            tag: spec.tag.to_string(),
            hwnd: spec.hwnd,
            preimage_visible: spec.preimage_visible,
            preimage_show: spec.preimage_show,
            preimage_outer: spec.preimage_outer,
            preimage_dwm: spec.preimage_dwm,
            opaque_hex: hex_bytes(spec.opaque),
            checksum_hex: String::new(),
        };
        r.checksum_hex = fnv_hex(&hide_checksum_body(&r));
        serde_json::to_string(&r).map_err(|_| RecordReject::Corrupt)
    }

    pub fn decode_hide_record(json: &str) -> Result<HideRecord, RecordReject> {
        let r: HideRecord = serde_json::from_str(json).map_err(|_| RecordReject::Incomplete)?;
        if r.kind != HIDE_RECORD_KIND {
            return Err(RecordReject::Mismatch);
        }
        if r.schema != RECORD_SCHEMA {
            return Err(RecordReject::Version);
        }
        if r.fixture_exe.is_empty()
            || r.fixture_creation.is_empty()
            || r.fixture_user.is_empty()
            || r.caller.is_empty()
            || r.full_title.is_empty()
        {
            return Err(RecordReject::Incomplete);
        }
        if !valid_probe_tag(&r.tag) {
            return Err(RecordReject::Corrupt);
        }
        let opaque = unhex(&r.opaque_hex).ok_or(RecordReject::Corrupt)?;
        if opaque.len() > MAX_OPAQUE_BYTES {
            return Err(RecordReject::Oversize);
        }
        if decode_marker_opaque(&opaque).is_none() {
            return Err(RecordReject::Mismatch);
        }
        if r.checksum_hex != fnv_hex(&hide_checksum_body(&r)) {
            return Err(RecordReject::Mismatch);
        }
        Ok(r)
    }

    /// Hide-record authorization: same exact original fixture launch
    /// identity, caller/title, HWND triple, and live tag as the topmost
    /// path. Geometry/visibility are restored state, never authorization.
    #[must_use]
    pub fn hide_record_authorizes(record: &HideRecord, obs: &FreshObs) -> bool {
        if record.hwnd != obs.hwnd || record.caller != obs.caller || record.full_title != obs.title
        {
            return false;
        }
        if record.fixture_pid != obs.pid
            || record.fixture_creation != obs.creation
            || record.fixture_exe != obs.exe
            || record.fixture_user != obs.user
            || record.fixture_session != obs.session
            || record.fixture_integrity != obs.integrity
        {
            return false;
        }
        match &obs.live_tag {
            LiveTag::Value(v) => {
                *v != 0 && visible_lifetime_ok(&record.tag, Some(tag_hex(*v).as_str()))
            }
            LiveTag::Absent | LiveTag::Unreadable => false,
        }
    }

    // -----------------------------------------------------------------------
    // Backend seam: live native calls behind a trait; unit tests inject the
    // fake and assert zero backend calls on every invalid state. Every write
    // chokepoint refreshes facts INSIDE via observe() immediately before the
    // call; callers never pass a stale GuardFacts across the write.
    // -----------------------------------------------------------------------
    /// Write journal entries for the injectable fake backend (tests only).
    #[cfg(test)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WriteKind {
        Topmost(bool),
        Tag(u64),
        Marker(u64),
    }

    #[allow(dead_code)]
    pub trait ProbeBackend {
        /// Fresh read of BOTH sides immediately before a write. Returns None
        /// when any live side is unreadable (fail-closed, zero writes).
        fn observe(&mut self, hwnd: u64) -> Option<GuardFacts>;
        fn read_tag(&self, hwnd: u64) -> LiveTag;
        fn read_exstyle(&self, hwnd: u64) -> Option<u32>;
        fn read_topmost(&self, hwnd: u64) -> Option<bool>;
        fn read_marker(&self, hwnd: u64) -> Option<u64>;
        fn write_topmost(&mut self, hwnd: u64, top: bool) -> bool;
        fn write_tag(&mut self, hwnd: u64, value: u64) -> bool;
        fn write_marker(&mut self, hwnd: u64, value: u64) -> bool;
    }

    #[cfg(test)]
    #[derive(Debug, Default)]
    #[allow(dead_code)]
    pub struct FakeBackend {
        pub tags: HashMap<u64, u64>,
        pub unreadable_tags: HashSet<u64>,
        pub exstyle: HashMap<u64, u32>,
        pub unreadable_exstyle: HashSet<u64>,
        pub topmost: HashMap<u64, bool>,
        pub unreadable_topmost: HashSet<u64>,
        pub markers: HashMap<u64, u64>,
        pub unreadable_markers: HashSet<u64>,
        pub fail_writes: bool,
        pub writes: Vec<(u64, WriteKind)>,
        /// Scripted fresh observations. When non-empty, observe() pops the
        /// front (None = unreadable). When empty, observe() returns None
        /// (fail-closed) so tests must script the pre/post reads explicitly.
        pub observe_queue: std::collections::VecDeque<Option<GuardFacts>>,
    }

    #[cfg(test)]
    impl ProbeBackend for FakeBackend {
        fn observe(&mut self, _hwnd: u64) -> Option<GuardFacts> {
            if self.observe_queue.is_empty() {
                return None;
            }
            self.observe_queue.pop_front().flatten()
        }
        fn read_tag(&self, hwnd: u64) -> LiveTag {
            if self.unreadable_tags.contains(&hwnd) {
                LiveTag::Unreadable
            } else {
                self.tags
                    .get(&hwnd)
                    .copied()
                    .map_or(LiveTag::Absent, LiveTag::Value)
            }
        }
        fn read_exstyle(&self, hwnd: u64) -> Option<u32> {
            if self.unreadable_exstyle.contains(&hwnd) {
                None
            } else {
                self.exstyle.get(&hwnd).copied()
            }
        }
        fn read_topmost(&self, hwnd: u64) -> Option<bool> {
            if self.unreadable_topmost.contains(&hwnd) {
                None
            } else {
                self.topmost.get(&hwnd).copied()
            }
        }
        fn read_marker(&self, hwnd: u64) -> Option<u64> {
            if self.unreadable_markers.contains(&hwnd) {
                None
            } else {
                self.markers.get(&hwnd).copied()
            }
        }
        fn write_topmost(&mut self, hwnd: u64, top: bool) -> bool {
            self.writes.push((hwnd, WriteKind::Topmost(top)));
            if self.fail_writes {
                return false;
            }
            self.topmost.insert(hwnd, top);
            true
        }
        fn write_tag(&mut self, hwnd: u64, value: u64) -> bool {
            self.writes.push((hwnd, WriteKind::Tag(value)));
            if self.fail_writes {
                return false;
            }
            self.tags.insert(hwnd, value);
            true
        }
        fn write_marker(&mut self, hwnd: u64, value: u64) -> bool {
            self.writes.push((hwnd, WriteKind::Marker(value)));
            if self.fail_writes {
                return false;
            }
            self.markers.insert(hwnd, value);
            true
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WriteOutcome {
        AppliedVerified,
        PostcheckFailed,
        Refused,
    }

    /// Trusted-side equality: the fresh observe must carry the same roster
    /// key, tag, and expected launch identity as the pre-check side. A scripted
    /// observation that moves after record publication fails here with zero
    /// writes.
    #[must_use]
    pub fn same_authorization(expected: &GuardFacts, fresh: &GuardFacts) -> bool {
        expected.stored_key == fresh.stored_key
            && expected.stored_tag == fresh.stored_tag
            && expected.expected_exe == fresh.expected_exe
            && expected.expected_user == fresh.expected_user
            && expected.expected_session == fresh.expected_session
            && expected.expected_integrity == fresh.expected_integrity
            && expected.expected_caller == fresh.expected_caller
            && expected.expected_title == fresh.expected_title
    }

    /// The one guarded-write chokepoint for topmost effects. Refreshes facts
    /// INSIDE via observe() immediately before the call; stale caller facts
    /// never authorize. Full postcheck re-observes and requires the requested
    /// topmost readback.
    pub fn guarded_topmost_write(
        backend: &mut impl ProbeBackend,
        expected: &GuardFacts,
        top: bool,
        counts: &mut EffectCounts,
    ) -> WriteOutcome {
        let hwnd = expected.stored_key.hwnd;
        let Some(fresh) = backend.observe(hwnd) else {
            return WriteOutcome::Refused;
        };
        if !guard_allow(&fresh) || !same_authorization(expected, &fresh) {
            return WriteOutcome::Refused;
        }
        if !backend.write_topmost(hwnd, top) {
            return WriteOutcome::Refused;
        }
        counts.policy_writes += 1;
        let Some(post) = backend.observe(hwnd) else {
            return WriteOutcome::PostcheckFailed;
        };
        if !guard_allow(&post)
            || !same_authorization(expected, &post)
            || backend.read_topmost(hwnd) != Some(top)
        {
            return WriteOutcome::PostcheckFailed;
        }
        WriteOutcome::AppliedVerified
    }

    /// Marker scalar chokepoint (dedicated probe property only). Same
    /// refresh-inside + full postcheck shape as topmost.
    pub fn guarded_marker_write(
        backend: &mut impl ProbeBackend,
        expected: &GuardFacts,
        value: u64,
        counts: &mut EffectCounts,
    ) -> WriteOutcome {
        let hwnd = expected.stored_key.hwnd;
        let Some(fresh) = backend.observe(hwnd) else {
            return WriteOutcome::Refused;
        };
        if !guard_allow(&fresh) || !same_authorization(expected, &fresh) {
            return WriteOutcome::Refused;
        }
        if !backend.write_marker(hwnd, value) {
            return WriteOutcome::Refused;
        }
        counts.policy_writes += 1;
        let Some(post) = backend.observe(hwnd) else {
            return WriteOutcome::PostcheckFailed;
        };
        if !guard_allow(&post)
            || !same_authorization(expected, &post)
            || backend.read_marker(hwnd) != Some(value)
        {
            return WriteOutcome::PostcheckFailed;
        }
        WriteOutcome::AppliedVerified
    }

    /// Guarded visibility write (hide/reveal) for the area-5 leg. Same
    /// refresh-inside shape as the topmost chokepoint but over the roster
    /// rejoin (no new trait method): re-observes via `rejoin_live`
    /// immediately before `ShowWindow`, requires the requested visibility
    /// readback, then re-observes for the postcheck. Counts one policy
    /// write on the attempted effect.
    fn guarded_visibility_write(
        handoff: &Handoff,
        entry: &RosterEntry,
        expected: &GuardFacts,
        cmd: i32,
        expect_visible: bool,
        counts: &mut EffectCounts,
    ) -> WriteOutcome {
        let hwnd = entry.hwnd;
        let Ok((fresh, _)) = rejoin_live(handoff, entry, hwnd) else {
            return WriteOutcome::Refused;
        };
        if !guard_allow(&fresh) || !same_authorization(expected, &fresh) {
            return WriteOutcome::Refused;
        }
        tiler_windows::ffi::window::show_window(hwnd, cmd);
        counts.policy_writes += 1;
        if !ffi_ext::wait_visible(hwnd, expect_visible, 5_000) {
            return WriteOutcome::PostcheckFailed;
        }
        let Ok((post, _)) = rejoin_live(handoff, entry, hwnd) else {
            return WriteOutcome::PostcheckFailed;
        };
        if !guard_allow(&post) || !same_authorization(expected, &post) {
            return WriteOutcome::PostcheckFailed;
        }
        WriteOutcome::AppliedVerified
    }

    /// Guarded geometry restore to the recorded PRE-hide outer rect via
    /// native `SetWindowPos` (no Z-order/activation change). Re-verifies the
    /// roster join immediately before the move; counts one policy write on
    /// the attempt and requires the outer readback to match.
    fn guarded_geometry_restore(
        handoff: &Handoff,
        entry: &RosterEntry,
        expected: &GuardFacts,
        outer: [i32; 4],
        counts: &mut EffectCounts,
    ) -> WriteOutcome {
        let hwnd = entry.hwnd;
        let Ok((fresh, _)) = rejoin_live(handoff, entry, hwnd) else {
            return WriteOutcome::Refused;
        };
        if !guard_allow(&fresh) || !same_authorization(expected, &fresh) {
            return WriteOutcome::Refused;
        }
        const SWP_NOZORDER: u32 = 0x0004;
        const SWP_NOACTIVATE: u32 = 0x0010;
        let (x, y) = (outer[0], outer[1]);
        let (w, h) = (outer[2] - outer[0], outer[3] - outer[1]);
        if w <= 0 || h <= 0 {
            return WriteOutcome::Refused;
        }
        if !tiler_windows::ffi::window::set_window_pos(
            hwnd,
            x,
            y,
            w,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        ) {
            return WriteOutcome::Refused;
        }
        counts.policy_writes += 1;
        let start = std::time::Instant::now();
        loop {
            let cur = tiler_windows::ffi::window::window_rect(hwnd)
                .map(|r| [r.left, r.top, r.right, r.bottom]);
            if cur == Some(outer) {
                break;
            }
            if start.elapsed().as_millis() >= 5_000 {
                return WriteOutcome::PostcheckFailed;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let Ok((post, _)) = rejoin_live(handoff, entry, hwnd) else {
            return WriteOutcome::PostcheckFailed;
        };
        if !guard_allow(&post) || !same_authorization(expected, &post) {
            return WriteOutcome::PostcheckFailed;
        }
        WriteOutcome::AppliedVerified
    }

    /// Tag bootstrap chokepoint (the only write allowed under the bootstrap
    /// exception): establishes the dedicated nonzero tag, counted as a
    /// baseline setup, never as a policy write. Refreshes inside.
    pub fn bootstrap_tag(
        backend: &mut impl ProbeBackend,
        expected: &GuardFacts,
        counts: &mut EffectCounts,
    ) -> bool {
        if !expected.bootstrap_tag_establishment {
            return false;
        }
        let hwnd = expected.stored_key.hwnd;
        let Some(fresh) = backend.observe(hwnd) else {
            return false;
        };
        if !same_authorization(expected, &fresh) {
            return false;
        }
        // Single dedicated establishment exception: the live adapter always
        // reports fresh facts with `bootstrap_tag_establishment == false`,
        // so the caller's local exception never reaches the fresh side.
        // Apply the exception to the freshly verified facts here only, still
        // requiring an absent live prop, a valid nonzero stored tag, and the
        // full process+window identity inside `guard_allow`. Postcheck below
        // keeps no exception.
        if fresh.live_tag != LiveTag::Absent || !valid_probe_tag(&expected.stored_tag) {
            return false;
        }
        let mut fresh_exc = fresh.clone();
        fresh_exc.bootstrap_tag_establishment = true;
        if !guard_allow(&fresh_exc) {
            return false;
        }
        let value = unhex16(&expected.stored_tag);
        if value == 0 || !backend.write_tag(hwnd, value) {
            return false;
        }
        let Some(post) = backend.observe(hwnd) else {
            return false;
        };
        if !same_authorization(expected, &post) {
            return false;
        }
        // Postcheck: the live tag must now equal the stored tag (bootstrap
        // exception no longer applies once the tag exists).
        let mut post_no_bootstrap = post.clone();
        post_no_bootstrap.bootstrap_tag_establishment = false;
        if !guard_allow(&post_no_bootstrap) {
            return false;
        }
        counts.baseline_setups += 1;
        true
    }

    fn unhex16(tag: &str) -> u64 {
        u64::from_str_radix(tag, 16).unwrap_or(0)
    }

    /// Two-pass teardown planner (pure): returns the ordered close list only
    /// when every caller verified clean; any unclean/unknown caller yields
    /// None and the live leg must send zero mutation protocol.
    #[must_use]
    pub fn planned_closes(clean_per_caller: &[(&str, bool)]) -> Option<Vec<String>> {
        if clean_per_caller.is_empty() {
            return None;
        }
        for (_, clean) in clean_per_caller {
            if !clean {
                return None;
            }
        }
        Some(
            clean_per_caller
                .iter()
                .map(|(caller, _)| (*caller).to_string())
                .collect(),
        )
    }

    /// Acquisition policy branch. A preowned window (originally topmost, or
    /// foreign-owned) authorizes zero policy writes: the branch returns
    /// without calling the backend at all.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AcquireOutcome {
        Acquired,
        PreownedNoWrite,
    }

    pub fn acquire_for_write(preowned: bool) -> AcquireOutcome {
        if preowned {
            AcquireOutcome::PreownedNoWrite
        } else {
            AcquireOutcome::Acquired
        }
    }

    // -----------------------------------------------------------------------
    // Recovery policy: restore the recorded PREIMAGE, never replay the effect.
    // preimage=false + intended=true + live=true => guarded set false.
    // live==preimage => already clear, zero writes. preowned => zero writes
    // regardless of live state. External true->true is unobservable bit
    // ownership and is handled by the same preimage rule (revert to false
    // when the preimage is false).
    // -----------------------------------------------------------------------
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RecoverOutcome {
        Restored,
        NoWriteAlreadyAtPreimage,
        NoWritePreowned,
        /// One field was restored while the other could not be completed
        /// (marker unknown drift preserved, or a second write refused after
        /// an earlier write). `counts.policy_writes` holds the actual writes.
        PartiallyRestored,
        RefusedNoWrites,
    }

    fn fresh_obs_from_facts(f: &GuardFacts) -> FreshObs {
        FreshObs {
            hwnd: f.live_hwnd,
            pid: f.live_pid,
            creation: f.live_creation.clone(),
            exe: f.live_exe.clone(),
            user: f.live_user.clone(),
            session: f.live_session,
            integrity: f.live_integrity,
            caller: f.live_caller.clone(),
            title: f.live_title.clone(),
            live_tag: f.live_tag.clone(),
        }
    }

    pub fn recover_from_record(
        record_json: &str,
        preowned: bool,
        backend: &mut impl ProbeBackend,
        counts: &mut EffectCounts,
    ) -> RecoverOutcome {
        let record = match decode_record(record_json) {
            Ok(r) => r,
            Err(_) => return RecoverOutcome::RefusedNoWrites,
        };
        // Preowned short-circuits before ANY backend contact: zero writes
        // regardless of live state.
        if preowned {
            return RecoverOutcome::NoWritePreowned;
        }
        let Some(preimage) = record.preimage_topmost else {
            return RecoverOutcome::RefusedNoWrites;
        };
        // Fresh read INSIDE the recoverer: the only authorization input.
        let hwnd = record.hwnd;
        let Some(fresh) = backend.observe(hwnd) else {
            return RecoverOutcome::RefusedNoWrites;
        };
        let obs = fresh_obs_from_facts(&fresh);
        if !record_authorizes(&record, &obs) {
            return RecoverOutcome::RefusedNoWrites;
        }
        // Record-side expected facts for the chokepoint's stored-side check.
        let expected = GuardFacts {
            stored_key: WindowKey {
                hwnd: record.hwnd,
                pid: fresh.live_pid,
                creation: fresh.live_creation.clone(),
            },
            live_hwnd: fresh.live_hwnd,
            live_pid: fresh.live_pid,
            live_creation: fresh.live_creation.clone(),
            expected_exe: record.fixture_exe.clone(),
            live_exe: fresh.live_exe.clone(),
            expected_user: record.fixture_user.clone(),
            live_user: fresh.live_user.clone(),
            expected_session: record.fixture_session,
            live_session: fresh.live_session,
            expected_integrity: record.fixture_integrity,
            live_integrity: fresh.live_integrity,
            expected_caller: record.caller.clone(),
            live_caller: fresh.live_caller.clone(),
            expected_title: record.full_title.clone(),
            live_title: fresh.live_title.clone(),
            stored_tag: record.tag.clone(),
            live_tag: fresh.live_tag.clone(),
            exstyle_readable: true,
            bootstrap_tag_establishment: false,
        };
        // The stored-side check above uses the freshly read live sides; the
        // chokepoint below re-observes before mutating, so a scripted change
        // between publication and effect still refuses with zero writes.
        // Decode the marker preimage BEFORE any write so a corrupt body
        // never leaves a half-restored window misreported as refused.
        let opaque = match unhex(&record.opaque_hex).and_then(|b| decode_marker_opaque(&b)) {
            Some(v) => v,
            None => return RecoverOutcome::RefusedNoWrites,
        };
        let intended_marker = marker_effect_for(opaque);
        let live = match backend.read_topmost(hwnd) {
            Some(v) => v,
            None => return RecoverOutcome::RefusedNoWrites,
        };
        // Reconcile fields independently: only no-op when BOTH topmost and
        // marker already match their preimages. An unreadable marker fails
        // closed with zero marker writes; an unknown drift value (neither
        // preimage nor the exact intended effect) is preserved with zero
        // marker writes.
        let live_marker_once = backend.read_marker(hwnd);
        let need_top = live != preimage;
        let marker_known_need = match live_marker_once {
            Some(m) if m == opaque => None,
            Some(m) if m == intended_marker => Some(true),
            _ => Some(false),
        };
        let marker_unknown = marker_known_need == Some(false);
        if !need_top && marker_known_need.is_none() {
            return RecoverOutcome::NoWriteAlreadyAtPreimage;
        }
        if marker_unknown && !need_top {
            // Topmost already at preimage; unknown marker drift is preserved.
            return RecoverOutcome::RefusedNoWrites;
        }
        if need_top {
            match guarded_topmost_write(backend, &expected, preimage, counts) {
                WriteOutcome::AppliedVerified => {}
                WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                    return RecoverOutcome::RefusedNoWrites;
                }
            }
        }
        match marker_known_need {
            None => RecoverOutcome::Restored,
            Some(false) => {
                // Unknown drift preserved; a just-completed topmost write
                // makes this partial, never a zero-write refusal.
                if need_top {
                    RecoverOutcome::PartiallyRestored
                } else {
                    RecoverOutcome::RefusedNoWrites
                }
            }
            Some(true) => match guarded_marker_write(backend, &expected, opaque, counts) {
                WriteOutcome::AppliedVerified => RecoverOutcome::Restored,
                WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                    if need_top || counts.policy_writes > 0 {
                        RecoverOutcome::PartiallyRestored
                    } else {
                        RecoverOutcome::RefusedNoWrites
                    }
                }
            },
        }
    }

    // -----------------------------------------------------------------------
    // Example-local reviewed FFI gap module (sole unsafe owner in this
    // example): checked exstyle read (the shared raw `window_exstyle` cannot
    // discriminate errors) and a title-text getter for the exact
    // sentinel/id/run-tag join. No other unsafe in this file.
    // -----------------------------------------------------------------------
    #[allow(unsafe_code)]
    mod ffi_ext {
        use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
        };

        /// Checked exstyle read mirroring `window_style_checked`: `None`
        /// only when the word reads zero with a live error.
        pub fn window_exstyle_checked(hwnd: u64) -> Option<u32> {
            unsafe {
                SetLastError(0);
                let value = GetWindowLongW(hwnd as isize as _, GWL_EXSTYLE);
                let err = GetLastError();
                if value == 0 && err != 0 {
                    None
                } else {
                    Some(value as u32)
                }
            }
        }

        /// Best-effort window title text; `None` when unreadable.
        /// Never used as a write vector: join evidence only.
        pub fn window_text(hwnd: u64) -> Option<String> {
            let len = unsafe { GetWindowTextLengthW(hwnd as isize as _) };
            if len < 0 {
                return None;
            }
            // Length excludes the NUL; empty titles read as Some("").
            let mut buf = vec![0u16; (len as usize) + 1];
            let copied =
                unsafe { GetWindowTextW(hwnd as isize as _, buf.as_mut_ptr(), buf.len() as i32) };
            if copied == 0 {
                let err = unsafe { GetLastError() };
                if err != 0 {
                    return None;
                }
                return Some(String::new());
            }
            Some(String::from_utf16_lossy(&buf[..copied as usize]))
        }

        /// Screen-space client rect via GetClientRect + ClientToScreen
        /// mapping of both corners. `None` when the window is gone or the
        /// mapping fails. Read-only geometry evidence.
        pub fn client_rect_screen(hwnd: u64) -> Option<(i32, i32, i32, i32)> {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
            use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
            unsafe {
                let mut rc: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
                if GetClientRect(hwnd as isize as _, &mut rc) == 0 {
                    return None;
                }
                let mut top_left = POINT {
                    x: rc.left,
                    y: rc.top,
                };
                let mut bottom_right = POINT {
                    x: rc.right,
                    y: rc.bottom,
                };
                if ClientToScreen(hwnd as isize as _, &mut top_left) == 0 {
                    return None;
                }
                if ClientToScreen(hwnd as isize as _, &mut bottom_right) == 0 {
                    return None;
                }
                Some((top_left.x, top_left.y, bottom_right.x, bottom_right.y))
            }
        }

        /// Virtual-desktop rect `(x, y, w, h)` via `GetSystemMetrics`.
        /// Read-only; used to normalize absolute `SendInput` coordinates.
        pub fn virtual_desktop_px() -> Option<(i32, i32, i32, i32)> {
            use windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics;
            const SM_XVIRTUALSCREEN: i32 = 76;
            const SM_YVIRTUALSCREEN: i32 = 77;
            const SM_CXVIRTUALSCREEN: i32 = 78;
            const SM_CYVIRTUALSCREEN: i32 = 79;
            unsafe {
                let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
                let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
                let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
                if w <= 0 || h <= 0 {
                    return None;
                }
                Some((x, y, w, h))
            }
        }

        /// One `SendInput` mouse record. Returns the accepted-event count
        /// (0 when the API refused it). Sole synthetic-input write vector in
        /// this example; every caller re-verifies guards immediately before
        /// each batch and releases the button in a RAII guard.
        pub fn send_mouse_input(dx: i32, dy: i32, mouse_data: u32, flags: u32) -> u32 {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                INPUT, INPUT_MOUSE, MOUSEINPUT, SendInput,
            };
            unsafe {
                let mut input: INPUT = std::mem::zeroed();
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi = MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: mouse_data,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                };
                let size = std::mem::size_of::<INPUT>() as i32;
                SendInput(1, &input, size)
            }
        }

        /// Show-command constants for the hide/reveal leg (own fixture only).
        pub const SW_HIDE: i32 = 0;
        pub const SW_SHOWNA: i32 = 8;

        /// Public `DwmSetWindowAttribute(DWMWA_CLOAK)` write probe. Returns
        /// the raw HRESULT (0 == S_OK). The caller records the HRESULT plus
        /// the native `cloaked()` readback before/after and restores 0.
        pub fn dwm_set_cloak(hwnd: u64, cloak: bool) -> i32 {
            use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
            const DWMWA_CLOAK: u32 = 14;
            let value: i32 = if cloak { 1 } else { 0 };
            unsafe {
                DwmSetWindowAttribute(
                    hwnd as isize as _,
                    DWMWA_CLOAK,
                    (&value as *const i32).cast(),
                    std::mem::size_of::<i32>() as u32,
                )
            }
        }

        /// Bounded visibility poll: true only when `is_visible` reports the
        /// wanted state inside the budget. Read-only.
        pub fn wait_visible(hwnd: u64, want: bool, timeout_ms: u64) -> bool {
            let start = std::time::Instant::now();
            loop {
                if tiler_windows::ffi::window::is_visible(hwnd) == want {
                    return true;
                }
                if start.elapsed().as_millis() >= u128::from(timeout_ms) {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Live roles (Windows only, never exercised by unit tests).
    // -----------------------------------------------------------------------
    use tiler_windows::ffi::dpi::{set_process_pm_v2, thread_pm_v2_active};
    use tiler_windows::ffi::enumeration::enumerate_monitors;
    use tiler_windows::ffi::window::{
        get_prop, is_window, set_prop, set_topmost, window_pid, window_rect,
    };
    use tiler_windows::ffi::{extended_frame_bounds, move_file_no_replace};
    use tiler_windows::native::HeldProcess;

    fn usage() -> i32 {
        eprintln!("usage:");
        eprintln!(
            "  win-probe setup --fixture-exe EXE --run-dir DIR --callers a,b [--suffix TEXT]"
        );
        eprintln!(
            "  win-probe controller --fixture-exe EXE --run-dir DIR --callers a,b [--suffix TEXT] [--stay-ms N]"
        );
        eprintln!(
            "  win-probe writer --run-dir DIR --caller ID --effect-top true|false --ready FILE [--stay-ms N] [--stop-file F] [--record NAME] [--no-mutate]"
        );
        eprintln!("  win-probe recoverer --run-dir DIR --record NAME");
        eprintln!("  win-probe measure123 --leg LEG --run-dir DIR [--fixture-exe EXE]");
        eprintln!(
            "    LEG: r14|topmost-preowned|topmost-loss|topmost-extclear|topmost-exttrue|storage-churn|storage-stale|dxgi|k5|k5-resize|hide-loss|identity"
        );
        2
    }

    pub fn run(argv: Vec<String>) -> i32 {
        if argv.is_empty() {
            return usage();
        }
        match argv[0].as_str() {
            "setup" => setup(&argv[1..]),
            "controller" => controller(&argv[1..]),
            "writer" => writer(&argv[1..]),
            "recoverer" => recoverer(&argv[1..]),
            "measure123" => measure123(&argv[1..]),
            "measure4" => measure_k5(&argv[1..]),
            _ => usage(),
        }
    }

    fn flag(args: &[String], name: &str) -> Option<String> {
        args.windows(2).find_map(|w| {
            if w[0] == name {
                Some(w[1].clone())
            } else {
                None
            }
        })
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct RosterEntry {
        caller: String,
        hwnd: u64,
        title: String,
        tag: String,
        marker: u64,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct Handoff {
        socket: String,
        nonce: String,
        fixture_pid: u32,
        fixture_exe: String,
        fixture_creation: String,
        fixture_user: String,
        fixture_session: u32,
        fixture_integrity: u32,
        run_tag: String,
        callers: Vec<String>,
        roster: Vec<RosterEntry>,
    }

    fn read_handoff(run_dir: &Path) -> Result<Handoff, String> {
        let raw = std::fs::read_to_string(run_dir.join("handoff.json"))
            .map_err(|e| format!("read handoff: {e}"))?;
        serde_json::from_str(&raw).map_err(|e| format!("parse handoff: {e}"))
    }

    fn send_one(exe: &str, socket: &str, nonce: &str, payload: &str) -> Result<String, String> {
        send_all(exe, socket, nonce, &[payload.to_string()])
    }

    /// One client connection carrying several ordered payload lines (one
    /// reply line per payload). Same hung-client bound as `send_one`.
    fn send_all(
        exe: &str,
        socket: &str,
        nonce: &str,
        payloads: &[String],
    ) -> Result<String, String> {
        // Read-only transport: get/list only. Mutation goes through
        // send_mutation(s) which re-verify the fixture immediately before send.
        // Bounded: spawn + wait_timeout + kill-hung-client, 30s cap.
        use std::io::Read;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("send")
            .arg("--socket")
            .arg(socket)
            .arg("--nonce")
            .arg(nonce);
        for payload in payloads {
            cmd.arg(payload);
        }
        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn send: {e}"))?;
        let timeout = std::time::Duration::from_secs(30);
        // wait_timeout is not stable on Child; emulate with a bounded poll.
        // The hung-client guard below preserves the fixture (never kills it):
        // only this short-lived send client is reaped.
        let start = std::time::Instant::now();
        loop {
            match child.try_wait().map_err(|e| format!("wait send: {e}"))? {
                Some(status) => {
                    let mut out = String::new();
                    if let Some(mut pipe) = child.stdout.take() {
                        let _ = pipe.read_to_string(&mut out);
                    }
                    if !status.success() {
                        return Err("send failed".to_string());
                    }
                    return Ok(out);
                }
                None => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("send timed out; hung client reaped".to_string());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
        }
    }

    fn send_mutation(
        handoff: &Handoff,
        exe: &str,
        socket: &str,
        nonce: &str,
        payload: &str,
    ) -> Result<String, String> {
        // Last chokepoint immediately before the call: no file I/O between
        // this guard and the send below.
        verify_fixture(handoff)?;
        verify_fixture_medium(handoff)?;
        let out = send_one(exe, socket, nonce, payload)?;
        Ok(out)
    }

    /// Multi-payload mutation variant: one guarded connection for an ordered
    /// op sequence (e.g. spawn+get, close+list).
    fn send_mutations(
        handoff: &Handoff,
        exe: &str,
        socket: &str,
        nonce: &str,
        payloads: &[String],
    ) -> Result<String, String> {
        verify_fixture(handoff)?;
        verify_fixture_medium(handoff)?;
        send_all(exe, socket, nonce, payloads)
    }

    fn fixture_snapshot(
        exe: &str,
        socket: &str,
        nonce: &str,
        caller: &str,
    ) -> Result<serde_json::Value, String> {
        let payload = serde_json::json!({"v":1,"request_id":"probe-get","nonce":nonce,"op":"get","ids":[caller]})
            .to_string();
        let out = send_one(exe, socket, nonce, &payload)?;
        let reply: serde_json::Value =
            serde_json::from_str(&out).map_err(|e| format!("parse get reply: {e}"))?;
        reply
            .pointer("/result/windows/0/snapshot")
            .cloned()
            .ok_or_else(|| "get reply carries no snapshot".to_string())
    }

    /// Resolve one caller id to its HWND: parse the fixture `native_handle`
    /// debug token, then verify PID + title text independently. Any mismatch
    /// fails closed (no HWND returned).
    fn resolve_hwnd(snapshot: &serde_json::Value, pid: u32, full_title: &str) -> Option<u64> {
        let handle = snapshot.get("native_handle")?.as_str()?;
        let hwnd_pos = handle.find("hwnd")?;
        let digits: String = handle[hwnd_pos..]
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let hwnd: u64 = digits.parse().ok()?;
        if hwnd == 0 || !is_window(hwnd) || window_pid(hwnd) != pid {
            return None;
        }
        if ffi_ext::window_text(hwnd).as_deref() != Some(full_title) {
            return None;
        }
        Some(hwnd)
    }

    #[derive(Debug, serde::Serialize)]
    struct Baseline {
        caller: String,
        hwnd: u64,
        native_handle: String,
        title: String,
        outer_px: Option<(i32, i32, i32, i32)>,
        dwm_px: Option<(i32, i32, i32, i32)>,
        exstyle: Option<u32>,
        foreground_hwnd: u64,
        monitors: usize,
    }

    fn capture_baseline(snapshot: &serde_json::Value, caller: &str, hwnd: u64) -> Baseline {
        let outer_px = window_rect(hwnd).map(|r| (r.left, r.top, r.right, r.bottom));
        let dwm_px = extended_frame_bounds(hwnd).map(|r| (r.left, r.top, r.right, r.bottom));
        Baseline {
            caller: caller.to_string(),
            hwnd,
            native_handle: snapshot
                .get("native_handle")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            title: snapshot
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            outer_px,
            dwm_px,
            exstyle: ffi_ext::window_exstyle_checked(hwnd),
            foreground_hwnd: tiler_windows::ffi::window::foreground_window(),
            monitors: enumerate_monitors().map(|m| m.len()).unwrap_or(0),
        }
    }

    fn fixture_identity(exe_path: &str, pid: u32) -> Result<(String, String, u32, u32), String> {
        let held = HeldProcess::open(pid).map_err(|e| format!("open fixture: {e}"))?;
        let id = held
            .identity()
            .map_err(|e| format!("fixture identity: {e}"))?;
        let integrity = held
            .integrity()
            .map_err(|e| format!("fixture integrity: {e}"))?;
        if id.exe_path != exe_path {
            return Err("fixture exe mismatch".to_string());
        }
        Ok((id.process_creation, id.user_sid, id.session_id, integrity))
    }

    /// PMv2 set + verify BEFORE any geometry read.
    fn require_pm_v2() -> Result<(), String> {
        set_process_pm_v2();
        if thread_pm_v2_active() {
            Ok(())
        } else {
            Err("PMv2 awareness not active; refusing geometry reads".to_string())
        }
    }

    fn setup(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("setup: --fixture-exe is required");
            return 2;
        };
        let Some(run_dir) = flag(args, "--run-dir") else {
            eprintln!("setup: --run-dir is required");
            return 2;
        };
        let Some(callers_raw) = flag(args, "--callers") else {
            eprintln!("setup: --callers is required");
            return 2;
        };
        let suffix = flag(args, "--suffix");
        let run_dir = PathBuf::from(run_dir);
        if let Err(e) = std::fs::create_dir_all(&run_dir) {
            eprintln!("setup: create run dir: {e}");
            return 1;
        }
        // Controller creates the exact fixture child; bind PID/creation/user/
        // session/medium-integrity via HeldProcess plus the announced PID.
        let serve_log = run_dir.join("fixture-stderr.log");
        let serve_log_file = match std::fs::File::create(&serve_log) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("setup: log file: {e}");
                return 1;
            }
        };
        let mut child = match std::process::Command::new(&fixture_exe)
            .arg("serve")
            .arg("--dir")
            .arg(&run_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(serve_log_file)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("setup: spawn fixture: {e}");
                return 1;
            }
        };
        let stdout = child.stdout.take();
        let mut announcement = String::new();
        if let Some(mut pipe) = stdout {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            let start = std::time::Instant::now();
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        announcement.push_str(&String::from_utf8_lossy(&buf[..n]));
                        if announcement.contains('\n') {
                            break;
                        }
                    }
                    Err(_) => break,
                }
                if start.elapsed() > std::time::Duration::from_secs(30) {
                    break;
                }
            }
        }
        let first_line = announcement.lines().next().unwrap_or("").trim().to_string();
        let ann: serde_json::Value = match serde_json::from_str(&first_line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("setup: bad announcement: {e}");
                std::mem::forget(child);
                return 1;
            }
        };
        let socket = ann
            .get("socket")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let nonce = ann
            .get("nonce")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let announced_pid = ann.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if socket.is_empty() || nonce.is_empty() || announced_pid == 0 {
            eprintln!("setup: incomplete announcement");
            std::mem::forget(child);
            return 1;
        }
        if child.id() != announced_pid {
            eprintln!("setup: announced PID does not match spawned child");
            std::mem::forget(child);
            return 1;
        }
        let (creation, user, session, integrity) =
            match fixture_identity(&fixture_exe, announced_pid) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("setup: {e}");
                    std::mem::forget(child);
                    return 1;
                }
            };
        // Spawn caller windows (acquired baseline; R14 pair recorded below).
        let callers: Vec<String> = callers_raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if callers.is_empty() {
            eprintln!("setup: no callers parsed");
            std::mem::forget(child);
            return 1;
        }
        // Measurement-only spawn extensions: `--topmost-spawn a,b` starts
        // those callers with window_level always_on_top (a measured
        // preowned arrangement); `--spawn-modes caller=mode,...` sets an
        // initial mode (windowed/maximized/minimized/borderless_fullscreen;
        // exclusive is refused: the fixture would pick the first video mode,
        // never the current one).
        let topmost_spawn: Vec<String> = flag(args, "--topmost-spawn")
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let spawn_modes = match parse_spawn_modes(flag(args, "--spawn-modes").as_deref()) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("setup: {e}");
                std::mem::forget(child);
                return 2;
            }
        };
        let specs: Vec<serde_json::Value> = callers
            .iter()
            .map(|id| {
                let mut spec = serde_json::json!({"id": id});
                if topmost_spawn.iter().any(|t| t == id) {
                    spec["window_level"] = serde_json::json!("always_on_top");
                }
                if let Some(mode) = spawn_modes
                    .iter()
                    .find(|(caller, _)| caller == id)
                    .map(|(_, mode)| mode)
                {
                    spec["mode"] = serde_json::json!(mode);
                }
                spec
            })
            .collect();
        let spawn = serde_json::json!({"v":1,"request_id":"probe-spawn","nonce":nonce,"op":"spawn","windows":specs})
            .to_string();
        // Protocol spawn is a mutation: verify the original trusted fixture
        // identity immediately before the call (no HWND yet to guard).
        let handoff_stub = Handoff {
            socket: socket.clone(),
            nonce: nonce.clone(),
            fixture_pid: announced_pid,
            fixture_exe: fixture_exe.clone(),
            fixture_creation: creation.clone(),
            fixture_user: user.clone(),
            fixture_session: session,
            fixture_integrity: integrity,
            run_tag: String::new(),
            callers: Vec::new(),
            roster: Vec::new(),
        };
        if let Err(e) = send_mutation(&handoff_stub, &fixture_exe, &socket, &nonce, &spawn) {
            eprintln!(
                "setup: spawn: {e}; preserving fixture pid {announced_pid} for manual shutdown"
            );
            std::mem::forget(child);
            return 1;
        }
        if let Err(e) = require_pm_v2() {
            eprintln!("setup: {e}");
            std::mem::forget(child);
            return 1;
        }
        // Join + baseline + tag bootstrap per caller. The run tag is
        // rejoined from the first snapshot title (server-controlled shape).
        let mut baselines = Vec::new();
        let mut roster = Vec::new();
        let mut run_tag = String::new();
        for caller in &callers {
            let snapshot = match fixture_snapshot(&fixture_exe, &socket, nonce.as_str(), caller) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("setup: snapshot {caller}: {e}");
                    std::mem::forget(child);
                    return 1;
                }
            };
            let snap_pid = snapshot.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if snap_pid != announced_pid {
                eprintln!("setup: snapshot PID mismatch for {caller}");
                std::mem::forget(child);
                return 1;
            }
            let snap_title = snapshot
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Rejoin run-tag from native-shaped title; never copy reply sides.
            let (parsed_caller, tag) = match parse_title_identity(&snap_title) {
                Some((c, t)) if c == *caller => (c, t),
                _ => {
                    eprintln!("setup: sentinel title shape mismatch for {caller}");
                    std::mem::forget(child);
                    return 1;
                }
            };
            let _ = parsed_caller;
            if run_tag.is_empty() {
                run_tag = tag;
            } else if run_tag != tag {
                eprintln!("setup: run-tag mismatch across callers");
                std::mem::forget(child);
                return 1;
            }
            let expect = expected_title(caller, &run_tag, suffix.as_deref());
            if snap_title != expect {
                eprintln!("setup: sentinel title mismatch for {caller}");
                std::mem::forget(child);
                return 1;
            }
            let Some(hwnd) = resolve_hwnd(&snapshot, announced_pid, &snap_title) else {
                eprintln!(
                    "setup: HWND join failed for {caller}; preserving fixture pid {announced_pid}"
                );
                std::mem::forget(child);
                return 1;
            };
            if topmost_spawn.iter().any(|t| t == caller) {
                // The preowned arrangement is app-own (spawn-level hint via
                // the owned server). Wait for the native TOPMOST bit so the
                // baseline below genuinely measures preowned state.
                if !wait_native_topmost_bit(hwnd, true, 5_000) {
                    eprintln!(
                        "setup: topmost-spawn {caller} never read topmost; preserving fixture pid {announced_pid}"
                    );
                    std::mem::forget(child);
                    return 1;
                }
            }
            baselines.push(capture_baseline(&snapshot, caller, hwnd));
            roster.push((caller.clone(), hwnd, snap_title));
        }
        // Tag + marker bootstrap per caller through the roster-bound backend
        // (refresh-inside). Each caller gets a durable before-record published
        // before any mutation. Preowned (originally topmost) windows are
        // counted as setup with zero policy writes and still bootstrapped.
        let mut counts = EffectCounts::default();
        let mut roster_out = Vec::new();
        // Handoff stub for roster binding during bootstrap (run_tag known).
        let handoff_stub2 = Handoff {
            socket: socket.clone(),
            nonce: nonce.clone(),
            fixture_pid: announced_pid,
            fixture_exe: fixture_exe.clone(),
            fixture_creation: creation.clone(),
            fixture_user: user.clone(),
            fixture_session: session,
            fixture_integrity: integrity,
            run_tag: run_tag.clone(),
            callers: callers.clone(),
            roster: Vec::new(),
        };
        for (caller, hwnd, title) in &roster {
            let tag = generate_probe_tag();
            if !valid_probe_tag(&tag) {
                eprintln!(
                    "setup: no entropy for tag bootstrap; preserving fixture pid {announced_pid}"
                );
                std::mem::forget(child);
                return 1;
            }
            // Deterministic nonzero marker preimage per caller (random low
            // bits mixed with a nonzero constant; never a production prop).
            let marker: u64 = (fnv_u64(&format!("{tag}:{caller}")) | 0x0100_0000_0000_0001).max(1);
            let entry_stub = RosterEntry {
                caller: caller.clone(),
                hwnd: *hwnd,
                title: title.clone(),
                tag: tag.clone(),
                marker,
            };
            // Baseline preimage for the setup record: native topmost now.
            let preimage_native = ffi_ext::window_exstyle_checked(*hwnd).map(topmost_of_exstyle);
            let preowned_native = preimage_native != Some(false);
            if preowned_native {
                counts.preowned_setups += 1;
            }
            let setup_record = match encode_record(&RecordSpec {
                schema: RECORD_SCHEMA,
                fixture_exe: &fixture_exe,
                fixture_pid: announced_pid,
                fixture_creation: &creation,
                fixture_user: &user,
                fixture_session: session,
                fixture_integrity: integrity,
                caller,
                full_title: title,
                tag: &tag,
                hwnd: *hwnd,
                effect_topmost: false,
                preimage_topmost: preimage_native,
                opaque: &encode_marker_opaque(marker),
            }) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("setup: encode setup record: {e}");
                    std::mem::forget(child);
                    return 1;
                }
            };
            let setup_name = format!("setup-{caller}.json");
            if let Err(e) = publish_record_atomic(&run_dir, &setup_name, &setup_record) {
                eprintln!(
                    "setup: publish setup record failed ({e}); preserving fixture pid {announced_pid}"
                );
                std::mem::forget(child);
                return 1;
            }
            // Bootstrap facts: absent-tag exception with full fresh identity.
            let mut boot_backend = LiveRoster {
                handoff: &handoff_stub2,
                entry: &entry_stub,
                hwnd: *hwnd,
            };
            // Build the expected side via one fresh rejoin, then flag the
            // single establishment exception for the chokepoint below (which
            // re-observes inside before writing).
            let mut expected = match rejoin_live(&handoff_stub2, &entry_stub, *hwnd) {
                Ok((facts, _)) => facts,
                Err(e) => {
                    eprintln!("setup: rejoin {caller}: {e}");
                    std::mem::forget(child);
                    return 1;
                }
            };
            expected.bootstrap_tag_establishment = true;
            if !bootstrap_tag(&mut boot_backend, &expected, &mut counts) {
                eprintln!(
                    "setup: tag bootstrap refused for {caller}; preserving fixture pid {announced_pid}"
                );
                std::mem::forget(child);
                return 1;
            }
            // Marker bootstrap through the same roster backend (guarded).
            let mut marker_backend = LiveRoster {
                handoff: &handoff_stub2,
                entry: &entry_stub,
                hwnd: *hwnd,
            };
            let mut marker_expected = match rejoin_live(&handoff_stub2, &entry_stub, *hwnd) {
                Ok((facts, _)) => facts,
                Err(e) => {
                    eprintln!("setup: marker rejoin {caller}: {e}");
                    std::mem::forget(child);
                    return 1;
                }
            };
            marker_expected.bootstrap_tag_establishment = false;
            match guarded_marker_write(&mut marker_backend, &marker_expected, marker, &mut counts) {
                WriteOutcome::AppliedVerified => {}
                WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                    eprintln!("setup: marker bootstrap refused for {caller}; preserving");
                    std::mem::forget(child);
                    return 1;
                }
            }
            roster_out.push(RosterEntry {
                caller: caller.clone(),
                hwnd: *hwnd,
                title: title.clone(),
                tag,
                marker,
            });
        }
        let handoff = Handoff {
            socket,
            nonce,
            fixture_pid: announced_pid,
            fixture_exe: fixture_exe.clone(),
            fixture_creation: creation,
            fixture_user: user,
            fixture_session: session,
            fixture_integrity: integrity,
            run_tag,
            callers,
            roster: roster_out,
        };
        let _ = std::fs::write(
            run_dir.join("baselines.json"),
            serde_json::to_string_pretty(&baselines).unwrap_or_default(),
        );
        match std::fs::write(
            run_dir.join("handoff.json"),
            serde_json::to_string(&handoff).unwrap_or_default(),
        ) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("setup: write handoff: {e}");
                std::mem::forget(child);
                1
            }
        }
    }

    /// Roster-bound live backend: observe() rejoins via the single
    /// fresh-read helper, so every chokepoint refreshes INSIDE immediately
    /// before the call with no file I/O between the final check and effect.
    struct NativeBackend;
    struct LiveRoster<'a> {
        handoff: &'a Handoff,
        entry: &'a RosterEntry,
        hwnd: u64,
    }

    impl ProbeBackend for LiveRoster<'_> {
        fn observe(&mut self, hwnd: u64) -> Option<GuardFacts> {
            if hwnd != self.hwnd {
                return None;
            }
            rejoin_live(self.handoff, self.entry, hwnd)
                .ok()
                .map(|(facts, _)| facts)
        }
        fn read_tag(&self, hwnd: u64) -> LiveTag {
            NativeBackend::read_tag_inner(hwnd)
        }
        fn read_exstyle(&self, hwnd: u64) -> Option<u32> {
            ffi_ext::window_exstyle_checked(hwnd)
        }
        fn read_topmost(&self, hwnd: u64) -> Option<bool> {
            ffi_ext::window_exstyle_checked(hwnd).map(topmost_of_exstyle)
        }
        fn read_marker(&self, hwnd: u64) -> Option<u64> {
            NativeBackend::read_marker_inner(hwnd)
        }
        fn write_topmost(&mut self, hwnd: u64, top: bool) -> bool {
            // Guard read is already inside observe(); this call performs the
            // effect with no intervening file I/O.
            set_topmost(hwnd, top)
        }
        fn write_tag(&mut self, hwnd: u64, value: u64) -> bool {
            set_prop(hwnd, PROBE_PROP, value as usize)
        }
        fn write_marker(&mut self, hwnd: u64, value: u64) -> bool {
            set_prop(hwnd, PROBE_MARKER_PROP, value as usize)
        }
    }
    /// Fresh-read helper shared by observe(): rejoins held process identity,
    /// native window text, independently parsed caller/run-tag, and protocol
    /// snapshot. No side copies expected values into live sides.
    fn rejoin_live(
        handoff: &Handoff,
        entry: &RosterEntry,
        hwnd: u64,
    ) -> Result<(GuardFacts, FreshObs), String> {
        if !is_window(hwnd) {
            return Err("window absent".to_string());
        }
        let pid = window_pid(hwnd);
        if pid != handoff.fixture_pid {
            return Err("HWND PID does not match fixture".to_string());
        }
        let held = HeldProcess::open(pid).map_err(|_| "window process absent".to_string())?;
        let id = held
            .identity()
            .map_err(|e| format!("window identity: {e}"))?;
        let integrity = held
            .integrity()
            .map_err(|e| format!("window integrity: {e}"))?;
        if id.exe_path.is_empty() || id.process_creation.is_empty() || id.user_sid.is_empty() {
            return Err("window identity incomplete".to_string());
        }
        let native_title = ffi_ext::window_text(hwnd).ok_or("native title unreadable")?;
        let (live_caller, live_run) =
            parse_title_identity(&native_title).ok_or("native title shape mismatch")?;
        if live_run != handoff.run_tag {
            return Err("run-tag moved".to_string());
        }
        let session = tiler_windows::ffi::session_of_pid(pid).unwrap_or(u32::MAX);
        let live_tag = NativeBackend::read_tag_inner(hwnd);
        let exstyle_readable = ffi_ext::window_exstyle_checked(hwnd).is_some();
        let facts = GuardFacts {
            stored_key: WindowKey {
                hwnd: entry.hwnd,
                pid: handoff.fixture_pid,
                creation: handoff.fixture_creation.clone(),
            },
            live_hwnd: hwnd,
            live_pid: pid,
            live_creation: id.process_creation.clone(),
            expected_exe: handoff.fixture_exe.clone(),
            live_exe: id.exe_path.clone(),
            expected_user: handoff.fixture_user.clone(),
            live_user: id.user_sid.clone(),
            expected_session: handoff.fixture_session,
            live_session: session,
            expected_integrity: handoff.fixture_integrity,
            live_integrity: integrity,
            expected_caller: entry.caller.clone(),
            live_caller: live_caller.clone(),
            expected_title: entry.title.clone(),
            live_title: native_title.clone(),
            stored_tag: entry.tag.clone(),
            live_tag: live_tag.clone(),
            exstyle_readable,
            bootstrap_tag_establishment: false,
        };
        let obs = FreshObs {
            hwnd,
            pid,
            creation: id.process_creation,
            exe: id.exe_path,
            user: id.user_sid,
            session,
            integrity,
            caller: live_caller,
            title: native_title,
            live_tag,
        };
        Ok((facts, obs))
    }

    impl NativeBackend {
        fn read_tag_inner(hwnd: u64) -> LiveTag {
            if !is_window(hwnd) {
                return LiveTag::Unreadable;
            }
            match get_prop(hwnd, PROBE_PROP) {
                Some(v) if v != 0 => LiveTag::Value(v as u64),
                Some(_) => LiveTag::Absent,
                None => LiveTag::Absent,
            }
        }
        fn read_marker_inner(hwnd: u64) -> Option<u64> {
            if !is_window(hwnd) {
                return None;
            }
            match get_prop(hwnd, PROBE_MARKER_PROP) {
                Some(v) if v != 0 => Some(v as u64),
                _ => None,
            }
        }
    }

    impl ProbeBackend for NativeBackend {
        fn observe(&mut self, hwnd: u64) -> Option<GuardFacts> {
            // Live observe needs roster context; the free function rejoin_live
            // is the canonical path. This trait entry returns None so a bare
            // NativeBackend never authorizes without roster binding.
            let _ = hwnd;
            None
        }
        fn read_tag(&self, hwnd: u64) -> LiveTag {
            Self::read_tag_inner(hwnd)
        }
        fn read_exstyle(&self, hwnd: u64) -> Option<u32> {
            ffi_ext::window_exstyle_checked(hwnd)
        }
        fn read_topmost(&self, hwnd: u64) -> Option<bool> {
            ffi_ext::window_exstyle_checked(hwnd).map(topmost_of_exstyle)
        }
        fn read_marker(&self, hwnd: u64) -> Option<u64> {
            Self::read_marker_inner(hwnd)
        }
        fn write_topmost(&mut self, hwnd: u64, top: bool) -> bool {
            set_topmost(hwnd, top)
        }
        fn write_tag(&mut self, hwnd: u64, value: u64) -> bool {
            set_prop(hwnd, PROBE_PROP, value as usize)
        }
        fn write_marker(&mut self, hwnd: u64, value: u64) -> bool {
            set_prop(hwnd, PROBE_MARKER_PROP, value as usize)
        }
    }

    fn controller(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("controller: --fixture-exe is required");
            return 2;
        };
        let Some(run_dir_raw) = flag(args, "--run-dir") else {
            eprintln!("controller: --run-dir is required");
            return 2;
        };
        let Some(callers) = flag(args, "--callers") else {
            eprintln!("controller: --callers is required");
            return 2;
        };
        let stay_ms = flag(args, "--stay-ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(15_000u64);
        let run_dir = PathBuf::from(&run_dir_raw);

        // Phase 0: fixture + callers + baseline + tag bootstrap.
        let mut setup_argv = vec![
            "--fixture-exe".to_string(),
            fixture_exe.clone(),
            "--run-dir".to_string(),
            run_dir_raw.clone(),
            "--callers".to_string(),
            callers.clone(),
        ];
        if let Some(suffix) = flag(args, "--suffix") {
            setup_argv.push("--suffix".to_string());
            setup_argv.push(suffix);
        }
        if setup(&setup_argv) != 0 {
            return 1;
        }
        let handoff = match read_handoff(&run_dir) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("controller: {e}");
                return 1;
            }
        };
        let probe_exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("controller: current exe: {e}");
                return 1;
            }
        };
        let _probe_exe_raw = probe_exe.to_string_lossy().to_string();

        // Phase 1: TWO nonmutating created probe children: first proves
        // graceful exit 0 via stop-file, second proves real forced kill with
        // abnormal exit. Each logs original ProcSnapshot + command/result.
        // Never kills the fixture; readiness-failure never calls child.kill.
        let preflight_ready = run_dir.join("preflight.ready");
        let preflight_stop = run_dir.join("preflight.stop");
        let mut graceful = match spawn_probe(
            &probe_exe,
            &[
                "writer",
                "--run-dir",
                &run_dir_raw,
                "--caller",
                &handoff.callers[0],
                "--effect-top",
                "true",
                "--ready",
                &preflight_ready.to_string_lossy(),
                "--stay-ms",
                "20000",
                "--stop-file",
                &preflight_stop.to_string_lossy(),
                "--no-mutate",
            ],
            &run_dir,
            "preflight-graceful.log",
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("controller: graceful probe spawn: {e}; preserving, no teardown sends");
                return 1;
            }
        };
        let _graceful_snapshot = match proc_snapshot_of(graceful.id()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("controller: graceful probe identity: {e}; preserving");
                std::mem::forget(graceful);
                return 1;
            }
        };
        if !wait_for_file(&preflight_ready, 15_000) {
            eprintln!("controller: graceful probe never ready; preserving (no kill, no teardown)");
            std::mem::forget(graceful);
            return 1;
        }
        let _ = std::fs::write(&preflight_stop, b"stop");
        if !wait_child_exit(&mut graceful, 10_000) {
            eprintln!("controller: graceful probe did not exit; preserving (no kill)");
            std::mem::forget(graceful);
            return 1;
        }
        // Second child: same --no-mutate, then EXACT forced kill (abnormal).
        let forced_ready = run_dir.join("preflight-forced.ready");
        let forced_stop = run_dir.join("preflight-forced.stop");
        let mut forced = match spawn_probe(
            &probe_exe,
            &[
                "writer",
                "--run-dir",
                &run_dir_raw,
                "--caller",
                &handoff.callers[0],
                "--effect-top",
                "true",
                "--ready",
                &forced_ready.to_string_lossy(),
                "--stay-ms",
                "60000",
                "--stop-file",
                &forced_stop.to_string_lossy(),
                "--no-mutate",
            ],
            &run_dir,
            "preflight-forced.log",
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("controller: forced probe spawn: {e}; preserving");
                return 1;
            }
        };
        let forced_snapshot = match proc_snapshot_of(forced.id()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("controller: forced probe identity: {e}; preserving");
                std::mem::forget(forced);
                return 1;
            }
        };
        if !wait_for_file(&forced_ready, 15_000) {
            eprintln!("controller: forced probe never ready; preserving (no kill)");
            std::mem::forget(forced);
            return 1;
        }
        if terminate_exact(&forced_snapshot).is_err() {
            eprintln!("controller: forced probe kill refused; preserving");
            std::mem::forget(forced);
            return 1;
        }
        if !wait_child_exit(&mut forced, 10_000) {
            eprintln!("controller: forced probe survived exact kill; preserving");
            std::mem::forget(forced);
            return 1;
        }

        // Phase 2: independent-recovery preflight on the DISPOSABLE (last)
        // caller via fresh writer + fresh recoverer children.
        let disposable = handoff.callers[handoff.callers.len() - 1].clone();
        if preflight_restore(&probe_exe, &run_dir, &run_dir_raw, &handoff, &disposable) != 0 {
            eprintln!("controller: preflight restore failed; preserving, no teardown sends");
            return 1;
        }

        // Phase 3: forced-writer-loss on the first caller.
        let subject = handoff.callers[0].clone();
        let writer_ready = run_dir.join("writer.ready");
        let writer_stop = run_dir.join("writer.stop");
        let record_name = format!("write-{subject}.json");
        // Basename only (record paths never carry directories).
        if record_name.contains(['/', '\\']) {
            eprintln!("controller: bad record name; preserving");
            return 1;
        }
        let mut writer = match spawn_probe(
            &probe_exe,
            &[
                "writer",
                "--run-dir",
                &run_dir_raw,
                "--caller",
                &subject,
                "--effect-top",
                "true",
                "--ready",
                &writer_ready.to_string_lossy(),
                "--stay-ms",
                "60000",
                "--stop-file",
                &writer_stop.to_string_lossy(),
                "--record",
                &record_name,
            ],
            &run_dir,
            "writer.log",
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("controller: writer spawn: {e}; preserving");
                return 1;
            }
        };
        let writer_snapshot = match proc_snapshot_of(writer.id()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("controller: writer identity: {e}; preserving");
                std::mem::forget(writer);
                return 1;
            }
        };
        if !wait_for_file(&writer_ready, 20_000) {
            eprintln!("controller: writer never became ready; preserving (no kill)");
            std::mem::forget(writer);
            return 1;
        }
        // Forced loss: terminate the EXACT writer only.
        if terminate_exact(&writer_snapshot).is_err() {
            eprintln!("controller: writer kill refused; preserving");
            std::mem::forget(writer);
            return 1;
        }
        if !wait_child_exit(&mut writer, 10_000) {
            eprintln!("controller: writer survived exact kill; preserving state");
            std::mem::forget(writer);
            return 1;
        }

        // Phase 4: fresh recoverer CHILD (never in-process).
        let recover_status = std::process::Command::new(&probe_exe)
            .arg("recoverer")
            .arg("--run-dir")
            .arg(&run_dir_raw)
            .arg("--record")
            .arg(&record_name)
            .status();
        match recover_status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("controller: fresh recoverer exited {s}; preserving");
                return 1;
            }
            Err(e) => {
                eprintln!("controller: recoverer spawn: {e}; preserving");
                return 1;
            }
        }
        let _ = stay_ms;

        // Phase 5: ordinary successful teardown (two-pass, guarded close_one).
        teardown_best_effort(&fixture_exe, &run_dir, &handoff)
    }

    /// Fresh-recoverer preflight on the DISPOSABLE caller: a writer CHILD makes
    /// one real harmless topmost mutation (recorded pre-write), the writer is
    /// force-killed, then a FRESH recoverer CHILD restores the preimage while
    /// the fixture/writer lineage survives. The controller never recovers
    /// in-process. Returns 0 only when the fresh child verifies live==false.
    fn preflight_restore(
        probe_exe: &Path,
        run_dir: &Path,
        run_dir_raw: &str,
        handoff: &Handoff,
        caller: &str,
    ) -> i32 {
        if verify_fixture(handoff).is_err() || verify_fixture_medium(handoff).is_err() {
            eprintln!("preflight: fixture identity not verified; preserving");
            return 1;
        }
        // Writer child: one harmless set-true on a disposable originally-false
        // window, then forced loss.
        let ready = run_dir.join("preflight-writer.ready");
        let record_name = "preflight.json";
        let mut writer = match spawn_probe(
            probe_exe,
            &[
                "writer",
                "--run-dir",
                run_dir_raw,
                "--caller",
                caller,
                "--effect-top",
                "true",
                "--ready",
                &ready.to_string_lossy(),
                "--stay-ms",
                "60000",
                "--record",
                record_name,
            ],
            run_dir,
            "preflight-writer.log",
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("preflight: writer spawn: {e}; preserving");
                return 1;
            }
        };
        let writer_snapshot = match proc_snapshot_of(writer.id()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("preflight: writer identity: {e}; preserving");
                std::mem::forget(writer);
                return 1;
            }
        };
        if !wait_for_file(&ready, 20_000) {
            eprintln!("preflight: writer never ready; preserving (no kill)");
            std::mem::forget(writer);
            return 1;
        }
        if terminate_exact(&writer_snapshot).is_err() {
            eprintln!("preflight: writer kill refused; preserving");
            std::mem::forget(writer);
            return 1;
        }
        if !wait_child_exit(&mut writer, 10_000) {
            eprintln!("preflight: writer survived exact kill; preserving");
            std::mem::forget(writer);
            return 1;
        }
        // Fresh recoverer child restores the preimage (false).
        let status = std::process::Command::new(probe_exe)
            .arg("recoverer")
            .arg("--run-dir")
            .arg(run_dir_raw)
            .arg("--record")
            .arg(record_name)
            .status();
        match status {
            Ok(s) if s.success() => 0,
            Ok(s) => {
                eprintln!("preflight: fresh recoverer exited {s}; preserving");
                1
            }
            Err(e) => {
                eprintln!("preflight: recoverer spawn: {e}; preserving");
                1
            }
        }
    }

    fn writer(args: &[String]) -> i32 {
        let Some(run_dir_raw) = flag(args, "--run-dir") else {
            eprintln!("writer: --run-dir is required");
            return 2;
        };
        let Some(caller) = flag(args, "--caller") else {
            eprintln!("writer: --caller is required");
            return 2;
        };
        let effect_top = flag(args, "--effect-top").as_deref() == Some("true");
        let Some(ready_raw) = flag(args, "--ready") else {
            eprintln!("writer: --ready is required");
            return 2;
        };
        let stay_ms = flag(args, "--stay-ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(15_000u64);
        let stop_file = flag(args, "--stop-file").map(PathBuf::from);
        let record_name = flag(args, "--record").unwrap_or(format!("write-{caller}.json"));
        let no_mutate = args.iter().any(|a| a == "--no-mutate");
        let run_dir = PathBuf::from(&run_dir_raw);

        let handoff = match read_handoff(&run_dir) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("writer: {e}");
                return 1;
            }
        };
        // Fresh role: retain the trusted launch identity, never sample
        // expected state from the current process. PMv2 before geometry.
        if require_pm_v2().is_err() {
            eprintln!("writer: PMv2 not active; zero writes");
            return 1;
        }
        if verify_fixture(&handoff).is_err() || verify_fixture_medium(&handoff).is_err() {
            eprintln!("writer: fixture identity not verified; zero writes");
            return 1;
        }
        let Some(entry) = handoff.roster.iter().find(|e| e.caller == caller) else {
            eprintln!("writer: unknown caller");
            return 1;
        };
        let snapshot = match fixture_snapshot(
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            caller.as_str(),
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("writer: {e}");
                return 1;
            }
        };
        let title = snapshot
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if title != entry.title {
            eprintln!("writer: title moved; zero writes");
            return 1;
        }
        let Some(hwnd) = resolve_hwnd(&snapshot, handoff.fixture_pid, &title) else {
            eprintln!("writer: HWND join failed; zero writes");
            return 1;
        };
        if hwnd != entry.hwnd {
            eprintln!("writer: HWND churned; stale roster discarded, zero writes");
            return 1;
        }
        if no_mutate {
            // Nonmutating child: ready, then bounded stay. No record, no writes.
            if std::fs::write(&ready_raw, b"ready\n").is_err() {
                return 1;
            }
            stay_bounded(stay_ms, stop_file.as_deref());
            return 0;
        }
        // Hide/reveal writer path (area 5): persists the PRE-hide baseline
        // (visibility, show-state, outer/DWM geometry) in a hide record,
        // hides via guarded ShowWindow, sets the marker effect, then stays
        // for the forced-kill leg. Selected by `--effect-visibility hide`.
        if flag(args, "--effect-visibility").as_deref() == Some("hide") {
            return writer_hide(
                &run_dir,
                &record_name,
                &ready_raw,
                stay_ms,
                &handoff,
                entry,
                hwnd,
            );
        }
        // Acquired-vs-first-seen: the writer only mutates a window whose live
        // preimage is non-topmost. First-seen topmost (or unreadable) refuses
        // with zero policy writes. Preowned baseline refuses even if live is
        // false (policy: never acquire a preowned window).
        if baseline_preowned(&run_dir, caller.as_str()) {
            eprintln!("writer: preowned baseline; zero writes");
            return 3;
        }
        let preimage_live = NativeBackend.read_topmost(hwnd);
        if preimage_live != Some(false) {
            eprintln!("writer: preowned/unreadable preimage; zero writes");
            return 3;
        }
        if acquire_for_write(false) != AcquireOutcome::Acquired {
            return 3;
        }
        // Marker preimage is the live scalar; opaque carries it for the
        // recoverer revert.
        let live_marker = match NativeBackend::read_marker_inner(hwnd) {
            Some(v) if v != 0 => v,
            _ => {
                eprintln!("writer: marker unreadable; zero writes");
                return 1;
            }
        };
        if live_marker != entry.marker {
            eprintln!("writer: marker moved; zero writes");
            return 1;
        }
        let record_json = match encode_record(&RecordSpec {
            schema: RECORD_SCHEMA,
            fixture_exe: &handoff.fixture_exe,
            fixture_pid: handoff.fixture_pid,
            fixture_creation: &handoff.fixture_creation,
            fixture_user: &handoff.fixture_user,
            fixture_session: handoff.fixture_session,
            fixture_integrity: handoff.fixture_integrity,
            caller: caller.as_str(),
            full_title: &entry.title,
            tag: &entry.tag,
            hwnd,
            effect_topmost: effect_top,
            preimage_topmost: preimage_live,
            opaque: &encode_marker_opaque(live_marker),
        }) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("writer: encode: {e}");
                return 1;
            }
        };
        // Atomic same-directory publish + sync BEFORE mutation.
        // Basename only (no new generic write tool); refuse silent overwrite.
        if record_name.contains(['/', '\\']) {
            eprintln!("writer: record name must be a basename");
            return 1;
        }
        if let Err(e) = publish_record_atomic(&run_dir, &record_name, &record_json) {
            eprintln!("writer: publish: {e}");
            return 1;
        }
        let mut counts = EffectCounts::default();
        // Refresh-inside chokepoints via the roster backend.
        let expected_top = match rejoin_live(&handoff, entry, hwnd) {
            Ok((facts, _)) => facts,
            Err(e) => {
                eprintln!("writer: rejoin: {e}");
                return 1;
            }
        };
        match guarded_topmost_write(
            &mut LiveRoster {
                handoff: &handoff,
                entry,
                hwnd,
            },
            &expected_top,
            effect_top,
            &mut counts,
        ) {
            WriteOutcome::AppliedVerified => {}
            WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                eprintln!("writer: guarded write refused");
                return 1;
            }
        }
        // Marker effect: set a distinct nonzero value, then the recoverer
        // reverts to the opaque preimage.
        let marker_effect = marker_effect_for(live_marker);
        let expected_marker = match rejoin_live(&handoff, entry, hwnd) {
            Ok((facts, _)) => facts,
            Err(e) => {
                eprintln!("writer: marker rejoin: {e}");
                return 1;
            }
        };
        match guarded_marker_write(
            &mut LiveRoster {
                handoff: &handoff,
                entry,
                hwnd,
            },
            &expected_marker,
            marker_effect,
            &mut counts,
        ) {
            WriteOutcome::AppliedVerified => {}
            WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                eprintln!("writer: marker write refused");
                return 1;
            }
        }
        // Postwrite/ready evidence persisted before waiting (supplementary;
        // the recoverer needs only the PRE-write record).
        let evidence = serde_json::json!({
            "effect_topmost": effect_top,
            "live_topmost": NativeBackend.read_topmost(hwnd),
            "policy_writes": counts.policy_writes,
        });
        if std::fs::write(run_dir.join("postwrite.json"), evidence.to_string()).is_err() {
            return 1;
        }
        if std::fs::write(&ready_raw, b"ready\n").is_err() {
            return 1;
        }
        stay_bounded(stay_ms, stop_file.as_deref());
        0
    }

    /// Hide writer body (area 5): baseline the live frame BEFORE any
    /// mutation, publish the hide record atomically, then hide + marker
    /// effect through the guarded chokepoints. The leg kills this exact
    /// process after ready; the fresh recoverer restores the baseline.
    fn writer_hide(
        run_dir: &Path,
        record_name: &str,
        ready_raw: &str,
        stay_ms: u64,
        handoff: &Handoff,
        entry: &RosterEntry,
        hwnd: u64,
    ) -> i32 {
        if baseline_preowned(run_dir, entry.caller.as_str()) {
            eprintln!("writer-hide: preowned baseline; zero writes");
            return 3;
        }
        let base = native_frame_of(hwnd);
        if !base.visible {
            eprintln!("writer-hide: already hidden preimage; zero writes");
            return 3;
        }
        if base.cloak == Some(true) {
            eprintln!("writer-hide: cloaked preimage; zero writes");
            return 3;
        }
        let live_marker = match NativeBackend::read_marker_inner(hwnd) {
            Some(v) if v != 0 => v,
            _ => {
                eprintln!("writer-hide: marker unreadable; zero writes");
                return 1;
            }
        };
        if live_marker != entry.marker {
            eprintln!("writer-hide: marker moved; zero writes");
            return 1;
        }
        if record_name.contains(['/', '\\']) {
            eprintln!("writer-hide: record name must be a basename");
            return 1;
        }
        let record_json = match encode_hide_record(&HideSpec {
            schema: RECORD_SCHEMA,
            fixture_exe: &handoff.fixture_exe,
            fixture_pid: handoff.fixture_pid,
            fixture_creation: &handoff.fixture_creation,
            fixture_user: &handoff.fixture_user,
            fixture_session: handoff.fixture_session,
            fixture_integrity: handoff.fixture_integrity,
            caller: entry.caller.as_str(),
            full_title: &entry.title,
            tag: &entry.tag,
            hwnd,
            preimage_visible: base.visible,
            preimage_show: base.placement_show,
            preimage_outer: base.outer_px,
            preimage_dwm: base.dwm_px,
            opaque: &encode_marker_opaque(live_marker),
        }) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("writer-hide: encode: {e}");
                return 1;
            }
        };
        if let Err(e) = publish_record_atomic(run_dir, record_name, &record_json) {
            eprintln!("writer-hide: publish: {e}");
            return 1;
        }
        let mut counts = EffectCounts::default();
        let expected_vis = match rejoin_live(handoff, entry, hwnd) {
            Ok((facts, _)) => facts,
            Err(e) => {
                eprintln!("writer-hide: rejoin: {e}");
                return 1;
            }
        };
        match guarded_visibility_write(
            handoff,
            entry,
            &expected_vis,
            ffi_ext::SW_HIDE,
            false,
            &mut counts,
        ) {
            WriteOutcome::AppliedVerified => {}
            WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                eprintln!("writer-hide: hide refused");
                return 1;
            }
        }
        let marker_effect = marker_effect_for(live_marker);
        let expected_marker = match rejoin_live(handoff, entry, hwnd) {
            Ok((facts, _)) => facts,
            Err(e) => {
                eprintln!("writer-hide: marker rejoin: {e}");
                return 1;
            }
        };
        match guarded_marker_write(
            &mut LiveRoster {
                handoff,
                entry,
                hwnd,
            },
            &expected_marker,
            marker_effect,
            &mut counts,
        ) {
            WriteOutcome::AppliedVerified => {}
            WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                eprintln!("writer-hide: marker write refused");
                return 1;
            }
        }
        let evidence = serde_json::json!({
            "kind": HIDE_RECORD_KIND,
            "effect_visible": false,
            "live_visible": tiler_windows::ffi::window::is_visible(hwnd),
            "live_cloak": tiler_windows::ffi::window::cloaked(hwnd),
            "baseline_show": base.placement_show,
            "baseline_outer": base.outer_px,
            "policy_writes": counts.policy_writes,
        });
        if std::fs::write(run_dir.join("postwrite-hide.json"), evidence.to_string()).is_err() {
            return 1;
        }
        if std::fs::write(ready_raw, b"ready\n").is_err() {
            return 1;
        }
        stay_bounded(stay_ms, None);
        0
    }

    /// Hide-record restore body (fresh recoverer child only): revalidates
    /// the exact original fixture identity, rejoins the native title, then
    /// restores baseline visibility (non-activating reveal), baseline outer
    /// geometry, and the marker preimage. Returns the process exit code
    /// (0 restored/no-op, 5 refused, 6 partial).
    fn recover_hide_live(run_dir: &Path, record_name: &str, record_json: &str) -> i32 {
        let record = match decode_hide_record(record_json) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("recover-hide: rejected record ({e}); zero writes");
                return 4;
            }
        };
        let handoff = match read_handoff(run_dir) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("recover-hide: {e}");
                return 1;
            }
        };
        if verify_fixture(&handoff).is_err() || verify_fixture_medium(&handoff).is_err() {
            eprintln!("recover-hide: fixture identity not verified; zero writes");
            return 1;
        }
        let Some(entry) = handoff.roster.iter().find(|e| e.caller == record.caller) else {
            eprintln!("recover-hide: record caller not in roster; zero writes");
            return 5;
        };
        let snapshot = match fixture_snapshot(
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &record.caller,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("recover-hide: {e}");
                return 1;
            }
        };
        let reply_title = snapshot.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let Some(hwnd) = resolve_hwnd(&snapshot, handoff.fixture_pid, reply_title) else {
            eprintln!("recover-hide: HWND join failed; zero writes");
            return 5;
        };
        if hwnd != entry.hwnd || hwnd != record.hwnd {
            eprintln!("recover-hide: HWND churned; stale roster discarded, zero writes");
            return 5;
        }
        let native_title = match ffi_ext::window_text(hwnd) {
            Some(t) => t,
            None => {
                eprintln!("recover-hide: native title unreadable; zero writes");
                return 5;
            }
        };
        if native_title != entry.title || native_title != record.full_title {
            eprintln!("recover-hide: native title moved; zero writes");
            return 5;
        }
        if baseline_preowned(run_dir, &record.caller) {
            eprintln!("recover-hide: preowned window; no recovery write");
            return 0;
        }
        let Ok((fresh, obs)) = rejoin_live(&handoff, entry, hwnd) else {
            eprintln!("recover-hide: rejoin refused; zero writes");
            return 5;
        };
        if !hide_record_authorizes(&record, &obs) {
            eprintln!("recover-hide: record does not authorize; zero writes");
            return 5;
        }
        let opaque = match unhex(&record.opaque_hex).and_then(|b| decode_marker_opaque(&b)) {
            Some(v) => v,
            None => {
                eprintln!("recover-hide: opaque preimage unreadable; zero writes");
                return 5;
            }
        };
        let intended_marker = marker_effect_for(opaque);
        let expected = GuardFacts {
            stored_key: WindowKey {
                hwnd: record.hwnd,
                pid: fresh.live_pid,
                creation: fresh.live_creation.clone(),
            },
            live_hwnd: fresh.live_hwnd,
            live_pid: fresh.live_pid,
            live_creation: fresh.live_creation.clone(),
            expected_exe: record.fixture_exe.clone(),
            live_exe: fresh.live_exe.clone(),
            expected_user: record.fixture_user.clone(),
            live_user: fresh.live_user.clone(),
            expected_session: record.fixture_session,
            live_session: fresh.live_session,
            expected_integrity: record.fixture_integrity,
            live_integrity: fresh.live_integrity,
            expected_caller: record.caller.clone(),
            live_caller: fresh.live_caller.clone(),
            expected_title: record.full_title.clone(),
            live_title: fresh.live_title.clone(),
            stored_tag: record.tag.clone(),
            live_tag: fresh.live_tag.clone(),
            exstyle_readable: true,
            bootstrap_tag_establishment: false,
        };
        let mut counts = EffectCounts::default();
        let live_visible = tiler_windows::ffi::window::is_visible(hwnd);
        let need_vis = live_visible != record.preimage_visible;
        let live_marker_once = NativeBackend::read_marker_inner(hwnd);
        let marker_known_need = match live_marker_once {
            Some(m) if m == opaque => None,
            Some(m) if m == intended_marker => Some(true),
            _ => Some(false),
        };
        let marker_unknown = marker_known_need == Some(false);
        if !need_vis && marker_known_need.is_none() {
            // Geometry may still have drifted under a hidden move: restore
            // the outer rect when it disagrees with the preimage.
            let cur_outer = tiler_windows::ffi::window::window_rect(hwnd)
                .map(|r| [r.left, r.top, r.right, r.bottom]);
            if record.preimage_outer.is_some()
                && cur_outer != record.preimage_outer
                && let Some(outer) = record.preimage_outer
            {
                match guarded_geometry_restore(&handoff, entry, &expected, outer, &mut counts) {
                    WriteOutcome::AppliedVerified => {}
                    _ => {
                        eprintln!("recover-hide: geometry restore refused");
                        return 5;
                    }
                }
                let _ = std::fs::write(
                    run_dir.join("recovered-hide.json"),
                    serde_json::json!({
                        "record": record_name,
                        "kind": HIDE_RECORD_KIND,
                        "policy_writes": counts.policy_writes,
                        "geometry_only": true,
                    })
                    .to_string(),
                );
                return 0;
            }
            eprintln!("recover-hide: already at preimage; no write");
            return 0;
        }
        if marker_unknown && !need_vis {
            // Visibility already at preimage; unknown marker drift preserved.
            // Geometry drift still restores independently below.
            let cur_outer = tiler_windows::ffi::window::window_rect(hwnd)
                .map(|r| [r.left, r.top, r.right, r.bottom]);
            if record.preimage_outer.is_some()
                && cur_outer != record.preimage_outer
                && let Some(outer) = record.preimage_outer
            {
                match guarded_geometry_restore(&handoff, entry, &expected, outer, &mut counts) {
                    WriteOutcome::AppliedVerified => {
                        let _ = std::fs::write(
                            run_dir.join("recovered-hide-partial.json"),
                            serde_json::json!({
                                "record": record_name,
                                "kind": HIDE_RECORD_KIND,
                                "policy_writes": counts.policy_writes,
                                "partial": true,
                            })
                            .to_string(),
                        );
                        eprintln!(
                            "recover-hide: partially restored; policy_writes={}",
                            counts.policy_writes
                        );
                        return 6;
                    }
                    _ => return 5,
                }
            }
            eprintln!("recover-hide: refused; zero writes");
            return 5;
        }
        let mut vis_restored = false;
        if need_vis {
            // Non-activating reveal restores a visible preimage; hide
            // restores a hidden one. Show-state beyond visibility is
            // verified via placement readback in the leg summary.
            let cmd = if record.preimage_visible {
                ffi_ext::SW_SHOWNA
            } else {
                ffi_ext::SW_HIDE
            };
            match guarded_visibility_write(
                &handoff,
                entry,
                &expected,
                cmd,
                record.preimage_visible,
                &mut counts,
            ) {
                WriteOutcome::AppliedVerified => vis_restored = true,
                WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                    eprintln!("recover-hide: visibility restore refused; zero further writes");
                    return 5;
                }
            }
        }
        // Geometry restore when the recorded outer disagrees (e.g. a hidden
        // move happened while the writer was dead).
        let mut geo_restored = false;
        let cur_outer = tiler_windows::ffi::window::window_rect(hwnd)
            .map(|r| [r.left, r.top, r.right, r.bottom]);
        if let Some(outer) = record.preimage_outer
            && cur_outer != Some(outer)
        {
            match guarded_geometry_restore(&handoff, entry, &expected, outer, &mut counts) {
                WriteOutcome::AppliedVerified => geo_restored = true,
                WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                    if vis_restored || counts.policy_writes > 0 {
                        let _ = std::fs::write(
                            run_dir.join("recovered-hide-partial.json"),
                            serde_json::json!({
                                "record": record_name,
                                "kind": HIDE_RECORD_KIND,
                                "policy_writes": counts.policy_writes,
                                "partial": true,
                            })
                            .to_string(),
                        );
                        eprintln!(
                            "recover-hide: partially restored; policy_writes={}",
                            counts.policy_writes
                        );
                        return 6;
                    }
                    return 5;
                }
            }
        }
        let _ = (vis_restored, geo_restored);
        match marker_known_need {
            None => {
                let _ = std::fs::write(
                    run_dir.join("recovered-hide.json"),
                    serde_json::json!({
                        "record": record_name,
                        "kind": HIDE_RECORD_KIND,
                        "policy_writes": counts.policy_writes,
                    })
                    .to_string(),
                );
                0
            }
            Some(false) => {
                if need_vis || counts.policy_writes > 0 {
                    let _ = std::fs::write(
                        run_dir.join("recovered-hide-partial.json"),
                        serde_json::json!({
                            "record": record_name,
                            "kind": HIDE_RECORD_KIND,
                            "policy_writes": counts.policy_writes,
                            "partial": true,
                        })
                        .to_string(),
                    );
                    eprintln!(
                        "recover-hide: partially restored; policy_writes={}",
                        counts.policy_writes
                    );
                    6
                } else {
                    eprintln!("recover-hide: refused; zero writes");
                    5
                }
            }
            Some(true) => match guarded_marker_write(
                &mut LiveRoster {
                    handoff: &handoff,
                    entry,
                    hwnd,
                },
                &expected,
                opaque,
                &mut counts,
            ) {
                WriteOutcome::AppliedVerified => {
                    let _ = std::fs::write(
                        run_dir.join("recovered-hide.json"),
                        serde_json::json!({
                            "record": record_name,
                            "kind": HIDE_RECORD_KIND,
                            "policy_writes": counts.policy_writes,
                        })
                        .to_string(),
                    );
                    0
                }
                WriteOutcome::PostcheckFailed | WriteOutcome::Refused => {
                    if need_vis || counts.policy_writes > 0 {
                        let _ = std::fs::write(
                            run_dir.join("recovered-hide-partial.json"),
                            serde_json::json!({
                                "record": record_name,
                                "kind": HIDE_RECORD_KIND,
                                "policy_writes": counts.policy_writes,
                                "partial": true,
                            })
                            .to_string(),
                        );
                        eprintln!(
                            "recover-hide: partially restored; policy_writes={}",
                            counts.policy_writes
                        );
                        6
                    } else {
                        eprintln!("recover-hide: refused; zero writes");
                        5
                    }
                }
            },
        }
    }

    fn recoverer(args: &[String]) -> i32 {
        let Some(dir) = flag(args, "--run-dir") else {
            eprintln!("recoverer: --run-dir is required");
            return 2;
        };
        let Some(name) = flag(args, "--record") else {
            eprintln!("recoverer: --record is required");
            return 2;
        };
        recover_live(Path::new(&dir), &name)
    }

    /// Shared recovery core (fresh recoverer CHILD only; the controller never
    /// calls this in-process): load the record by basename from the exact run
    /// directory, revalidate original fixture identity, rejoin native title
    /// exactly, restore the PREIMAGE. Manual-friendly: no new windows/writer.
    fn recover_live(run_dir: &Path, record_name: &str) -> i32 {
        if record_name.contains(['/', '\\']) {
            eprintln!("recover: record name must be a basename; zero writes");
            return 1;
        }
        if require_pm_v2().is_err() {
            eprintln!("recover: PMv2 not active; zero writes");
            return 1;
        }
        let handoff = match read_handoff(run_dir) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("recover: {e}");
                return 1;
            }
        };
        if verify_fixture(&handoff).is_err() || verify_fixture_medium(&handoff).is_err() {
            eprintln!("recover: fixture identity not verified; zero writes");
            return 1;
        }
        let record_json = match std::fs::read_to_string(run_dir.join(record_name)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("recover: read record: {e}");
                return 1;
            }
        };
        // Hide records route to the visibility/geometry restore path; all
        // other records use the existing topmost preimage path.
        if decode_hide_record(&record_json).is_ok() {
            return recover_hide_live(run_dir, record_name, &record_json);
        }
        let record = match decode_record(&record_json) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("recover: rejected record ({e}); zero writes");
                return 4;
            }
        };
        let Some(entry) = handoff.roster.iter().find(|e| e.caller == record.caller) else {
            eprintln!("recover: record caller not in roster; zero writes");
            return 5;
        };
        let snapshot = match fixture_snapshot(
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &record.caller,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("recover: {e}");
                return 1;
            }
        };
        // Native title is the join authority, not the reply title: read the
        // live text and require the exact full expected match.
        let reply_title = snapshot.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let Some(hwnd) = resolve_hwnd(&snapshot, handoff.fixture_pid, reply_title) else {
            eprintln!("recover: HWND join failed; zero writes");
            return 5;
        };
        if hwnd != entry.hwnd {
            eprintln!("recover: HWND churned; stale roster discarded, zero writes");
            return 5;
        }
        let native_title = match ffi_ext::window_text(hwnd) {
            Some(t) => t,
            None => {
                eprintln!("recover: native title unreadable; zero writes");
                return 5;
            }
        };
        if native_title != entry.title {
            eprintln!("recover: native title moved; zero writes");
            return 5;
        }
        // Preowned from the setup baseline: an originally-topmost (or
        // unreadable-exstyle) window is never acquired or recovered.
        let preowned = baseline_preowned(run_dir, &record.caller);
        let mut counts = EffectCounts::default();
        let mut backend = LiveRoster {
            handoff: &handoff,
            entry,
            hwnd,
        };
        match recover_from_record(&record_json, preowned, &mut backend, &mut counts) {
            RecoverOutcome::Restored => {
                let _ = std::fs::write(
                    run_dir.join("recovered.json"),
                    serde_json::json!({
                        "record": record_name,
                        "effect_topmost": record.effect_topmost,
                        "policy_writes": counts.policy_writes,
                    })
                    .to_string(),
                );
                0
            }
            RecoverOutcome::NoWriteAlreadyAtPreimage => {
                eprintln!("recover: already at preimage; no write");
                0
            }
            RecoverOutcome::NoWritePreowned => {
                eprintln!("recover: preowned window; no acquire/recovery write");
                0
            }
            RecoverOutcome::PartiallyRestored => {
                let _ = std::fs::write(
                    run_dir.join("recovered-partial.json"),
                    serde_json::json!({
                        "record": record_name,
                        "effect_topmost": record.effect_topmost,
                        "policy_writes": counts.policy_writes,
                        "partial": true,
                    })
                    .to_string(),
                );
                eprintln!(
                    "recover: partially restored; policy_writes={}",
                    counts.policy_writes
                );
                6
            }
            RecoverOutcome::RefusedNoWrites => {
                eprintln!("recover: refused; zero writes");
                5
            }
        }
    }

    #[allow(dead_code)]
    fn publish_record_atomic(run_dir: &Path, name: &str, json: &str) -> Result<(), String> {
        let target = run_dir.join(name);
        if target.exists() {
            return Err("record exists; refusing silent overwrite".to_string());
        }
        let tmp = run_dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, json).map_err(|e| format!("write tmp: {e}"))?;
        // Sync needs a write-capable handle: FlushFileBuffers on a
        // read-only open fails with access-denied on Windows.
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&tmp)
            .map_err(|e| format!("open tmp: {e}"))?;
        file.sync_all().map_err(|e| format!("sync tmp: {e}"))?;
        drop(file);
        move_file_no_replace(&tmp, &target).map_err(|e| format!("publish: {e}"))?;
        Ok(())
    }

    /// Revalidate the exact original fixture launch identity. Any move fails
    /// closed with no native writes downstream.
    fn verify_fixture(h: &Handoff) -> Result<(), String> {
        let held =
            HeldProcess::open(h.fixture_pid).map_err(|_| "fixture process absent".to_string())?;
        let id = held
            .identity()
            .map_err(|e| format!("fixture identity: {e}"))?;
        let integrity = held
            .integrity()
            .map_err(|e| format!("fixture integrity: {e}"))?;
        if id.pid != h.fixture_pid
            || id.exe_path != h.fixture_exe
            || id.process_creation != h.fixture_creation
            || id.user_sid != h.fixture_user
            || id.session_id != h.fixture_session
            || integrity != h.fixture_integrity
        {
            return Err("fixture identity moved".to_string());
        }
        Ok(())
    }

    /// Medium-integrity binding: the probe only runs at
    /// SECURITY_MANDATORY_MEDIUM_RID. Any other level refuses.
    fn verify_fixture_medium(h: &Handoff) -> Result<(), String> {
        if h.fixture_integrity != MEDIUM_RID {
            return Err("fixture integrity is not medium; refusing".to_string());
        }
        Ok(())
    }

    /// Preowned comes from the setup baseline: originally topmost (or an
    /// unreadable exstyle read) means the window was never ours to acquire.
    fn baseline_preowned(run_dir: &Path, caller: &str) -> bool {
        let raw = std::fs::read_to_string(run_dir.join("baselines.json")).unwrap_or_default();
        let baselines: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap_or_default();
        baselines
            .iter()
            .find(|b| b.get("caller").and_then(|v| v.as_str()) == Some(caller))
            .and_then(|b| b.get("exstyle"))
            .and_then(|v| v.as_u64())
            .map(|ex| ex & u64::from(WS_EX_TOPMOST_BIT) != 0)
            .unwrap_or(true)
    }

    /// Spawn the exact probe executable with exact `.arg` forwarding (no
    /// shell quoting layer). Child stdio goes to bounded files in the run dir.
    fn spawn_probe(
        probe_exe: &Path,
        argv: &[&str],
        run_dir: &Path,
        log_name: &str,
    ) -> Result<std::process::Child, String> {
        let log =
            std::fs::File::create(run_dir.join(log_name)).map_err(|e| format!("log file: {e}"))?;
        let err_log = log.try_clone().map_err(|e| format!("log clone: {e}"))?;
        let mut cmd = std::process::Command::new(probe_exe);
        for arg in argv {
            cmd.arg(arg);
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(log)
            .stderr(err_log)
            .spawn()
            .map_err(|e| format!("spawn: {e}"))
    }

    fn wait_for_file(path: &Path, timeout_ms: u64) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed().as_millis() < u128::from(timeout_ms) {
            if path.exists() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        path.exists()
    }

    fn wait_child_exit(child: &mut std::process::Child, timeout_ms: u64) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed().as_millis() < u128::from(timeout_ms) {
            match child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                Err(_) => return false,
            }
        }
        matches!(child.try_wait(), Ok(Some(_)))
    }

    /// Bounded stay for writer/preflight children: stop-file exit or timeout.
    fn stay_bounded(stay_ms: u64, stop_file: Option<&Path>) {
        let start = std::time::Instant::now();
        while start.elapsed().as_millis() < u128::from(stay_ms) {
            if stop_file.is_some_and(|p| p.exists()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn proc_snapshot_of(pid: u32) -> Result<ProcSnapshot, String> {
        let held = HeldProcess::open(pid).map_err(|_| "target process absent".to_string())?;
        let id = held
            .identity()
            .map_err(|e| format!("target identity: {e}"))?;
        let integrity = held
            .integrity()
            .map_err(|e| format!("target integrity: {e}"))?;
        let session = tiler_windows::ffi::session_of_pid(pid).unwrap_or(u32::MAX);
        Ok(ProcSnapshot {
            pid: id.pid,
            exe: id.exe_path,
            creation: id.process_creation,
            user: id.user_sid,
            session,
            integrity,
        })
    }

    /// Terminate ONLY the exact original process: every expected field is
    /// mandatory (empty expected refuses), liveness is rechecked immediately
    /// before terminate, and medium integrity is bound. No fail-open, no 0
    /// expected, no session skip.
    fn terminate_exact(expected: &ProcSnapshot) -> Result<(), String> {
        if expected.pid == 0
            || expected.exe.is_empty()
            || expected.creation.is_empty()
            || expected.user.is_empty()
            || expected.integrity != MEDIUM_RID
        {
            return Err("refusing kill: incomplete expected identity".to_string());
        }
        let held = HeldProcess::open_terminate(expected.pid)
            .map_err(|_| "target process absent".to_string())?;
        let id = held
            .identity()
            .map_err(|e| format!("target identity: {e}"))?;
        let integrity = held
            .integrity()
            .map_err(|e| format!("target integrity: {e}"))?;
        let session = tiler_windows::ffi::session_of_pid(expected.pid).unwrap_or(u32::MAX);
        let live = ProcSnapshot {
            pid: id.pid,
            exe: id.exe_path,
            creation: id.process_creation,
            user: id.user_sid,
            session,
            integrity,
        };
        if !exact_identity_match(expected, &live) || live.integrity != MEDIUM_RID {
            return Err("target identity moved; refusing kill".to_string());
        }
        held.terminate().map_err(|e| format!("terminate: {e}"))?;
        Ok(())
    }

    /// Guarded per-window close: snapshot, resolve HWND, fresh rejoin, exact
    /// native/protocol caller join, reverify full identity/tag, then send
    /// close [caller]. Tag is kept until close (window destruction removes
    /// it); teardown never removes tags.
    fn close_one(fixture_exe: &str, handoff: &Handoff, entry: &RosterEntry) -> Result<(), String> {
        verify_fixture(handoff)?;
        verify_fixture_medium(handoff)?;
        let snapshot =
            fixture_snapshot(fixture_exe, &handoff.socket, &handoff.nonce, &entry.caller)
                .map_err(|e| format!("snapshot: {e}"))?;
        let Some(hwnd) = resolve_hwnd(&snapshot, handoff.fixture_pid, &entry.title) else {
            return Err(format!("HWND join failed for {}", entry.caller));
        };
        if hwnd != entry.hwnd {
            return Err(format!("HWND churned for {}", entry.caller));
        }
        // Fresh rejoin + tag present + exact caller/title join.
        let (_facts, obs) = rejoin_live(handoff, entry, hwnd)?;
        if obs.caller != entry.caller || obs.title != entry.title {
            return Err(format!("caller/title moved for {}", entry.caller));
        }
        match &obs.live_tag {
            LiveTag::Value(v) if visible_lifetime_ok(&entry.tag, Some(tag_hex(*v).as_str())) => {}
            _ => return Err(format!("tag not present for {}", entry.caller)),
        }
        let close = serde_json::json!({"v":1,"request_id":"probe-close","nonce":handoff.nonce,"op":"close","ids":[entry.caller.clone()]})
            .to_string();
        send_mutation(
            handoff,
            fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &close,
        )?;
        Ok(())
    }

    /// Ordinary successful teardown: two-pass (pass 1 verifies every caller
    /// with zero sends; any failure preserves with zero mutation sends, pass
    /// 2 closes one caller at a time), then list-empty check, then shutdown
    /// with original server identity, then process/pipe absence verify. Never
    /// auto-kills the fixture on ambiguity; never removes tags.
    fn teardown_best_effort(_fixture_exe: &str, run_dir: &Path, handoff: &Handoff) -> i32 {
        // Pass 1: verify all guards with zero sends.
        let mut checks = Vec::new();
        for entry in &handoff.roster {
            let snapshot = match fixture_snapshot(
                _fixture_exe,
                &handoff.socket,
                &handoff.nonce,
                &entry.caller,
            ) {
                Ok(s) => s,
                Err(_) => {
                    checks.push((entry.caller.as_str(), false));
                    continue;
                }
            };
            let title = snapshot.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let clean = resolve_hwnd(&snapshot, handoff.fixture_pid, title)
                .is_some_and(|hwnd| hwnd == entry.hwnd)
                && rejoin_live(handoff, entry, entry.hwnd).is_ok();
            checks.push((entry.caller.as_str(), clean));
        }
        let Some(ids) = planned_closes(&checks) else {
            eprintln!("teardown: pass 1 not all clean; preserving state, zero close sends");
            eprintln!(
                "manual guarded recovery: inspect run dir files, then close each verified caller via guarded close_one and shutdown"
            );
            let _ = run_dir;
            return 1;
        };
        // Pass 2: close one caller at a time with full guards.
        for caller in &ids {
            let Some(entry) = handoff.roster.iter().find(|e| &e.caller == caller) else {
                eprintln!("teardown: unknown caller {caller}; preserving");
                return 1;
            };
            if let Err(e) = close_one(_fixture_exe, handoff, entry) {
                eprintln!("teardown: close {caller} refused ({e}); preserving residue");
                return 1;
            }
        }
        let list =
            serde_json::json!({"v":1,"request_id":"probe-list","nonce":handoff.nonce,"op":"list"});
        match send_one(
            _fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &list.to_string(),
        ) {
            Ok(out) => {
                let remaining: usize = serde_json::from_str::<serde_json::Value>(&out)
                    .ok()
                    .and_then(|v| {
                        v.pointer("/result/windows")
                            .and_then(|w| w.as_array())
                            .map(|a| a.len())
                    })
                    .or_else(|| {
                        serde_json::from_str::<serde_json::Value>(&out)
                            .ok()
                            .and_then(|v| {
                                v.pointer("/result/ids")
                                    .and_then(|w| w.as_array())
                                    .map(|a| a.len())
                            })
                    })
                    .unwrap_or(usize::MAX);
                if remaining != 0 {
                    eprintln!("teardown: run roster not empty ({remaining}); no shutdown");
                    return 1;
                }
            }
            Err(e) => {
                eprintln!("teardown: list failed: {e}");
                return 1;
            }
        }
        // Shutdown with original server identity (empty roster verified).
        match shutdown_and_verify(_fixture_exe, handoff, &list.to_string()) {
            Ok(()) => {
                let _ = run_dir;
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    }

    /// Guarded shutdown plus process/pipe absence verification. Shared by
    /// ordinary teardown and measurement-specific cleanup. Never kills.
    fn shutdown_and_verify(
        fixture_exe: &str,
        handoff: &Handoff,
        list_payload: &str,
    ) -> Result<(), String> {
        if verify_fixture(handoff).is_err() || verify_fixture_medium(handoff).is_err() {
            return Err("teardown: server identity moved; no shutdown send".to_string());
        }
        let shutdown = serde_json::json!({"v":1,"request_id":"probe-bye","nonce":handoff.nonce,"op":"shutdown"});
        if send_mutation(
            handoff,
            fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &shutdown.to_string(),
        )
        .is_err()
        {
            return Err("teardown: shutdown failed; preserving state, no kill".to_string());
        }
        // Verify process absence (bounded) and pipe absence (connect fails).
        let start = std::time::Instant::now();
        let mut gone = false;
        while start.elapsed() < std::time::Duration::from_secs(10) {
            if HeldProcess::open(handoff.fixture_pid).is_err() {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if !gone {
            return Err(
                "teardown: fixture process still present; no kill, preserving state".to_string(),
            );
        }
        if send_one(fixture_exe, &handoff.socket, &handoff.nonce, list_payload).is_ok() {
            return Err("teardown: pipe still answers; preserving state".to_string());
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Bounded measurement legs (priorities 1-3 + DXGI assessment). Each leg
    // runs in its own process invocation with its own fixture server and run
    // dir, reusing the private setup/writer/recoverer machinery above. Raw
    // evidence stays in the external run dir, never in the repository.
    // -----------------------------------------------------------------------

    fn measure123(args: &[String]) -> i32 {
        let Some(leg) = flag(args, "--leg") else {
            eprintln!("measure123: --leg is required");
            return 2;
        };
        match leg.as_str() {
            "r14" => measure_r14(args),
            "topmost-preowned" => measure_topmost_preowned(args),
            "topmost-loss" => measure_topmost_loss(args),
            "topmost-extclear" => measure_topmost_extclear(args),
            "topmost-exttrue" => measure_topmost_exttrue(args),
            "storage-churn" => measure_storage_churn(args),
            "storage-stale" => measure_storage_stale(args),
            "dxgi" => measure_dxgi(args),
            "k5" => measure_k5(args),
            "k5-resize" => measure_k5_resize(args),
            "hide-loss" => measure_hide_loss(args),
            "identity" => measure_identity(args),
            _ => {
                eprintln!("measure123: unknown --leg {leg}");
                2
            }
        }
    }

    /// Measurement-only spawn-mode table (`caller=mode,...`). Only modes the
    /// fixture can enter without a display-mode change are accepted.
    fn parse_spawn_modes(raw: Option<&str>) -> Result<Vec<(String, String)>, String> {
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for part in raw.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (caller, mode) = part
                .split_once('=')
                .ok_or_else(|| format!("bad --spawn-modes entry {part:?}"))?;
            let caller = caller.trim().to_string();
            let mode = mode.trim().to_string();
            if caller.is_empty() {
                return Err(format!("bad --spawn-modes entry {part:?}"));
            }
            if !matches!(
                mode.as_str(),
                "windowed" | "maximized" | "minimized" | "borderless_fullscreen"
            ) {
                return Err(format!("unsupported --spawn-modes mode {mode:?}"));
            }
            out.push((caller, mode));
        }
        Ok(out)
    }

    /// Native topmost-bit poll (bounded). True only when a live read reports
    /// the wanted bit inside the budget.
    fn wait_native_topmost_bit(hwnd: u64, want: bool, timeout_ms: u64) -> bool {
        let start = std::time::Instant::now();
        loop {
            if ffi_ext::window_exstyle_checked(hwnd).map(topmost_of_exstyle) == Some(want) {
                return true;
            }
            if start.elapsed().as_millis() >= u128::from(timeout_ms) {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// One native geometry/style/state readback for a roster window.
    #[derive(Debug, Clone, serde::Serialize)]
    struct NativeFrame {
        hwnd: u64,
        outer_px: Option<[i32; 4]>,
        dwm_px: Option<[i32; 4]>,
        client_px: Option<[i32; 4]>,
        style: Option<u32>,
        exstyle: Option<u32>,
        topmost: Option<bool>,
        placement_show: Option<u32>,
        placement_flags: Option<u32>,
        cloak: Option<bool>,
        visible: bool,
        foreground: bool,
        marker: Option<u64>,
        tag_present: bool,
    }

    fn rect4(r: Option<windows_sys::Win32::Foundation::RECT>) -> Option<[i32; 4]> {
        r.map(|v| [v.left, v.top, v.right, v.bottom])
    }

    fn native_frame_of(hwnd: u64) -> NativeFrame {
        let exstyle = ffi_ext::window_exstyle_checked(hwnd);
        let placement = tiler_windows::ffi::window::window_placement(hwnd);
        NativeFrame {
            hwnd,
            outer_px: rect4(window_rect(hwnd)),
            dwm_px: rect4(extended_frame_bounds(hwnd)),
            client_px: ffi_ext::client_rect_screen(hwnd).map(|(l, t, r, b)| [l, t, r, b]),
            style: tiler_windows::ffi::window::window_style_checked(hwnd),
            exstyle,
            topmost: exstyle.map(topmost_of_exstyle),
            placement_show: placement.as_ref().map(|p| p.showCmd),
            placement_flags: placement.as_ref().map(|p| p.flags),
            cloak: tiler_windows::ffi::window::cloaked(hwnd),
            visible: tiler_windows::ffi::window::is_visible(hwnd),
            foreground: tiler_windows::ffi::window::foreground_window() == hwnd,
            marker: NativeBackend::read_marker_inner(hwnd),
            tag_present: matches!(NativeBackend::read_tag_inner(hwnd), LiveTag::Value(_)),
        }
    }

    fn roster_entry<'a>(handoff: &'a Handoff, caller: &str) -> Result<&'a RosterEntry, String> {
        handoff
            .roster
            .iter()
            .find(|e| e.caller == caller)
            .ok_or_else(|| format!("roster lacks {caller}"))
    }

    /// Join a roster caller to its live HWND and capture one native frame.
    /// Refuses on HWND churn (stale roster, zero writes downstream).
    fn capture_frame(handoff: &Handoff, entry: &RosterEntry) -> Result<NativeFrame, String> {
        let snapshot = fixture_snapshot(
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &entry.caller,
        )?;
        let title = snapshot.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let hwnd = resolve_hwnd(&snapshot, handoff.fixture_pid, title)
            .ok_or_else(|| format!("HWND join failed for {}", entry.caller))?;
        if hwnd != entry.hwnd {
            return Err(format!("HWND churned for {}", entry.caller));
        }
        Ok(native_frame_of(hwnd))
    }

    /// Fixture `observed.mode` readback, labeled as a setter cache on
    /// Windows (never a fullscreen proof). `Null` when unknown.
    fn observed_mode(snapshot: &serde_json::Value) -> serde_json::Value {
        snapshot
            .pointer("/observed/mode")
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    }

    /// Poll until outer+DWM frames repeat once (settled) or the budget ends.
    /// Returns the last frame plus whether it settled.
    fn settle_frame(
        handoff: &Handoff,
        entry: &RosterEntry,
        timeout_ms: u64,
    ) -> Result<(NativeFrame, bool), String> {
        let start = std::time::Instant::now();
        let mut prev: Option<NativeFrame> = None;
        loop {
            let frame = capture_frame(handoff, entry)?;
            if let Some(p) = &prev
                && p.outer_px == frame.outer_px
                && p.dwm_px == frame.dwm_px
            {
                return Ok((frame, true));
            }
            prev = Some(frame);
            if start.elapsed().as_millis() >= u128::from(timeout_ms) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        prev.map(|f| (f, false))
            .ok_or_else(|| "no frame captured".to_string())
    }

    /// Monitor full rects plus primary flag, for coverage comparison.
    fn monitor_fulls() -> Vec<([i32; 4], bool)> {
        enumerate_monitors()
            .map(|list| {
                list.iter()
                    .map(|m| {
                        (
                            [m.full.left, m.full.top, m.full.right, m.full.bottom],
                            m.primary,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn monitors_value(fulls: &[([i32; 4], bool)]) -> serde_json::Value {
        serde_json::json!(
            fulls
                .iter()
                .map(|(full, primary)| serde_json::json!({
                    "primary": primary,
                    "full": full,
                }))
                .collect::<Vec<_>>()
        )
    }

    /// Exact containment of a frame inside one monitor full rect
    /// (PMv2 physical pixels).
    fn frame_within(full: [i32; 4], frame: [i32; 4]) -> bool {
        full[0] <= frame[0] && full[1] <= frame[1] && frame[2] <= full[2] && frame[3] <= full[3]
    }

    fn coverage_verdict(fulls: &[([i32; 4], bool)], frame: &Option<[i32; 4]>) -> serde_json::Value {
        match frame {
            None => serde_json::json!({"frame": null, "verdict": "unreadable"}),
            Some(f) => {
                let exact = fulls.iter().any(|(full, _)| full == f);
                let within = fulls.iter().any(|(full, _)| frame_within(*full, *f));
                let verdict = if exact {
                    "covers-monitor-exact"
                } else if within {
                    "within-monitor"
                } else {
                    "no-cover"
                };
                serde_json::json!({"frame": f, "exact_monitor_full": exact, "within_monitor_full": within, "verdict": verdict})
            }
        }
    }

    /// App-own `set` through the owned server (guarded send).
    fn app_set(
        handoff: &Handoff,
        caller: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let payload = serde_json::json!({"v":1,"request_id":"measure-set","nonce":handoff.nonce,"op":"set","ids":[caller],"patch":patch}).to_string();
        let out = send_mutation(
            handoff,
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &payload,
        )?;
        serde_json::from_str(&out).map_err(|e| format!("parse set reply: {e}"))
    }

    /// Run `setup` for one measurement leg (own fixture server).
    fn run_setup(
        fixture_exe: &str,
        run_dir: &Path,
        callers: &str,
        extra: &[&str],
    ) -> Result<Handoff, i32> {
        if std::fs::create_dir_all(run_dir).is_err() {
            eprintln!("measure: create run dir");
            return Err(1);
        }
        let run_raw = run_dir.to_string_lossy().to_string();
        let mut argv = vec![
            "--fixture-exe".to_string(),
            fixture_exe.to_string(),
            "--run-dir".to_string(),
            run_raw,
            "--callers".to_string(),
            callers.to_string(),
        ];
        for chunk in extra.chunks(2) {
            argv.push(chunk[0].to_string());
            argv.push(chunk[1].to_string());
        }
        if setup(&argv) != 0 {
            return Err(1);
        }
        read_handoff(run_dir).map_err(|e| {
            eprintln!("measure: {e}");
            1
        })
    }

    /// Bounded child-exit wait returning the exact status (abnormal kills
    /// are evidence, not just booleans).
    fn wait_child_status(
        child: &mut std::process::Child,
        timeout_ms: u64,
    ) -> Option<std::process::ExitStatus> {
        let start = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => {
                    if start.elapsed().as_millis() >= u128::from(timeout_ms) {
                        return None;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => return None,
            }
        }
    }

    /// Fresh recoverer child (never in-process), logs to the run dir.
    fn run_recoverer(
        probe_exe: &Path,
        run_dir: &Path,
        run_raw: &str,
        record_name: &str,
        log_name: &str,
    ) -> Result<Option<std::process::ExitStatus>, String> {
        let mut child = spawn_probe(
            probe_exe,
            &["recoverer", "--run-dir", run_raw, "--record", record_name],
            run_dir,
            log_name,
        )
        .map_err(|e| format!("recoverer spawn: {e}"))?;
        Ok(wait_child_status(&mut child, 30_000))
    }

    /// Exact killed-writer provenance for one forced-loss leg.
    /// (Full identity also lands in `<caller>-writer-spawn.json`.)
    #[allow(dead_code)]
    struct WriterLoss {
        record_name: String,
        pid: u32,
        exe: String,
        creation: String,
        user: String,
        session: u32,
        integrity: u32,
        exit_code: Option<i32>,
    }

    /// Spawn a writer child for one guarded mutation set, wait for ready
    /// (mutation applied), then terminate exactly that child and record the
    /// abnormal exit. Preserves everything on any deviation.
    fn forced_writer_loss(
        probe_exe: &Path,
        run_dir: &Path,
        run_raw: &str,
        caller: &str,
        record_name: &str,
        ready_name: &str,
        stay_ms: u64,
    ) -> Result<WriterLoss, i32> {
        let ready = run_dir.join(ready_name);
        let stay_arg = stay_ms.to_string();
        let ready_arg = ready.to_string_lossy().to_string();
        let argv = [
            "writer",
            "--run-dir",
            run_raw,
            "--caller",
            caller,
            "--effect-top",
            "true",
            "--ready",
            &ready_arg,
            "--stay-ms",
            &stay_arg,
            "--record",
            record_name,
        ];
        let mut child = spawn_probe(probe_exe, &argv, run_dir, &format!("{caller}-writer.log"))
            .map_err(|e| {
                eprintln!("measure: writer spawn for {caller}: {e}; preserving");
                1
            })?;
        let snapshot = match proc_snapshot_of(child.id()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure: writer identity for {caller}: {e}; preserving");
                std::mem::forget(child);
                return Err(1);
            }
        };
        let _ = std::fs::write(
            run_dir.join(format!("{caller}-writer-spawn.json")),
            serde_json::json!({
                "caller": caller,
                "record": record_name,
                "argv": argv,
                "pid": snapshot.pid,
                "exe": snapshot.exe,
                "creation": snapshot.creation,
                "user": snapshot.user,
                "session": snapshot.session,
                "integrity": snapshot.integrity,
            })
            .to_string(),
        );
        if !wait_for_file(&ready, 20_000) {
            eprintln!("measure: writer never ready for {caller}; preserving (no kill)");
            std::mem::forget(child);
            return Err(1);
        }
        if terminate_exact(&snapshot).is_err() {
            eprintln!("measure: writer kill refused for {caller}; preserving");
            std::mem::forget(child);
            return Err(1);
        }
        match wait_child_status(&mut child, 10_000) {
            Some(status) => {
                let code = status.code();
                let abnormal = !status.success();
                let _ = std::fs::write(
                    run_dir.join(format!("{caller}-writer-kill.json")),
                    serde_json::json!({
                        "caller": caller,
                        "pid": snapshot.pid,
                        "kill": "exact-terminate",
                        "exit_code": code,
                        "exit_abnormal": abnormal,
                    })
                    .to_string(),
                );
                if !abnormal {
                    eprintln!("measure: writer for {caller} exited normally; preserving");
                    return Err(1);
                }
                Ok(WriterLoss {
                    record_name: record_name.to_string(),
                    pid: snapshot.pid,
                    exe: snapshot.exe,
                    creation: snapshot.creation,
                    user: snapshot.user,
                    session: snapshot.session,
                    integrity: snapshot.integrity,
                    exit_code: code,
                })
            }
            None => {
                eprintln!("measure: writer for {caller} survived exact kill; preserving");
                std::mem::forget(child);
                Err(1)
            }
        }
    }

    fn recovered_writes(run_dir: &Path, name: &str) -> Option<u64> {
        let raw = std::fs::read_to_string(run_dir.join(name)).ok()?;
        let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
        value.get("policy_writes")?.as_u64()
    }

    /// Read-only record survival check: the file decodes and its opaque
    /// body carries the expected marker preimage.
    fn record_survives(run_dir: &Path, record_name: &str, marker: u64) -> Result<bool, String> {
        let raw = std::fs::read_to_string(run_dir.join(record_name))
            .map_err(|e| format!("read record: {e}"))?;
        let record = decode_record(&raw).map_err(|_| "record rejected".to_string())?;
        let opaque = unhex(&record.opaque_hex).ok_or("opaque unhex failed".to_string())?;
        Ok(decode_marker_opaque(&opaque) == Some(marker))
    }

    /// Poll `get` until the caller id is gone from the roster (per-window
    /// ok=false). Transport failure is not disappearance proof.
    fn wait_caller_gone(handoff: &Handoff, caller: &str, timeout_ms: u64) -> bool {
        let start = std::time::Instant::now();
        loop {
            let payload = serde_json::json!({"v":1,"request_id":"measure-gone","nonce":handoff.nonce,"op":"get","ids":[caller]}).to_string();
            if let Ok(out) = send_one(
                &handoff.fixture_exe,
                &handoff.socket,
                &handoff.nonce,
                &payload,
            ) {
                let mut matched = false;
                let mut all_bad = true;
                for line in out.lines() {
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                        continue;
                    };
                    if value.pointer("/request_id").and_then(|r| r.as_str()) != Some("measure-gone")
                    {
                        continue;
                    }
                    matched = true;
                    if let Some(arr) = value.pointer("/result/windows").and_then(|w| w.as_array()) {
                        for entry in arr {
                            if entry.get("ok") == Some(&serde_json::Value::Bool(true)) {
                                all_bad = false;
                            }
                        }
                    }
                }
                if matched && all_bad {
                    return true;
                }
            }
            if start.elapsed().as_millis() >= u128::from(timeout_ms) {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    fn current_exe_or(code: i32) -> Result<std::path::PathBuf, i32> {
        std::env::current_exe().map_err(|e| {
            eprintln!("measure: current exe: {e}");
            code
        })
    }

    // -- R14: acquired-vs-first-seen borderless --------------------------------
    //
    // Normal caller: native PMv2 outer/DWM/client/styles/placement/cloak/
    // visible before, app-own set borderless via the fixture protocol,
    // native confirm of monitor coverage after settle, app-own windowed
    // exit with exact per-channel compare. Startup-borderless caller:
    // observe only, safe protocol close at teardown. `observed.mode` is
    // recorded as a setter cache, never a fullscreen proof.
    fn measure_r14(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 r14: --fixture-exe is required");
            return 2;
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 r14: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 r14: PMv2 not active; zero reads");
            return 1;
        }
        let handoff = match run_setup(
            &fixture_exe,
            &run_dir,
            "m14norm,m14boot",
            &["--spawn-modes", "m14boot=borderless_fullscreen"],
        ) {
            Ok(h) => h,
            Err(code) => return code,
        };
        let fulls = monitor_fulls();
        let _ = std::fs::write(
            run_dir.join("monitors.json"),
            monitors_value(&fulls).to_string(),
        );
        let norm = match roster_entry(&handoff, "m14norm") {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 r14: {e}; preserving");
                return 1;
            }
        };
        let boot = match roster_entry(&handoff, "m14boot") {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 r14: {e}; preserving");
                return 1;
            }
        };
        let before = match capture_frame(&handoff, norm) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 r14 before: {e}; preserving");
                return 1;
            }
        };
        let enter_reply = match app_set(
            &handoff,
            "m14norm",
            serde_json::json!({"mode": "borderless_fullscreen"}),
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 r14 enter: {e}; preserving");
                return 1;
            }
        };
        let (after_set, set_settled) = match settle_frame(&handoff, norm, 8_000) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("measure123 r14 settle-after-set: {e}; preserving");
                return 1;
            }
        };
        let coverage_set = coverage_verdict(&fulls, &after_set.dwm_px);
        let exit_reply = match app_set(&handoff, "m14norm", serde_json::json!({"mode": "windowed"}))
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 r14 exit: {e}; preserving");
                return 1;
            }
        };
        let (after_exit, exit_settled) = match settle_frame(&handoff, norm, 8_000) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("measure123 r14 settle-after-exit: {e}; preserving");
                return 1;
            }
        };
        let compare = serde_json::json!({
            "style": {"before": before.style, "after": after_exit.style, "equal": before.style == after_exit.style},
            "exstyle": {"before": before.exstyle, "after": after_exit.exstyle, "equal": before.exstyle == after_exit.exstyle},
            "placement_show": {"before": before.placement_show, "after": after_exit.placement_show, "equal": before.placement_show == after_exit.placement_show},
            "cloak": {"before": before.cloak, "after": after_exit.cloak, "equal": before.cloak == after_exit.cloak},
            "visible": {"before": before.visible, "after": after_exit.visible, "equal": before.visible == after_exit.visible},
            "outer_px": {"before": before.outer_px, "after": after_exit.outer_px, "equal": before.outer_px == after_exit.outer_px},
            "dwm_px": {"before": before.dwm_px, "after": after_exit.dwm_px, "equal": before.dwm_px == after_exit.dwm_px},
            "client_px": {"before": before.client_px, "after": after_exit.client_px, "equal": before.client_px == after_exit.client_px},
        });
        // First-seen borderless: observe only (no exit-forcing as policy).
        let (boot_frame, boot_settled) = match settle_frame(&handoff, boot, 8_000) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("measure123 r14 boot settle: {e}; preserving");
                return 1;
            }
        };
        let boot_snapshot = fixture_snapshot(
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            "m14boot",
        )
        .unwrap_or(serde_json::Value::Null);
        let summary = serde_json::json!({
            "leg": "r14",
            "pmv2": true,
            "monitors": monitors_value(&fulls),
            "before": before,
            "enter_reply": enter_reply,
            "after_set": after_set,
            "set_settled": set_settled,
            "coverage_after_set": coverage_set,
            "exit_reply": exit_reply,
            "after_exit": after_exit,
            "exit_settled": exit_settled,
            "compare_before_exit": compare,
            "boot_frame_first_seen": boot_frame,
            "boot_settled": boot_settled,
            "boot_coverage": coverage_verdict(&fulls, &boot_frame.dwm_px),
            "boot_observed_mode_cache": observed_mode(&boot_snapshot),
            "note": "observed.mode is a setter cache on Windows, not a fullscreen proof",
        });
        let _ = std::fs::write(run_dir.join("r14.json"), summary.to_string());
        teardown_best_effort(&fixture_exe, &run_dir, &handoff)
    }

    // -- Topmost legs ----------------------------------------------------------
    //
    // A: measured preowned-true arrangement (app-own spawn level before the
    // policy baseline): writer returns the preowned code with zero writes,
    // recoverer writes nothing regardless of the live bit.
    fn measure_topmost_preowned(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 topmost-preowned: --fixture-exe is required");
            return 2;
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 topmost-preowned: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 topmost-preowned: PMv2 not active");
            return 1;
        }
        let handoff = match run_setup(
            &fixture_exe,
            &run_dir,
            "tma_pre",
            &["--topmost-spawn", "tma_pre"],
        ) {
            Ok(h) => h,
            Err(code) => return code,
        };
        let probe_exe = match current_exe_or(1) {
            Ok(p) => p,
            Err(code) => return code,
        };
        let entry = match roster_entry(&handoff, "tma_pre") {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 topmost-preowned: {e}; preserving");
                return 1;
            }
        };
        // Arrangement proof: setup baseline measured preowned AND the live
        // bit is still true now.
        let baseline_preowned_flag = baseline_preowned(&run_dir, "tma_pre");
        let live_now = match capture_frame(&handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 topmost-preowned live read: {e}; preserving");
                return 1;
            }
        };
        if !baseline_preowned_flag || live_now.topmost != Some(true) {
            eprintln!(
                "measure123 topmost-preowned: arrangement not preowned-true (baseline={baseline_preowned_flag}, live={:?}); preserving",
                live_now.topmost
            );
            return 1;
        }
        // Writer child: must refuse with the preowned code (3), zero writes.
        let record_name = "write-tma_pre.json";
        let ready = run_dir.join("tma-writer.ready");
        let ready_arg = ready.to_string_lossy().to_string();
        let mut writer = match spawn_probe(
            &probe_exe,
            &[
                "writer",
                "--run-dir",
                &run_raw,
                "--caller",
                "tma_pre",
                "--effect-top",
                "true",
                "--ready",
                &ready_arg,
                "--stay-ms",
                "30000",
                "--record",
                record_name,
            ],
            &run_dir,
            "tma_pre-writer.log",
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("measure123 topmost-preowned writer spawn: {e}; preserving");
                return 1;
            }
        };
        let writer_status = wait_child_status(&mut writer, 35_000);
        let writer_exit = writer_status.as_ref().and_then(|s| s.code());
        if writer_exit != Some(3) {
            // Own created child only: stop it before preserving.
            if writer_status.is_none() {
                if let Ok(snapshot) = proc_snapshot_of(writer.id()) {
                    let _ = terminate_exact(&snapshot);
                }
                std::mem::forget(writer);
            }
            eprintln!(
                "measure123 topmost-preowned: writer exit {writer_exit:?}, expected Some(3); preserving"
            );
            return 1;
        }
        let record_absent = !run_dir.join(record_name).exists();
        let after_writer = match capture_frame(&handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 topmost-preowned post-writer read: {e}; preserving");
                return 1;
            }
        };
        // Recoverer on the setup record at live-true: zero writes.
        let recover_true = match run_recoverer(
            &probe_exe,
            &run_dir,
            &run_raw,
            "setup-tma_pre.json",
            "recoverer-true.log",
        ) {
            Ok(s) => s.and_then(|v| v.code()),
            Err(e) => {
                eprintln!("measure123 topmost-preowned recoverer: {e}; preserving");
                return 1;
            }
        };
        let recovered_absent_true = !run_dir.join("recovered.json").exists()
            && !run_dir.join("recovered-partial.json").exists();
        // External clear via the app-own fixture path, then recoverer again
        // at live-false: still zero writes.
        let ext_reply = match app_set(
            &handoff,
            "tma_pre",
            serde_json::json!({"window_level": "normal"}),
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 topmost-preowned external clear: {e}; preserving");
                return 1;
            }
        };
        let cleared = wait_native_topmost_bit(entry.hwnd, false, 5_000);
        let recover_false = match run_recoverer(
            &probe_exe,
            &run_dir,
            &run_raw,
            "setup-tma_pre.json",
            "recoverer-false.log",
        ) {
            Ok(s) => s.and_then(|v| v.code()),
            Err(e) => {
                eprintln!("measure123 topmost-preowned recoverer-2: {e}; preserving");
                return 1;
            }
        };
        let recovered_absent_false = !run_dir.join("recovered.json").exists()
            && !run_dir.join("recovered-partial.json").exists();
        let live_before_topmost = live_now.topmost;
        let after_all = capture_frame(&handoff, entry).unwrap_or(live_now);
        let summary = serde_json::json!({
            "leg": "topmost-preowned",
            "baseline_preowned": baseline_preowned_flag,
            "live_topmost_before": live_before_topmost,
            "writer_exit": writer_exit,
            "writer_record_absent": record_absent,
            "live_topmost_after_writer": after_writer.topmost,
            "marker_after_writer": after_writer.marker,
            "marker_expected": entry.marker,
            "recoverer_exit_live_true": recover_true,
            "recovered_absent_live_true": recovered_absent_true,
            "external_clear_reply": ext_reply,
            "external_clear_native_false": cleared,
            "recoverer_exit_live_false": recover_false,
            "recovered_absent_live_false": recovered_absent_false,
            "live_topmost_end": after_all.topmost,
            "note": "zero recovery writes in both live states; setup tag/marker bootstrap counts are setup-phase, never policy writes",
        });
        let _ = std::fs::write(run_dir.join("topmost-preowned.json"), summary.to_string());
        if writer_exit != Some(3)
            || !record_absent
            || recover_true != Some(0)
            || !recovered_absent_true
            || recover_false != Some(0)
            || !recovered_absent_false
        {
            eprintln!("measure123 topmost-preowned: expectation mismatch; preserving");
            return 1;
        }
        teardown_best_effort(&fixture_exe, &run_dir, &handoff)
    }

    /// Shared forced-loss prologue for the topmost loss/clear/true legs:
    /// setup, baseline reads, writer forced loss, post-loss reads.
    struct LossPrologue {
        handoff: Handoff,
        probe_exe: std::path::PathBuf,
        fixture_exe: String,
        run_raw: String,
        caller: String,
        record_name: String,
        loss: WriterLoss,
        live_after_loss: NativeFrame,
        record_ok: bool,
    }

    fn loss_prologue(args: &[String], leg: &str, caller: &str) -> Result<LossPrologue, i32> {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 {leg}: --fixture-exe is required");
            return Err(2);
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 {leg}: --run-dir is required");
            return Err(2);
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 {leg}: PMv2 not active");
            return Err(1);
        }
        let handoff = run_setup(&fixture_exe, &run_dir, caller, &[])?;
        let probe_exe = current_exe_or(1)?;
        let entry = roster_entry(&handoff, caller).map_err(|e| {
            eprintln!("measure123 {leg}: {e}; preserving");
            1
        })?;
        let before = capture_frame(&handoff, entry).map_err(|e| {
            eprintln!("measure123 {leg} before: {e}; preserving");
            1
        })?;
        if before.topmost != Some(false) || before.marker != Some(entry.marker) {
            eprintln!(
                "measure123 {leg}: baseline not acquired-clean (topmost={:?}, marker={:?} vs {}); preserving",
                before.topmost, before.marker, entry.marker
            );
            return Err(1);
        }
        let record_name = format!("write-{caller}.json");
        let loss = forced_writer_loss(
            &probe_exe,
            &run_dir,
            &run_raw,
            caller,
            &record_name,
            &format!("{caller}.ready"),
            30_000,
        )?;
        let live_after_loss = capture_frame(&handoff, entry).map_err(|e| {
            eprintln!("measure123 {leg} post-loss read: {e}; preserving");
            1
        })?;
        let expected_marker = marker_effect_for(entry.marker);
        if live_after_loss.topmost != Some(true) || live_after_loss.marker != Some(expected_marker)
        {
            eprintln!(
                "measure123 {leg}: post-loss state unexpected (topmost={:?}, marker={:?}); preserving",
                live_after_loss.topmost, live_after_loss.marker
            );
            return Err(1);
        }
        let record_ok = record_survives(&run_dir, &record_name, entry.marker).map_err(|e| {
            eprintln!("measure123 {leg} record survival: {e}; preserving");
            1
        })?;
        if !record_ok {
            eprintln!(
                "measure123 {leg}: opaque record body does not carry the preimage; preserving"
            );
            return Err(1);
        }
        Ok(LossPrologue {
            handoff,
            probe_exe,
            fixture_exe,
            run_raw,
            caller: caller.to_string(),
            record_name,
            loss,
            live_after_loss,
            record_ok,
        })
    }

    fn finish_topmost_leg(
        pro: &LossPrologue,
        summary_name: &str,
        summary: serde_json::Value,
        expect_writes: u64,
    ) -> i32 {
        let run_dir = PathBuf::from(&pro.run_raw);
        let _ = std::fs::write(run_dir.join(summary_name), summary.to_string());
        match recovered_writes(&run_dir, "recovered.json") {
            Some(w) if w == expect_writes => {
                teardown_best_effort(&pro.fixture_exe, &run_dir, &pro.handoff)
            }
            other => {
                eprintln!(
                    "measure123 {}: recovered policy_writes {other:?}, expected {expect_writes}; preserving",
                    summary_name
                );
                1
            }
        }
    }

    // B: forced writer loss on the normal case, then fresh recoverer.
    // Reads topmost/marker + record survival after loss, then verifies
    // false + original marker after the fresh recoverer.
    fn measure_topmost_loss(args: &[String]) -> i32 {
        let pro = match loss_prologue(args, "topmost-loss", "tmb_norm") {
            Ok(p) => p,
            Err(code) => return code,
        };
        let run_dir = PathBuf::from(&pro.run_raw);
        let entry = match roster_entry(&pro.handoff, &pro.caller) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 topmost-loss: {e}; preserving");
                return 1;
            }
        };
        let status = match run_recoverer(
            &pro.probe_exe,
            &run_dir,
            &pro.run_raw,
            &pro.record_name,
            "recoverer-loss.log",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure123 topmost-loss recoverer: {e}; preserving");
                return 1;
            }
        };
        let recover_exit = status.as_ref().and_then(|s| s.code());
        if recover_exit != Some(0) {
            eprintln!("measure123 topmost-loss: recoverer exit {recover_exit:?}; preserving");
            return 1;
        }
        let after = match capture_frame(&pro.handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 topmost-loss post-recover read: {e}; preserving");
                return 1;
            }
        };
        if after.topmost != Some(false) || after.marker != Some(entry.marker) {
            eprintln!(
                "measure123 topmost-loss: post-recover state unexpected (topmost={:?}, marker={:?}); preserving",
                after.topmost, after.marker
            );
            return 1;
        }
        let summary = serde_json::json!({
            "leg": "topmost-loss",
            "writer": {"pid": pro.loss.pid, "exit_code": pro.loss.exit_code, "exit_abnormal": true},
            "live_topmost_after_loss": pro.live_after_loss.topmost,
            "live_marker_after_loss": pro.live_after_loss.marker,
            "record_survives_with_preimage": pro.record_ok,
            "recoverer_exit": recover_exit,
            "live_topmost_after_recover": after.topmost,
            "live_marker_after_recover": after.marker,
            "marker_expected": entry.marker,
        });
        finish_topmost_leg(&pro, "topmost-loss.json", summary, 2)
    }

    /// Harness external native clear (NOT an app/protocol clear): a
    /// same-controller test intervention that natively clears the fixture
    /// window TOPMOST bit via the existing `ffi::set_topmost` Rust mutation
    /// behind the original fixture guard (`guarded_topmost_write`). It
    /// simulates an independent external setter for this mechanism
    /// experiment; the source is recorded explicitly in leg summaries.
    fn harness_external_native_clear(
        handoff: &Handoff,
        entry: &RosterEntry,
    ) -> Result<u64, String> {
        let expected = rejoin_live(handoff, entry, entry.hwnd)
            .map(|(facts, _)| facts)
            .map_err(|e| format!("harness clear rejoin: {e}"))?;
        let mut backend = LiveRoster {
            handoff,
            entry,
            hwnd: entry.hwnd,
        };
        let mut counts = EffectCounts::default();
        match guarded_topmost_write(&mut backend, &expected, false, &mut counts) {
            WriteOutcome::AppliedVerified => Ok(counts.policy_writes),
            WriteOutcome::PostcheckFailed => Err("harness clear postcheck failed".to_string()),
            WriteOutcome::Refused => Err("harness clear refused".to_string()),
        }
    }

    // C: external app-own clear after writer kill, then fresh recoverer
    // skips topmost (already at preimage) but restores the marker.
    fn measure_topmost_extclear(args: &[String]) -> i32 {
        let pro = match loss_prologue(args, "topmost-extclear", "tmc_ext") {
            Ok(p) => p,
            Err(code) => return code,
        };
        let run_dir = PathBuf::from(&pro.run_raw);
        let entry = match roster_entry(&pro.handoff, &pro.caller) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 topmost-extclear: {e}; preserving");
                return 1;
            }
        };
        let ext_reply = match app_set(
            &pro.handoff,
            &pro.caller,
            serde_json::json!({"window_level": "normal"}),
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 topmost-extclear external set: {e}; preserving");
                return 1;
            }
        };
        // Persist the protocol reply + immediate native readback BEFORE the
        // wait, so the accepted-but-ineffective app clear is filed as evidence
        // even if a later step preserves.
        let app_immediate = capture_frame(&pro.handoff, entry).ok();
        let _ = std::fs::write(
            run_dir.join("extclear-app-attempt.json"),
            serde_json::json!({
                "external_clear_reply": ext_reply,
                "immediate_topmost": app_immediate.as_ref().and_then(|f| f.topmost),
                "immediate_marker": app_immediate.as_ref().and_then(|f| f.marker),
                "immediate_exstyle": app_immediate.as_ref().and_then(|f| f.exstyle),
            })
            .to_string(),
        );
        // App-protocol clear is an accepted setter-cache no-op after a native
        // set (winit 0.30.13 `set_window_level` diffs cached flags:
        // platform_impl/windows/window.rs + window_state.rs `apply_diff`;
        // accepted reply is not a native oracle). Record the noop, then apply
        // the HARNESS external native clear below.
        let app_cleared = wait_native_topmost_bit(entry.hwnd, false, 5_000);
        let _ = std::fs::write(
            run_dir.join("extclear-app-readback.json"),
            serde_json::json!({
                "app_clear_native_false_within_5s": app_cleared,
                "live_topmost_after_wait": capture_frame(&pro.handoff, entry)
                    .map(|f| f.topmost)
                    .unwrap_or(None),
            })
            .to_string(),
        );
        // HARNESS external native clear (NOT app clear): same-controller test
        // intervention via ffi::set_topmost behind guarded_topmost_write.
        let harness_writes = match harness_external_native_clear(&pro.handoff, entry) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("measure123 topmost-extclear harness clear: {e}; preserving");
                return 1;
            }
        };
        let harness_topmost = ffi_ext::window_exstyle_checked(entry.hwnd).map(topmost_of_exstyle);
        let _ = std::fs::write(
            run_dir.join("extclear-harness-clear.json"),
            serde_json::json!({
                "source": "harness-external-native-clear via ffi::set_topmost behind guarded_topmost_write (same-controller test intervention, not app protocol)",
                "harness_clear_writes": harness_writes,
                "live_topmost_after_harness_clear": harness_topmost,
            })
            .to_string(),
        );
        if harness_topmost != Some(false) {
            eprintln!("measure123 topmost-extclear: harness clear did not read false; preserving");
            return 1;
        }
        let pre_recover = match capture_frame(&pro.handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 topmost-extclear pre-recover read: {e}; preserving");
                return 1;
            }
        };
        let status = match run_recoverer(
            &pro.probe_exe,
            &run_dir,
            &pro.run_raw,
            &pro.record_name,
            "recoverer-extclear.log",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure123 topmost-extclear recoverer: {e}; preserving");
                return 1;
            }
        };
        let recover_exit = status.as_ref().and_then(|s| s.code());
        if recover_exit != Some(0) {
            eprintln!("measure123 topmost-extclear: recoverer exit {recover_exit:?}; preserving");
            return 1;
        }
        let after = match capture_frame(&pro.handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 topmost-extclear post-recover read: {e}; preserving");
                return 1;
            }
        };
        if after.topmost != Some(false) || after.marker != Some(entry.marker) {
            eprintln!(
                "measure123 topmost-extclear: post-recover state unexpected (topmost={:?}, marker={:?}); preserving",
                after.topmost, after.marker
            );
            return 1;
        }
        let summary = serde_json::json!({
            "leg": "topmost-extclear",
            "writer": {"pid": pro.loss.pid, "exit_code": pro.loss.exit_code, "exit_abnormal": true},
            "external_clear_reply": ext_reply,
            "app_clear_native_false_within_5s": app_cleared,
            "app_clear_mechanism": "accepted-but-ineffective: winit 0.30.13 set_window_level diffs cached flags (window.rs + window_state.rs apply_diff); accepted reply is not a native oracle",
            "harness_external_native_clear_source": "harness-external-native-clear via ffi::set_topmost behind guarded_topmost_write (same-controller test intervention, not app protocol)",
            "harness_clear_writes": harness_writes,
            "live_topmost_before_recover": pre_recover.topmost,
            "live_marker_before_recover": pre_recover.marker,
            "recoverer_exit": recover_exit,
            "live_topmost_after_recover": after.topmost,
            "live_marker_after_recover": after.marker,
            "marker_expected": entry.marker,
            "note": "app protocol clear is an accepted setter-cache no-op; harness native clear restores false externally, then recoverer skips topmost and restores marker with 1 policy write",
        });
        finish_topmost_leg(&pro, "topmost-extclear.json", summary, 1)
    }

    // D: external app-own true->true after writer kill (bit unchanged, no
    // setter-identity inference), then fresh recoverer restores false.
    fn measure_topmost_exttrue(args: &[String]) -> i32 {
        let pro = match loss_prologue(args, "topmost-exttrue", "tmd_ext") {
            Ok(p) => p,
            Err(code) => return code,
        };
        let run_dir = PathBuf::from(&pro.run_raw);
        let entry = match roster_entry(&pro.handoff, &pro.caller) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 topmost-exttrue: {e}; preserving");
                return 1;
            }
        };
        let bit_before = capture_frame(&pro.handoff, entry)
            .map(|f| f.topmost)
            .unwrap_or(None);
        let ext_reply = match app_set(
            &pro.handoff,
            &pro.caller,
            serde_json::json!({"window_level": "always_on_top"}),
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 topmost-exttrue external set: {e}; preserving");
                return 1;
            }
        };
        let bit_after = capture_frame(&pro.handoff, entry)
            .map(|f| f.topmost)
            .unwrap_or(None);
        if bit_before != Some(true) || bit_after != Some(true) {
            eprintln!(
                "measure123 topmost-exttrue: bit moved across external set ({bit_before:?} -> {bit_after:?}); preserving"
            );
            return 1;
        }
        let status = match run_recoverer(
            &pro.probe_exe,
            &run_dir,
            &pro.run_raw,
            &pro.record_name,
            "recoverer-exttrue.log",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure123 topmost-exttrue recoverer: {e}; preserving");
                return 1;
            }
        };
        let recover_exit = status.as_ref().and_then(|s| s.code());
        if recover_exit != Some(0) {
            eprintln!("measure123 topmost-exttrue: recoverer exit {recover_exit:?}; preserving");
            return 1;
        }
        let after = match capture_frame(&pro.handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 topmost-exttrue post-recover read: {e}; preserving");
                return 1;
            }
        };
        if after.topmost != Some(false) || after.marker != Some(entry.marker) {
            eprintln!(
                "measure123 topmost-exttrue: post-recover state unexpected (topmost={:?}, marker={:?}); preserving",
                after.topmost, after.marker
            );
            return 1;
        }
        let summary = serde_json::json!({
            "leg": "topmost-exttrue",
            "writer": {"pid": pro.loss.pid, "exit_code": pro.loss.exit_code, "exit_abnormal": true},
            "external_set_reply": ext_reply,
            "bit_before_external": bit_before,
            "bit_after_external": bit_after,
            "bit_unchanged": bit_before == bit_after,
            "recoverer_exit": recover_exit,
            "live_topmost_after_recover": after.topmost,
            "live_marker_after_recover": after.marker,
            "marker_expected": entry.marker,
            "note": "external true->true leaves the bit unchanged; no setter-identity inference; recoverer restores recorded false with 2 policy writes",
        });
        finish_topmost_leg(&pro, "topmost-exttrue.json", summary, 2)
    }

    // -- Storage legs ----------------------------------------------------------
    //
    // churn: bounded spawn/close loop with per-window join, tag
    // establishment and disappearance wait; reports counts, elapsed, HWND
    // distinct/repeats and tag absence on any HWND-value repeat. No
    // no-reuse bound is claimed.
    fn measure_storage_churn(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 storage-churn: --fixture-exe is required");
            return 2;
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 storage-churn: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 storage-churn: PMv2 not active");
            return 1;
        }
        let handoff = match run_setup(&fixture_exe, &run_dir, "stc_seed", &[]) {
            Ok(h) => h,
            Err(code) => return code,
        };
        let budget = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();
        let mut hwnds: Vec<u64> = Vec::new();
        let mut repeats: Vec<serde_json::Value> = Vec::new();
        let mut spawned: u32 = 0;
        let mut closed: u32 = 0;
        let mut limitation: Option<String> = None;
        let mut i: u32 = 0;
        while start.elapsed() < budget && spawned < 500 {
            let caller = format!("ch{i:05}");
            let spawn = serde_json::json!({"v":1,"request_id":format!("ch-spawn-{i}"),"nonce":handoff.nonce,"op":"spawn","windows":[{"id":caller}]})
                .to_string();
            let get = serde_json::json!({"v":1,"request_id":format!("ch-get-{i}"),"nonce":handoff.nonce,"op":"get","ids":[caller]})
                .to_string();
            let out = match send_mutations(
                &handoff,
                &fixture_exe,
                &handoff.socket,
                &handoff.nonce,
                &[spawn, get],
            ) {
                Ok(o) => o,
                Err(e) => {
                    limitation = Some(format!("spawn/get transport failed at {caller}: {e}"));
                    break;
                }
            };
            let snapshot = out
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter(|v| {
                    v.pointer("/request_id").and_then(|r| r.as_str())
                        == Some(format!("ch-get-{i}").as_str())
                })
                .find_map(|v| {
                    let ok_entry =
                        v.pointer("/result/windows/0/ok") == Some(&serde_json::Value::Bool(true));
                    if ok_entry {
                        v.pointer("/result/windows/0/snapshot").cloned()
                    } else {
                        None
                    }
                });
            let Some(snapshot) = snapshot else {
                limitation = Some(format!("no snapshot for {caller}"));
                break;
            };
            let title = snapshot.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let Some(hwnd) = resolve_hwnd(&snapshot, handoff.fixture_pid, title) else {
                limitation = Some(format!("HWND join failed for {caller}"));
                break;
            };
            // Join check: the exact native title must be the sentinel join.
            if parse_title_identity(title).is_none() {
                limitation = Some(format!("title shape mismatch for {caller}"));
                break;
            }
            if let Some(first) = hwnds.iter().position(|h| *h == hwnd) {
                let tag_absent = !matches!(NativeBackend::read_tag_inner(hwnd), LiveTag::Value(_));
                repeats.push(serde_json::json!({
                    "hwnd": hwnd,
                    "first_index": first,
                    "repeat_index": i,
                    "tag_absent_on_repeat": tag_absent,
                }));
                if !tag_absent {
                    limitation = Some(format!("stale tag present on repeated HWND {hwnd}"));
                    break;
                }
            }
            // Establishment-style tag for this disposable window (join-gated
            // native write; the tag dies with the window at close).
            let tag = generate_probe_tag();
            if valid_probe_tag(&tag)
                && let Ok(value) = u64::from_str_radix(&tag, 16)
            {
                let _ = tiler_windows::ffi::window::set_prop(hwnd, PROBE_PROP, value as usize);
            }
            hwnds.push(hwnd);
            spawned += 1;
            let close = serde_json::json!({"v":1,"request_id":format!("ch-close-{i}"),"nonce":handoff.nonce,"op":"close","ids":[caller]})
                .to_string();
            let list = serde_json::json!({"v":1,"request_id":format!("ch-list-{i}"),"nonce":handoff.nonce,"op":"list"})
                .to_string();
            let close_out = match send_mutations(
                &handoff,
                &fixture_exe,
                &handoff.socket,
                &handoff.nonce,
                &[close, list],
            ) {
                Ok(o) => o,
                Err(e) => {
                    limitation = Some(format!("close/list transport failed at {caller}: {e}"));
                    break;
                }
            };
            let close_ok = close_out
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter(|v| {
                    v.pointer("/request_id").and_then(|r| r.as_str())
                        == Some(format!("ch-close-{i}").as_str())
                })
                .any(|v| v.pointer("/result/windows/0/ok") == Some(&serde_json::Value::Bool(true)));
            if !close_ok || !wait_caller_gone(&handoff, &caller, 5_000) {
                limitation = Some(format!("caller {caller} did not disappear after close"));
                break;
            }
            closed += 1;
            i += 1;
        }
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let mut distinct = hwnds.clone();
        distinct.sort_unstable();
        distinct.dedup();
        let summary = serde_json::json!({
            "leg": "storage-churn",
            "spawned": spawned,
            "closed_confirmed": closed,
            "elapsed_ms": elapsed_ms,
            "hwnd_distinct": distinct.len(),
            "hwnd_repeats": repeats.len(),
            "repeat_events": repeats,
            "limitation": limitation,
            "note": "HWND values may repeat across genuinely new windows; identity comes from the HWND+PID/title/tag triple, never the numeric value alone",
        });
        let _ = std::fs::write(run_dir.join("storage-churn.json"), summary.to_string());
        if limitation.is_some() {
            eprintln!("measure123 storage-churn: limited; preserving");
            return 1;
        }
        if spawned == 0 {
            eprintln!("measure123 storage-churn: no windows churned; preserving");
            return 1;
        }
        teardown_best_effort(&fixture_exe, &run_dir, &handoff)
    }

    // stale: close the old caller, recreate the same id on the same server,
    // then invoke the ORIGINAL record with a fresh recoverer. The stale
    // record must refuse with zero writes either way (HWND churned or tag
    // mismatch on numeric HWND reuse).
    fn measure_storage_stale(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 storage-stale: --fixture-exe is required");
            return 2;
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 storage-stale: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 storage-stale: PMv2 not active");
            return 1;
        }
        let handoff = match run_setup(&fixture_exe, &run_dir, "sto_old", &[]) {
            Ok(h) => h,
            Err(code) => return code,
        };
        let probe_exe = match current_exe_or(1) {
            Ok(p) => p,
            Err(code) => return code,
        };
        let entry = match roster_entry(&handoff, "sto_old") {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 storage-stale: {e}; preserving");
                return 1;
            }
        };
        let old_hwnd = entry.hwnd;
        let old_marker = entry.marker;
        let record_name = "write-sto_old.json";
        let loss = match forced_writer_loss(
            &probe_exe,
            &run_dir,
            &run_raw,
            "sto_old",
            record_name,
            "sto_old.ready",
            30_000,
        ) {
            Ok(l) => l,
            Err(code) => return code,
        };
        let summary_writer = serde_json::json!({"pid": loss.pid, "exit_code": loss.exit_code, "exit_abnormal": true});
        let record_ok = match record_survives(&run_dir, record_name, old_marker) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 storage-stale record survival: {e}; preserving");
                return 1;
            }
        };
        // Close the old caller through the guarded protocol path.
        if close_one(&fixture_exe, &handoff, entry).is_err() {
            eprintln!("measure123 storage-stale: close of old caller refused; preserving");
            return 1;
        }
        if !wait_caller_gone(&handoff, "sto_old", 5_000) {
            eprintln!("measure123 storage-stale: old caller did not disappear; preserving");
            return 1;
        }
        // Recreate the same id on the SAME server process.
        let spawn = serde_json::json!({"v":1,"request_id":"stale-spawn","nonce":handoff.nonce,"op":"spawn","windows":[{"id":"sto_old"}]}).to_string();
        if send_mutation(
            &handoff,
            &fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &spawn,
        )
        .is_err()
        {
            eprintln!("measure123 storage-stale: recreate refused; preserving");
            return 1;
        }
        let snapshot = match fixture_snapshot(
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            "sto_old",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure123 storage-stale recreate snapshot: {e}; preserving");
                return 1;
            }
        };
        let new_title = snapshot.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let new_hwnd = match resolve_hwnd(&snapshot, handoff.fixture_pid, new_title) {
            Some(h) if new_title == entry.title => h,
            _ => {
                eprintln!("measure123 storage-stale: recreated join failed; preserving");
                return 1;
            }
        };
        let hwnd_reused = new_hwnd == old_hwnd;
        let old_tag_absent = !matches!(NativeBackend::read_tag_inner(new_hwnd), LiveTag::Value(_));
        // Invoke the ORIGINAL record with a fresh recoverer: must refuse
        // with zero writes.
        let status = match run_recoverer(
            &probe_exe,
            &run_dir,
            &run_raw,
            record_name,
            "recoverer-stale.log",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure123 storage-stale recoverer: {e}; preserving");
                return 1;
            }
        };
        let recover_exit = status.as_ref().and_then(|s| s.code());
        let no_recovered = !run_dir.join("recovered.json").exists()
            && !run_dir.join("recovered-partial.json").exists();
        let after_attempt = native_frame_of(new_hwnd);
        let record_still_ok = record_survives(&run_dir, record_name, old_marker).unwrap_or(false);
        let summary = serde_json::json!({
            "leg": "storage-stale",
            "writer": summary_writer,
            "old_hwnd": old_hwnd,
            "new_hwnd": new_hwnd,
            "hwnd_numeric_reuse": hwnd_reused,
            "old_tag_absent_on_new": old_tag_absent,
            "record_survives_with_preimage": record_ok,
            "recoverer_exit": recover_exit,
            "recovered_files_absent": no_recovered,
            "live_topmost_after_attempt": after_attempt.topmost,
            "live_marker_after_attempt": after_attempt.marker,
            "record_still_readable": record_still_ok,
            "note": "stale roster (old HWND) authorizes nothing; numeric HWND reuse without the tag also refuses",
        });
        let _ = std::fs::write(run_dir.join("storage-stale.json"), summary.to_string());
        if recover_exit != Some(5) || !no_recovered {
            eprintln!("measure123 storage-stale: recoverer did not refuse cleanly; preserving");
            return 1;
        }
        // Custom cleanup: the roster is stale by design (old HWND), so the
        // ordinary two-pass teardown would preserve. Close the recreated
        // caller by fresh join, then verify empty + shutdown.
        let close = serde_json::json!({"v":1,"request_id":"stale-close","nonce":handoff.nonce,"op":"close","ids":["sto_old"]}).to_string();
        let live_title = ffi_ext::window_text(new_hwnd).unwrap_or_default();
        if live_title != entry.title {
            eprintln!("measure123 storage-stale: recreated title moved; preserving");
            return 1;
        }
        if send_mutation(
            &handoff,
            &fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &close,
        )
        .is_err()
        {
            eprintln!("measure123 storage-stale: cleanup close refused; preserving");
            return 1;
        }
        if !wait_caller_gone(&handoff, "sto_old", 5_000) {
            eprintln!("measure123 storage-stale: recreated caller did not disappear; preserving");
            return 1;
        }
        let list =
            serde_json::json!({"v":1,"request_id":"stale-list","nonce":handoff.nonce,"op":"list"});
        let list_out = match send_one(
            &fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            &list.to_string(),
        ) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("measure123 storage-stale list: {e}; preserving");
                return 1;
            }
        };
        let remaining: usize = serde_json::from_str::<serde_json::Value>(&list_out)
            .ok()
            .and_then(|v| {
                v.pointer("/result/windows")
                    .and_then(|w| w.as_array())
                    .map(|a| a.len())
            })
            .or_else(|| {
                serde_json::from_str::<serde_json::Value>(&list_out)
                    .ok()
                    .and_then(|v| {
                        v.pointer("/result/ids")
                            .and_then(|w| w.as_array())
                            .map(|a| a.len())
                    })
            })
            .unwrap_or(usize::MAX);
        if remaining != 0 {
            eprintln!("measure123 storage-stale: roster not empty ({remaining}); no shutdown");
            return 1;
        }
        match shutdown_and_verify(&fixture_exe, &handoff, &list.to_string()) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    }

    // -- DXGI assessment (read-only; unimplemented) --------------------------------
    //
    // Source/tool capability check only: no fixture, no window, no display
    // change. Records the way out for a future dedicated probe.
    fn measure_dxgi(args: &[String]) -> i32 {
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 dxgi: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if std::fs::create_dir_all(&run_dir).is_err() {
            eprintln!("measure123 dxgi: create run dir");
            return 1;
        }
        // Repo-relative source facts (no machine paths recorded).
        let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let read_rel = |rel: &str| {
            std::fs::read_to_string(repo.join(rel)).map_err(|_| format!("cannot read {rel}"))
        };
        let win_rs = read_rel("crates/tiler-live-fixture/src/win.rs");
        let first_mode_pick = win_rs
            .as_deref()
            .map(|src| src.contains("monitor.video_modes().next()"))
            .unwrap_or(false);
        let tiler_cargo = read_rel("crates/tiler-windows/Cargo.toml");
        let windows_dep = tiler_cargo
            .as_deref()
            .map(|src| {
                src.lines()
                    .find(|l| l.trim_start().starts_with("windows "))
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_default();
        let lock = read_rel("Cargo.lock");
        let lock_text = lock.as_deref().unwrap_or("");
        let lock_windows = lock_version(lock_text, "windows");
        let lock_windows_sys_all = lock_versions(lock_text, "windows-sys");
        // Cached crate file presence (file name only, no local paths).
        let cache_hit = cached_crate_names()
            .iter()
            .any(|name| name.starts_with("windows-0.61"));
        // Listed SDK toolchain presence (booleans only, no paths).
        let has_cl = tool_present("cl.exe");
        let has_link = tool_present("link.exe");
        let summary = serde_json::json!({
            "leg": "dxgi",
            "status": "unimplemented",
            "fixture_first_mode_pick": first_mode_pick,
            "tiler_windows_dep": windows_dep,
            "lock_windows": lock_windows,
            "lock_windows_sys": lock_windows_sys_all,
            "cached_windows_crate": cache_hit,
            "toolchain_cl": has_cl,
            "toolchain_link": has_link,
            "way_out": [
                "new small Windows-only fixture example (never reuse the softbuffer fixture HWND)",
                "depend on the already-locked windows crate with Win32_Graphics_Dxgi + Win32_Graphics_Direct3D11 features (offline-safe)",
                "dedicated own window; D3D11 device + swapchain; SetFullscreenState(TRUE) on the CURRENT mode only (never ResizeTarget/arbitrary modes)",
                "read back GetFullscreenState + output GetDesc + current display mode before/after; always leave fullscreen and restore the captured display",
                "no app/game/anticheat windows; label FSO/independent-flip/composed present-path unverified (GetFullscreenState cannot prove them)",
            ],
        });
        let _ = std::fs::write(run_dir.join("dxgi.json"), summary.to_string());
        0
    }

    // -- K5: native-event oracle vs polled DWM + synthetic drag/resize -----
    //
    // PID-scoped WinEvent observation (fixture PID only, pumped on the
    // installing thread) against an independent continuous PMv2
    // EXTENDED_FRAME_BOUNDS poll. Callbacks carry delivery timestamps only;
    // geometry always comes from the poll stream (an event never sources a
    // frame). Synthetic caption drag + border resize use guarded absolute
    // SendInput with RAII button release and cursor restore.
    const K5_CALLER: &str = "k5a";
    const K5_EVENT_MOVESIZESTART: u32 = 0x000A;
    const K5_EVENT_MOVESIZEEND: u32 = 0x000B;
    const K5_EVENT_LOCATIONCHANGE: u32 = 0x800B;
    const K5_HTCAPTION: u32 = 2;
    const K5_HTRIGHT: u32 = 11;
    const K5_HTLEFT: u32 = 10;
    const K5_HTTOP: u32 = 12;
    const K5_HTTOPLEFT: u32 = 13;
    const K5_HTTOPRIGHT: u32 = 14;
    const K5_HTBOTTOM: u32 = 15;
    const K5_HTBOTTOMLEFT: u32 = 16;
    const K5_HTBOTTOMRIGHT: u32 = 17;
    const K5_WS_THICKFRAME: u32 = 0x0004_0000;
    const K5_POLL_MS: u64 = 5;
    const K5_BRACKET_TOL_MS: f64 = 150.0;

    #[derive(Debug, Clone, Copy, serde::Serialize)]
    struct K5Callback {
        dt_ms: f64,
        tick: u32,
        event: u32,
        hwnd: u64,
        id_object: i32,
        thread: u32,
        time: u32,
    }

    #[derive(Debug, Clone, Copy, serde::Serialize)]
    struct K5Sample {
        dt_ms: f64,
        tick: u32,
        rect: Option<[i32; 4]>,
    }

    struct K5Sink {
        t0: std::time::Instant,
        events: std::sync::Mutex<Vec<K5Callback>>,
    }

    impl tiler_windows::ffi::hooks::WinEventSink for K5Sink {
        fn on_win_event(&self, event: tiler_windows::ffi::hooks::WinEvent) {
            // Lightweight only: timestamp + delivery metadata. No DWM reads,
            // no logging, no locks held across a pump (single short lock).
            let dt_ms = self.t0.elapsed().as_micros() as f64 / 1000.0;
            let tick = tiler_windows::ffi::input::tick_count();
            if let Ok(mut guard) = self.events.lock() {
                guard.push(K5Callback {
                    dt_ms,
                    tick,
                    event: event.event,
                    hwnd: event.hwnd,
                    id_object: event.id_object,
                    thread: event.thread,
                    time: event.time,
                });
            }
        }
    }

    /// Adjacent-sample DWM transitions: both readable and different.
    /// Unreadable samples break segments (never interpolated).
    fn k5_transitions(samples: &[K5Sample]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for i in 1..samples.len() {
            match (samples[i - 1].rect, samples[i].rect) {
                (Some(prev), Some(curr)) if prev != curr => out.push((i - 1, i)),
                _ => {}
            }
        }
        out
    }

    /// Whether one WinEvent code is geometry-relevant for the oracle.
    fn k5_event_relevant(event: u32) -> bool {
        event == K5_EVENT_MOVESIZESTART
            || event == K5_EVENT_MOVESIZEEND
            || event == K5_EVENT_LOCATIONCHANGE
    }

    /// Bracket each transition in `[prev - tol, curr + tol]`: `covered` when
    /// a relevant callback lands inside, `ambiguous` when only irrelevant
    /// callbacks land inside, `uncovered` otherwise. Returns the three
    /// counts plus per-segment `(prev_idx, curr_idx, verdict)`.
    fn k5_bracket(
        samples: &[K5Sample],
        transitions: &[(usize, usize)],
        callbacks: &[K5Callback],
        tol_ms: f64,
    ) -> (usize, usize, usize, Vec<(usize, usize, &'static str)>) {
        let mut covered = 0;
        let mut uncovered = 0;
        let mut ambiguous = 0;
        let mut detail = Vec::new();
        for (prev, curr) in transitions {
            let lo = samples[*prev].dt_ms - tol_ms;
            let hi = samples[*curr].dt_ms + tol_ms;
            let mut relevant = false;
            let mut any = false;
            for cb in callbacks {
                if cb.dt_ms >= lo && cb.dt_ms <= hi {
                    any = true;
                    if k5_event_relevant(cb.event) {
                        relevant = true;
                        break;
                    }
                }
            }
            let verdict = if relevant {
                covered += 1;
                "covered"
            } else if any {
                ambiguous += 1;
                "ambiguous"
            } else {
                uncovered += 1;
                "uncovered"
            };
            detail.push((*prev, *curr, verdict));
        }
        (covered, uncovered, ambiguous, detail)
    }

    /// Delivery-to-read lag per callback: next poll sample at/after the
    /// callback minus the callback time. Callbacks past the last sample
    /// are dropped (no extrapolation).
    fn k5_next_sample_lags(callbacks: &[K5Callback], samples: &[K5Sample]) -> Vec<f64> {
        let mut out = Vec::new();
        for cb in callbacks {
            for sample in samples {
                if sample.dt_ms >= cb.dt_ms {
                    out.push(sample.dt_ms - cb.dt_ms);
                    break;
                }
            }
        }
        out
    }

    fn k5_stats(values: &[f64]) -> serde_json::Value {
        if values.is_empty() {
            return serde_json::json!({"n": 0});
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();
        let p50 = sorted[n / 2];
        serde_json::json!({
            "n": n,
            "min": sorted[0],
            "p50": p50,
            "max": sorted[n - 1],
        })
    }

    /// Single pump + DWM sample step on the installing thread (delivers
    /// queued hook callbacks, then reads an independent DWM frame).
    fn k5_pump_sample(stream: &mut Vec<K5Sample>, t0: std::time::Instant, hwnd: u64) {
        tiler_windows::ffi::pump::drain_messages();
        let dt_ms = t0.elapsed().as_micros() as f64 / 1000.0;
        let rect = tiler_windows::ffi::extended_frame_bounds(hwnd)
            .map(|r| [r.left, r.top, r.right, r.bottom]);
        stream.push(K5Sample {
            dt_ms,
            tick: tiler_windows::ffi::input::tick_count(),
            rect,
        });
    }

    /// Bounded poll for `ms`: pump + sample every `K5_POLL_MS`.
    fn k5_poll_for(stream: &mut Vec<K5Sample>, t0: std::time::Instant, hwnd: u64, ms: u64) {
        let end = t0.elapsed().as_millis() as u64 + ms;
        loop {
            k5_pump_sample(stream, t0, hwnd);
            let now = t0.elapsed().as_millis() as u64;
            if now >= end {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(
                (end - now).min(K5_POLL_MS),
            ));
        }
    }

    /// Bounded sampled settle: pump + sample until outer+DWM repeat once.
    /// Returns whether the frame settled inside the budget.
    type K5FrameKey = (Option<[i32; 4]>, Option<[i32; 4]>);

    fn k5_settle(
        stream: &mut Vec<K5Sample>,
        t0: std::time::Instant,
        hwnd: u64,
        timeout_ms: u64,
    ) -> bool {
        let start = t0.elapsed().as_millis() as u64;
        let mut prev: Option<K5FrameKey> = None;
        loop {
            k5_pump_sample(stream, t0, hwnd);
            let frame = native_frame_of(hwnd);
            let key = (frame.outer_px, frame.dwm_px);
            if prev == Some(key) {
                return true;
            }
            prev = Some(key);
            if t0.elapsed().as_millis() as u64 - start >= timeout_ms {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(K5_POLL_MS));
        }
    }

    /// Input-ownership re-verification immediately before each mouse batch:
    /// our window holds the foreground and the target resolves to a
    /// fixture-owned window. Any deviation refuses (zero input).
    fn k5_input_guards(handoff: &Handoff, hwnd: u64, x: i32, y: i32) -> Result<(), String> {
        if tiler_windows::ffi::window::foreground_window() != hwnd {
            return Err("foreground is not the fixture window; no input".to_string());
        }
        let at = tiler_windows::ffi::window::window_at_point(x, y);
        if at == 0 {
            return Err("no window at target; no input".to_string());
        }
        if tiler_windows::ffi::window::window_pid(at) != handoff.fixture_pid {
            return Err("target is not fixture-owned; no input".to_string());
        }
        Ok(())
    }

    /// Absolute cursor move via `SendInput` (virtual-desktop normalized).
    /// Returns the accepted-event count (0 = refused).
    fn k5_move_abs(x: i32, y: i32) -> Result<u32, String> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK,
        };
        let (vx, vy, vw, vh) =
            ffi_ext::virtual_desktop_px().ok_or_else(|| "no virtual desktop".to_string())?;
        if vw <= 1 || vh <= 1 {
            return Err("degenerate virtual desktop".to_string());
        }
        let nx = ((x as i64 - vx as i64) * 65535 / (vw as i64 - 1)).clamp(0, 65535) as i32;
        let ny = ((y as i64 - vy as i64) * 65535 / (vh as i64 - 1)).clamp(0, 65535) as i32;
        Ok(ffi_ext::send_mouse_input(
            nx,
            ny,
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        ))
    }

    /// RAII left-button release: armed on LEFTDOWN, disarmed after the
    /// matching LEFTUP. Drop re-releases, so the button is never held when
    /// the leg returns imperfectly.
    struct K5LeftUp {
        armed: bool,
    }

    impl Drop for K5LeftUp {
        fn drop(&mut self) {
            if self.armed {
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP;
                ffi_ext::send_mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP);
            }
        }
    }

    /// Caption hit point (verified `HTCAPTION`), trying a few caption rows.
    fn k5_caption_point(hwnd: u64) -> Option<(i32, i32)> {
        let r = tiler_windows::ffi::extended_frame_bounds(hwnd)?;
        let cx = (r.left + r.right) / 2;
        for dy in [8, 12, 16, 20, 24] {
            let (x, y) = (cx, r.top + dy);
            if tiler_windows::ffi::window::hit_test_at(hwnd, x, y) == Some(K5_HTCAPTION) {
                return Some((x, y));
            }
        }
        None
    }

    /// Whether one `WM_NCHITTEST` code is a resizable border/corner
    /// (`HTLEFT`..`HTBOTTOMRIGHT`, 10..=17). Pure classifier for the
    /// `k5-resize` survey; the live scan verifies codes with `hit_test_at`.
    #[must_use]
    pub fn k5_ht_is_resize(code: u32) -> bool {
        (K5_HTLEFT..=K5_HTBOTTOMRIGHT).contains(&code)
    }

    /// Whether a style word carries `WS_THICKFRAME` (resizable frame).
    /// Pure predicate; the live leg joins it with iconic/zoomed readbacks.
    #[must_use]
    pub fn k5_style_resizable(style: u32) -> bool {
        style & K5_WS_THICKFRAME != 0
    }

    /// Outer-rect border/corner scan (verified resize `HT*`), trying the
    /// bottom-right corner first (both dims move), then right and bottom
    /// edges. Uses `GetWindowRect` outer geometry: the invisible resize
    /// border lives outside `EXTENDED_FRAME_BOUNDS`, so DWM-rect insets
    /// land interior and never report `HTRIGHT` (prior K5 stimulus-4
    /// failure mode). Returns the point plus the verified hit code.
    fn k5_resize_point(hwnd: u64) -> Option<(i32, i32, u32)> {
        let outer = tiler_windows::ffi::window::window_rect(hwnd)?;
        let (l, t, r, b) = (outer.left, outer.top, outer.right, outer.bottom);
        if r - l < 64 || b - t < 64 {
            return None;
        }
        let mid_x = (l + r) / 2;
        let mid_y = (t + b) / 2;
        let mut candidates: Vec<(i32, i32)> = Vec::new();
        for inset in [1, 2, 3, 4, 5, 6, 7, 8] {
            candidates.push((r - inset, b - inset)); // corner first
        }
        for inset in [1, 2, 3, 4, 5, 6, 7, 8] {
            candidates.push((r - inset, mid_y)); // right edge
        }
        for inset in [1, 2, 3, 4, 5, 6, 7, 8] {
            candidates.push((mid_x, b - inset)); // bottom edge
        }
        for inset in [1, 2, 3, 4, 5, 6, 7, 8] {
            candidates.push((l + inset, b - inset)); // bottom-left
            candidates.push((r - inset, t + inset)); // top-right
        }
        for (x, y) in candidates {
            if let Some(code) = tiler_windows::ffi::window::hit_test_at(hwnd, x, y)
                && k5_ht_is_resize(code)
            {
                return Some((x, y, code));
            }
        }
        None
    }

    /// Bounded hit-test survey over the same outer-rect candidates (at most
    /// one `SendMessageTimeout` per point): `(x, y, ht-code-or-0-when-sent-
    /// but-unmapped, readable)`. `None` code reads as 0 with `readable`
    /// false. Evidence for why a resize point was/was-not chosen.
    fn k5_border_survey(hwnd: u64) -> Vec<(i32, i32, u32, bool)> {
        let Some(outer) = tiler_windows::ffi::window::window_rect(hwnd) else {
            return Vec::new();
        };
        let (l, t, r, b) = (outer.left, outer.top, outer.right, outer.bottom);
        let mid_x = (l + r) / 2;
        let mid_y = (t + b) / 2;
        let mut points = Vec::new();
        for inset in [2, 4, 6] {
            points.push((r - inset, b - inset));
            points.push((r - inset, mid_y));
            points.push((mid_x, b - inset));
        }
        let mut out = Vec::new();
        for (x, y) in points {
            match tiler_windows::ffi::window::hit_test_at(hwnd, x, y) {
                Some(code) => out.push((x, y, code, true)),
                None => out.push((x, y, 0, false)),
            }
        }
        out
    }

    /// Right-border hit point (verified `HTRIGHT`), trying a few insets.
    fn k5_right_border_point(hwnd: u64) -> Option<(i32, i32)> {
        let r = tiler_windows::ffi::extended_frame_bounds(hwnd)?;
        let y = (r.top + r.bottom) / 2;
        for dx in [2, 4, 6, 8, 10, 12] {
            let (x, yy) = (r.right - dx, y);
            if tiler_windows::ffi::window::hit_test_at(hwnd, x, yy) == Some(K5_HTRIGHT) {
                return Some((x, yy));
            }
        }
        None
    }

    fn k5_rect_of(hwnd: u64) -> Option<[i32; 4]> {
        tiler_windows::ffi::extended_frame_bounds(hwnd).map(|r| [r.left, r.top, r.right, r.bottom])
    }

    fn measure_k5(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 k5: --fixture-exe is required");
            return 2;
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 k5: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 k5: PMv2 not active");
            return 1;
        }
        let handoff = match run_setup(&fixture_exe, &run_dir, K5_CALLER, &[]) {
            Ok(h) => h,
            Err(code) => return code,
        };
        let entry = match roster_entry(&handoff, K5_CALLER) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 k5: {e}; preserving");
                return 1;
            }
        };
        let snapshot0 = match fixture_snapshot(
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            K5_CALLER,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure123 k5 snapshot: {e}; preserving");
                return 1;
            }
        };
        let title0 = snapshot0
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let hwnd = match resolve_hwnd(&snapshot0, handoff.fixture_pid, &title0) {
            Some(h) => h,
            None => {
                eprintln!("measure123 k5: HWND join failed; preserving");
                return 1;
            }
        };
        if hwnd != entry.hwnd {
            eprintln!("measure123 k5: HWND churned; preserving");
            return 1;
        }
        if rejoin_live(&handoff, entry, hwnd).is_err() {
            eprintln!("measure123 k5: rejoin refused; preserving");
            return 1;
        }
        // Known start geometry on the current output.
        if app_set(
            &handoff,
            K5_CALLER,
            serde_json::json!({"size_px": [640, 480]}),
        )
        .is_err()
        {
            eprintln!("measure123 k5: baseline size refused; preserving");
            return 1;
        }
        // Foreground via the fixture protocol first (no synthetic input),
        // native SetForegroundWindow once as fallback; synthetic legs are
        // skipped unless the foreground verifies on our window.
        let _ = app_set(&handoff, K5_CALLER, serde_json::json!({"focus": true}));
        let mut focus_method = "socket";
        if tiler_windows::ffi::window::foreground_window() != hwnd {
            let _ = tiler_windows::ffi::window::set_foreground_window(hwnd);
            std::thread::sleep(std::time::Duration::from_millis(300));
            focus_method = "native";
        }
        let foreground_ok = tiler_windows::ffi::window::foreground_window() == hwnd;

        // PID-scoped observation only: fixture PID, pumped on this thread.
        let t0 = std::time::Instant::now();
        let sink = std::sync::Arc::new(K5Sink {
            t0,
            events: std::sync::Mutex::new(Vec::new()),
        });
        tiler_windows::ffi::hooks::set_win_event_sink(sink.clone());
        let hook_move = tiler_windows::ffi::hooks::install_win_event_hook_for_pid(
            K5_EVENT_MOVESIZESTART,
            K5_EVENT_MOVESIZEEND,
            handoff.fixture_pid,
        );
        let hook_loc = tiler_windows::ffi::hooks::install_win_event_hook_for_pid(
            0x8000,
            K5_EVENT_LOCATIONCHANGE,
            handoff.fixture_pid,
        );
        let hooks_active = hook_move.is_some() || hook_loc.is_some();
        if !hooks_active {
            eprintln!("measure123 k5: hooks refused; continuing poll-only");
        }
        let cursor0 = tiler_windows::ffi::input::cursor_pos();

        let mut stream: Vec<K5Sample> = Vec::new();
        k5_poll_for(&mut stream, t0, hwnd, 400);
        let frame0 = native_frame_of(hwnd);

        // -- Stimulus 1: programmatic move (+80,+60 outer, clamped) ----
        let stim1_start = t0.elapsed().as_micros() as f64 / 1000.0;
        let before1 = k5_rect_of(hwnd);
        let outer_now = tiler_windows::ffi::window::window_rect(hwnd);
        let mut stim1_detail = serde_json::json!({"skipped": false});
        if let Some(r) = outer_now {
            let (nx, ny) = (r.left + 80, r.top + 60);
            match app_set(
                &handoff,
                K5_CALLER,
                serde_json::json!({"position_outer_px": [nx, ny]}),
            ) {
                Ok(_) => {
                    let settled = k5_settle(&mut stream, t0, hwnd, 8_000);
                    stim1_detail["settled"] = serde_json::json!(settled);
                }
                Err(e) => stim1_detail["error"] = serde_json::json!(e),
            }
        } else {
            stim1_detail["skipped"] = serde_json::json!("outer unreadable");
        }
        let after1 = k5_rect_of(hwnd);
        let stim1_end = t0.elapsed().as_micros() as f64 / 1000.0;

        // -- Stimulus 2: programmatic resize to [760, 560] ---------------
        let stim2_start = t0.elapsed().as_micros() as f64 / 1000.0;
        let before2 = k5_rect_of(hwnd);
        let mut stim2_detail = serde_json::json!({"skipped": false});
        match app_set(
            &handoff,
            K5_CALLER,
            serde_json::json!({"size_px": [760, 560]}),
        ) {
            Ok(_) => {
                let settled = k5_settle(&mut stream, t0, hwnd, 8_000);
                stim2_detail["settled"] = serde_json::json!(settled);
            }
            Err(e) => stim2_detail["error"] = serde_json::json!(e),
        }
        let after2 = k5_rect_of(hwnd);
        let stim2_end = t0.elapsed().as_micros() as f64 / 1000.0;

        // -- Stimulus 3: synthetic caption drag (+140,+70, ~0.7 s) -------
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        };
        let stim3_start = t0.elapsed().as_micros() as f64 / 1000.0;
        let before3 = k5_rect_of(hwnd);
        let mut stim3_detail = serde_json::json!({});
        let mut hold = K5LeftUp { armed: false };
        if !foreground_ok {
            stim3_detail["skipped"] = serde_json::json!("foreground unverified; zero input");
        } else if let Some((px, py)) = k5_caption_point(hwnd) {
            let (tx, ty) = (px + 140, py + 70);
            let guards = k5_input_guards(&handoff, hwnd, px, py)
                .and(k5_input_guards(&handoff, hwnd, tx, ty));
            match guards {
                Err(e) => stim3_detail["skipped"] = serde_json::json!(e),
                Ok(()) => {
                    let warped = tiler_windows::ffi::input::set_cursor_pos(px, py);
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    let back = tiler_windows::ffi::input::cursor_pos();
                    let arrived =
                        back.is_some_and(|(x, y)| (x - px).abs() <= 2 && (y - py).abs() <= 2);
                    if !warped || !arrived {
                        stim3_detail["skipped"] =
                            serde_json::json!("cursor warp unverified; zero button input");
                    } else {
                        let down_n = ffi_ext::send_mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN);
                        stim3_detail["down_inserted"] = serde_json::json!(down_n);
                        if down_n == 0 {
                            stim3_detail["skipped"] =
                                serde_json::json!("LEFTDOWN refused; zero drag");
                        } else {
                            hold.armed = true;
                            let mut move_n: u32 = 0;
                            for step in 1..=14u32 {
                                let x = px + 140 * step as i32 / 14;
                                let y = py + 70 * step as i32 / 14;
                                move_n += k5_move_abs(x, y).unwrap_or(0);
                                k5_pump_sample(&mut stream, t0, hwnd);
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }
                            stim3_detail["move_inserted"] = serde_json::json!(move_n);
                            let up_n = ffi_ext::send_mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP);
                            hold.armed = false;
                            stim3_detail["up_inserted"] = serde_json::json!(up_n);
                        }
                    }
                }
            }
        } else {
            stim3_detail["skipped"] = serde_json::json!("no verified caption point; zero input");
        }
        let after3 = k5_rect_of(hwnd);
        let stim3_end = t0.elapsed().as_micros() as f64 / 1000.0;

        // -- Stimulus 4: synthetic right-border resize (+120 w, ~0.7 s) ---
        let stim4_start = t0.elapsed().as_micros() as f64 / 1000.0;
        let before4 = k5_rect_of(hwnd);
        let mut stim4_detail = serde_json::json!({});
        if tiler_windows::ffi::window::foreground_window() != hwnd {
            stim4_detail["skipped"] = serde_json::json!("foreground moved; zero input");
        } else if let Some((px, py)) = k5_right_border_point(hwnd) {
            let (tx, ty) = (px + 120, py);
            let guards = k5_input_guards(&handoff, hwnd, px, py)
                .and(k5_input_guards(&handoff, hwnd, tx, ty));
            match guards {
                Err(e) => stim4_detail["skipped"] = serde_json::json!(e),
                Ok(()) => {
                    let warped = tiler_windows::ffi::input::set_cursor_pos(px, py);
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    let back = tiler_windows::ffi::input::cursor_pos();
                    let arrived =
                        back.is_some_and(|(x, y)| (x - px).abs() <= 2 && (y - py).abs() <= 2);
                    if !warped || !arrived {
                        stim4_detail["skipped"] =
                            serde_json::json!("cursor warp unverified; zero button input");
                    } else {
                        let down_n = ffi_ext::send_mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN);
                        stim4_detail["down_inserted"] = serde_json::json!(down_n);
                        if down_n == 0 {
                            stim4_detail["skipped"] =
                                serde_json::json!("LEFTDOWN refused; zero drag");
                        } else {
                            hold.armed = true;
                            let mut move_n: u32 = 0;
                            for step in 1..=14u32 {
                                let x = px + 120 * step as i32 / 14;
                                move_n += k5_move_abs(x, py).unwrap_or(0);
                                k5_pump_sample(&mut stream, t0, hwnd);
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }
                            stim4_detail["move_inserted"] = serde_json::json!(move_n);
                            let up_n = ffi_ext::send_mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP);
                            hold.armed = false;
                            stim4_detail["up_inserted"] = serde_json::json!(up_n);
                        }
                    }
                }
            }
        } else {
            stim4_detail["skipped"] = serde_json::json!("no verified border point; zero input");
        }
        let after4 = k5_rect_of(hwnd);
        let stim4_end = t0.elapsed().as_micros() as f64 / 1000.0;

        k5_poll_for(&mut stream, t0, hwnd, 400);
        let frame_end = native_frame_of(hwnd);
        drop(hold);
        drop(hook_move);
        drop(hook_loc);
        tiler_windows::ffi::pump::drain_messages();

        // Cursor restore to the captured point (user idle); warp only.
        let cursor_restored = match cursor0 {
            Some((x, y)) => {
                let ok = tiler_windows::ffi::input::set_cursor_pos(x, y);
                std::thread::sleep(std::time::Duration::from_millis(80));
                ok && tiler_windows::ffi::input::cursor_pos()
                    .is_some_and(|(cx, cy)| (cx - x).abs() <= 2 && (cy - y).abs() <= 2)
            }
            None => false,
        };

        // Oracle evaluation (delivery timestamps only; geometry is poll-only).
        let callbacks: Vec<K5Callback> = sink.events.lock().map(|g| g.clone()).unwrap_or_default();
        let relevant: Vec<&K5Callback> = callbacks
            .iter()
            .filter(|cb| k5_event_relevant(cb.event) && cb.hwnd == hwnd)
            .collect();
        let transitions = k5_transitions(&stream);
        let (covered, uncovered, ambiguous, detail) =
            k5_bracket(&stream, &transitions, &callbacks, K5_BRACKET_TOL_MS);
        let relevant_owned: Vec<K5Callback> = relevant.into_iter().copied().collect();
        let lags = k5_next_sample_lags(&relevant_owned, &stream);
        let period_ms = if stream.len() > 1 {
            (stream.last().map(|s| s.dt_ms).unwrap_or(0.0)
                - stream.first().map(|s| s.dt_ms).unwrap_or(0.0))
                / (stream.len() - 1) as f64
        } else {
            0.0
        };
        let unreadable = stream.iter().filter(|s| s.rect.is_none()).count();

        let stim_rows = serde_json::json!([
            {"name": "programmatic_move", "start_ms": stim1_start, "end_ms": stim1_end,
             "before": before1, "after": after1,
             "moved": matches!((before1, after1), (Some(a), Some(b)) if a != b),
             "detail": stim1_detail},
            {"name": "programmatic_resize", "start_ms": stim2_start, "end_ms": stim2_end,
             "before": before2, "after": after2,
             "moved": matches!((before2, after2), (Some(a), Some(b)) if a != b),
             "detail": stim2_detail},
            {"name": "synthetic_caption_drag", "start_ms": stim3_start, "end_ms": stim3_end,
             "before": before3, "after": after3,
             "moved": matches!((before3, after3), (Some(a), Some(b)) if a != b),
             "detail": stim3_detail},
            {"name": "synthetic_border_resize", "start_ms": stim4_start, "end_ms": stim4_end,
             "before": before4, "after": after4,
             "moved": matches!((before4, after4), (Some(a), Some(b)) if a != b),
             "detail": stim4_detail},
        ]);
        // Per-stimulus brackets over expanded windows [start-100, end+400].
        let mut per_stimulus = Vec::new();
        for row in [
            ("programmatic_move", stim1_start, stim1_end),
            ("programmatic_resize", stim2_start, stim2_end),
            ("synthetic_caption_drag", stim3_start, stim3_end),
            ("synthetic_border_resize", stim4_start, stim4_end),
        ] {
            let (name, s, e) = row;
            let seg: Vec<(usize, usize)> = transitions
                .iter()
                .copied()
                .filter(|(_, c)| stream[*c].dt_ms >= s - 100.0 && stream[*c].dt_ms <= e + 400.0)
                .collect();
            let window_cbs: Vec<K5Callback> = callbacks
                .iter()
                .copied()
                .filter(|cb| {
                    cb.dt_ms >= s - 100.0 - K5_BRACKET_TOL_MS
                        && cb.dt_ms <= e + 400.0 + K5_BRACKET_TOL_MS
                })
                .collect();
            let (c, u, a, _) = k5_bracket(&stream, &seg, &window_cbs, K5_BRACKET_TOL_MS);
            let rel_n = window_cbs
                .iter()
                .filter(|cb| k5_event_relevant(cb.event))
                .count();
            per_stimulus.push(serde_json::json!({
                "name": name,
                "transitions": seg.len(),
                "covered": c, "uncovered": u, "ambiguous": a,
                "callbacks_in_window": window_cbs.len(),
                "relevant_in_window": rel_n,
            }));
        }

        let summary = serde_json::json!({
            "leg": "k5",
            "pmv2": true,
            "hwnd": hwnd,
            "fixture_pid": handoff.fixture_pid,
            "focus_method": focus_method,
            "foreground_ok": foreground_ok,
            "hooks_active": hooks_active,
            "poll": {"samples": stream.len(), "period_ms_mean": period_ms,
                     "unreadable": unreadable, "poll_ms": K5_POLL_MS},
            "callbacks": {"total": callbacks.len(),
                          "relevant_own_window": relevant_owned.len(),
                          "note": "delivery timestamps only; geometry is poll-sourced, never event-sourced"},
            "oracle": {"transitions": transitions.len(), "covered": covered,
                       "uncovered": uncovered, "ambiguous": ambiguous,
                       "bracket_tol_ms": K5_BRACKET_TOL_MS,
                       "segments": detail.iter().map(|(p, c, v)| serde_json::json!([p, c, v])).collect::<Vec<_>>()},
            "relevant_callback_to_poll_lag_ms": k5_stats(&lags),
            "tick_note": "TickCount is coarse (~15.6 ms); monotonic Instant is the join domain",
            "aliasing_note": "5 ms poll vs ~700 ms drags: transitions may coalesce; uncovered counts bound the oracle, not the compositor",
            "stimuli": stim_rows,
            "per_stimulus": per_stimulus,
            "frame_first": frame0,
            "frame_end": frame_end,
            "cursor_restored": cursor_restored,
            "button_held_at_exit": false,
        });
        let _ = std::fs::write(run_dir.join("k5.json"), summary.to_string());
        let _ = std::fs::write(
            run_dir.join("k5_polls.json"),
            serde_json::json!(stream).to_string(),
        );
        let _ = std::fs::write(
            run_dir.join("k5_callbacks.json"),
            serde_json::json!(callbacks).to_string(),
        );
        teardown_best_effort(&fixture_exe, &run_dir, &handoff)
    }

    /// K5-resize retry leg (caller `k5r`): resize-only followup to the prior
    /// K5 run whose stimulus 4 scanned DWM-rect insets and found no
    /// `HTRIGHT` (zero resize input). Uses the outer-rect corner/edge scan
    /// (`k5_resize_point`), verifies thickframe + windowed (not iconic /
    /// zoomed) + resizable, then one bounded synthetic down/move/up with
    /// native outer+DWM readback. Reuses the K5 hooks/poll/oracle/cursor
    /// machinery; leaves the existing `k5` leg untouched.
    fn measure_k5_resize(args: &[String]) -> i32 {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        };
        const CALLER: &str = "k5r";
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 k5-resize: --fixture-exe is required");
            return 2;
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 k5-resize: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 k5-resize: PMv2 not active");
            return 1;
        }
        let handoff = match run_setup(&fixture_exe, &run_dir, CALLER, &[]) {
            Ok(h) => h,
            Err(code) => return code,
        };
        let entry = match roster_entry(&handoff, CALLER) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 k5-resize: {e}; preserving");
                return 1;
            }
        };
        let snapshot0 = match fixture_snapshot(
            &handoff.fixture_exe,
            &handoff.socket,
            &handoff.nonce,
            CALLER,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure123 k5-resize snapshot: {e}; preserving");
                return 1;
            }
        };
        let title0 = snapshot0
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let hwnd = match resolve_hwnd(&snapshot0, handoff.fixture_pid, &title0) {
            Some(h) => h,
            None => {
                eprintln!("measure123 k5-resize: HWND join failed; preserving");
                return 1;
            }
        };
        if hwnd != entry.hwnd {
            eprintln!("measure123 k5-resize: HWND churned; preserving");
            return 1;
        }
        if rejoin_live(&handoff, entry, hwnd).is_err() {
            eprintln!("measure123 k5-resize: rejoin refused; preserving");
            return 1;
        }
        if app_set(&handoff, CALLER, serde_json::json!({"size_px": [640, 480]})).is_err() {
            eprintln!("measure123 k5-resize: baseline size refused; preserving");
            return 1;
        }
        let _ = app_set(&handoff, CALLER, serde_json::json!({"focus": true}));
        let mut focus_method = "socket";
        if tiler_windows::ffi::window::foreground_window() != hwnd {
            let _ = tiler_windows::ffi::window::set_foreground_window(hwnd);
            std::thread::sleep(std::time::Duration::from_millis(300));
            focus_method = "native";
        }
        let foreground_ok = tiler_windows::ffi::window::foreground_window() == hwnd;

        let t0 = std::time::Instant::now();
        let sink = std::sync::Arc::new(K5Sink {
            t0,
            events: std::sync::Mutex::new(Vec::new()),
        });
        tiler_windows::ffi::hooks::set_win_event_sink(sink.clone());
        let hook_move = tiler_windows::ffi::hooks::install_win_event_hook_for_pid(
            K5_EVENT_MOVESIZESTART,
            K5_EVENT_MOVESIZEEND,
            handoff.fixture_pid,
        );
        let hook_loc = tiler_windows::ffi::hooks::install_win_event_hook_for_pid(
            0x8000,
            K5_EVENT_LOCATIONCHANGE,
            handoff.fixture_pid,
        );
        let hooks_active = hook_move.is_some() || hook_loc.is_some();
        if !hooks_active {
            eprintln!("measure123 k5-resize: hooks refused; continuing poll-only");
        }
        let cursor0 = tiler_windows::ffi::input::cursor_pos();

        let mut stream: Vec<K5Sample> = Vec::new();
        k5_poll_for(&mut stream, t0, hwnd, 400);
        let frame0 = native_frame_of(hwnd);

        // Style/resizable verification (native readback, not setter echo).
        let style = tiler_windows::ffi::window::window_style_checked(hwnd);
        let iconic = tiler_windows::ffi::window::is_iconic(hwnd);
        let zoomed = tiler_windows::ffi::window::is_zoomed(hwnd);
        let style_detail = serde_json::json!({
            "style": style,
            "thickframe": style.map(k5_style_resizable),
            "iconic": iconic,
            "zoomed": zoomed,
            "windowed_resizable": style.is_some_and(k5_style_resizable) && !iconic && !zoomed,
        });

        // Outer-rect hit survey + chosen resize point.
        let survey = k5_border_survey(hwnd);
        let survey_json: Vec<serde_json::Value> = survey
            .iter()
            .map(|(x, y, code, readable)| {
                serde_json::json!({"x": x, "y": y, "ht": code, "readable": readable,
                    "resize": k5_ht_is_resize(*code) && *readable})
            })
            .collect();
        let picked = k5_resize_point(hwnd);
        let outer_now = tiler_windows::ffi::window::window_rect(hwnd)
            .map(|r| [r.left, r.top, r.right, r.bottom]);

        // -- Single bounded synthetic resize --------------------------------
        let stim_start = t0.elapsed().as_micros() as f64 / 1000.0;
        let before_outer = outer_now;
        let before_dwm = k5_rect_of(hwnd);
        let mut detail = serde_json::json!({
            "style": style_detail,
            "survey": survey_json,
            "picked": picked.map(|(x, y, c)| serde_json::json!({"x": x, "y": y, "ht": c})),
            "outer_now": outer_now,
        });
        let mut hold = K5LeftUp { armed: false };
        if !foreground_ok {
            detail["skipped"] = serde_json::json!("foreground unverified; zero input");
        } else if style.is_some_and(|s| !k5_style_resizable(s)) || iconic || zoomed {
            detail["skipped"] =
                serde_json::json!("not windowed-resizable (thickframe/iconic/zoomed); zero input");
        } else if let Some((px, py, ht)) = picked {
            // Drag delta follows the grabbed edge, always INWARD (negative)
            // so the pre-verified target stays over the fixture window:
            // the ownership guard requires `WindowFromPoint` at BOTH ends
            // to resolve to the fixture PID, and an outward target lands on
            // whatever is behind (prior run: refused, zero input).
            let (dx, dy) = if ht == K5_HTBOTTOMRIGHT
                || ht == K5_HTBOTTOMLEFT
                || ht == K5_HTTOPRIGHT
                || ht == K5_HTTOPLEFT
            {
                (-160, -100)
            } else if ht == K5_HTBOTTOM || ht == K5_HTTOP {
                (0, -100)
            } else {
                (-160, 0)
            };
            let (tx, ty) = (px + dx, py + dy);
            match k5_input_guards(&handoff, hwnd, px, py)
                .and(k5_input_guards(&handoff, hwnd, tx, ty))
            {
                Err(e) => detail["skipped"] = serde_json::json!(e),
                Ok(()) => {
                    let warped = tiler_windows::ffi::input::set_cursor_pos(px, py);
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    let back = tiler_windows::ffi::input::cursor_pos();
                    let arrived =
                        back.is_some_and(|(x, y)| (x - px).abs() <= 2 && (y - py).abs() <= 2);
                    if !warped || !arrived {
                        detail["skipped"] =
                            serde_json::json!("cursor warp unverified; zero button input");
                    } else {
                        let down_n = ffi_ext::send_mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN);
                        detail["down_inserted"] = serde_json::json!(down_n);
                        if down_n == 0 {
                            detail["skipped"] = serde_json::json!("LEFTDOWN refused; zero drag");
                        } else {
                            hold.armed = true;
                            let mut move_n: u32 = 0;
                            for step in 1..=20u32 {
                                let x = px + dx * step as i32 / 20;
                                let y = py + dy * step as i32 / 20;
                                move_n += k5_move_abs(x, y).unwrap_or(0);
                                k5_pump_sample(&mut stream, t0, hwnd);
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }
                            detail["move_inserted"] = serde_json::json!(move_n);
                            detail["delta"] = serde_json::json!([dx, dy]);
                            let up_n = ffi_ext::send_mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP);
                            hold.armed = false;
                            detail["up_inserted"] = serde_json::json!(up_n);
                            let settled = k5_settle(&mut stream, t0, hwnd, 8_000);
                            detail["settled"] = serde_json::json!(settled);
                        }
                    }
                }
            }
        } else {
            detail["skipped"] =
                serde_json::json!("no verified outer-rect resize point; zero input");
        }
        let after_outer = tiler_windows::ffi::window::window_rect(hwnd)
            .map(|r| [r.left, r.top, r.right, r.bottom]);
        let after_dwm = k5_rect_of(hwnd);
        let stim_end = t0.elapsed().as_micros() as f64 / 1000.0;

        k5_poll_for(&mut stream, t0, hwnd, 400);
        let frame_end = native_frame_of(hwnd);
        drop(hold);
        drop(hook_move);
        drop(hook_loc);
        tiler_windows::ffi::pump::drain_messages();

        let cursor_restored = match cursor0 {
            Some((x, y)) => {
                let ok = tiler_windows::ffi::input::set_cursor_pos(x, y);
                std::thread::sleep(std::time::Duration::from_millis(80));
                ok && tiler_windows::ffi::input::cursor_pos()
                    .is_some_and(|(cx, cy)| (cx - x).abs() <= 2 && (cy - y).abs() <= 2)
            }
            None => false,
        };

        let callbacks: Vec<K5Callback> = sink.events.lock().map(|g| g.clone()).unwrap_or_default();
        let relevant: Vec<&K5Callback> = callbacks
            .iter()
            .filter(|cb| k5_event_relevant(cb.event) && cb.hwnd == hwnd)
            .collect();
        let transitions = k5_transitions(&stream);
        let (covered, uncovered, ambiguous, segs) =
            k5_bracket(&stream, &transitions, &callbacks, K5_BRACKET_TOL_MS);
        let relevant_owned: Vec<K5Callback> = relevant.into_iter().copied().collect();
        let lags = k5_next_sample_lags(&relevant_owned, &stream);
        let period_ms = if stream.len() > 1 {
            (stream.last().map(|s| s.dt_ms).unwrap_or(0.0)
                - stream.first().map(|s| s.dt_ms).unwrap_or(0.0))
                / (stream.len() - 1) as f64
        } else {
            0.0
        };
        let unreadable = stream.iter().filter(|s| s.rect.is_none()).count();

        let summary = serde_json::json!({
            "leg": "k5-resize",
            "pmv2": true,
            "hwnd": hwnd,
            "fixture_pid": handoff.fixture_pid,
            "focus_method": focus_method,
            "foreground_ok": foreground_ok,
            "hooks_active": hooks_active,
            "poll": {"samples": stream.len(), "period_ms_mean": period_ms,
                     "unreadable": unreadable, "poll_ms": K5_POLL_MS},
            "callbacks": {"total": callbacks.len(),
                          "relevant_own_window": relevant_owned.len(),
                          "note": "delivery timestamps only; geometry is poll-sourced, never event-sourced"},
            "oracle": {"transitions": transitions.len(), "covered": covered,
                       "uncovered": uncovered, "ambiguous": ambiguous,
                       "bracket_tol_ms": K5_BRACKET_TOL_MS,
                       "segments": segs.iter().map(|(p, c, v)| serde_json::json!([p, c, v])).collect::<Vec<_>>()},
            "relevant_callback_to_poll_lag_ms": k5_stats(&lags),
            "stimulus": {"name": "synthetic_outer_resize",
                         "start_ms": stim_start, "end_ms": stim_end,
                         "before_outer": before_outer, "after_outer": after_outer,
                         "before_dwm": before_dwm, "after_dwm": after_dwm,
                         "outer_moved": matches!((before_outer, after_outer), (Some(a), Some(b)) if a != b),
                         "dwm_moved": matches!((before_dwm, after_dwm), (Some(a), Some(b)) if a != b),
                         "detail": detail},
            "frame_first": frame0,
            "frame_end": frame_end,
            "cursor_restored": cursor_restored,
            "button_held_at_exit": false,
        });
        let _ = std::fs::write(run_dir.join("k5-resize.json"), summary.to_string());
        let _ = std::fs::write(
            run_dir.join("k5-resize_polls.json"),
            serde_json::json!(stream).to_string(),
        );
        let _ = std::fs::write(
            run_dir.join("k5-resize_callbacks.json"),
            serde_json::json!(callbacks).to_string(),
        );
        teardown_best_effort(&fixture_exe, &run_dir, &handoff)
    }

    /// Forced hide-writer loss: mirrors `forced_writer_loss` but spawns the
    /// writer in hide mode (`--effect-visibility hide`). Returns the exact
    /// killed-writer provenance; preserves everything on any deviation.
    fn forced_hide_writer_loss(
        probe_exe: &Path,
        run_dir: &Path,
        run_raw: &str,
        caller: &str,
        record_name: &str,
        ready_name: &str,
        stay_ms: u64,
    ) -> Result<WriterLoss, i32> {
        let ready = run_dir.join(ready_name);
        let stay_arg = stay_ms.to_string();
        let ready_arg = ready.to_string_lossy().to_string();
        let argv = [
            "writer",
            "--run-dir",
            run_raw,
            "--caller",
            caller,
            "--effect-top",
            "false",
            "--effect-visibility",
            "hide",
            "--ready",
            &ready_arg,
            "--stay-ms",
            &stay_arg,
            "--record",
            record_name,
        ];
        let mut child = spawn_probe(probe_exe, &argv, run_dir, &format!("{caller}-writer.log"))
            .map_err(|e| {
                eprintln!("measure: hide writer spawn for {caller}: {e}; preserving");
                1
            })?;
        let snapshot = match proc_snapshot_of(child.id()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure: hide writer identity for {caller}: {e}; preserving");
                std::mem::forget(child);
                return Err(1);
            }
        };
        let _ = std::fs::write(
            run_dir.join(format!("{caller}-writer-spawn.json")),
            serde_json::json!({
                "caller": caller,
                "record": record_name,
                "argv": argv,
                "pid": snapshot.pid,
                "exe": snapshot.exe,
                "creation": snapshot.creation,
                "user": snapshot.user,
                "session": snapshot.session,
                "integrity": snapshot.integrity,
            })
            .to_string(),
        );
        if !wait_for_file(&ready, 20_000) {
            eprintln!("measure: hide writer never ready for {caller}; preserving (no kill)");
            std::mem::forget(child);
            return Err(1);
        }
        if terminate_exact(&snapshot).is_err() {
            eprintln!("measure: hide writer kill refused for {caller}; preserving");
            std::mem::forget(child);
            return Err(1);
        }
        match wait_child_status(&mut child, 10_000) {
            Some(status) => {
                let code = status.code();
                let abnormal = !status.success();
                let _ = std::fs::write(
                    run_dir.join(format!("{caller}-writer-kill.json")),
                    serde_json::json!({
                        "caller": caller,
                        "pid": snapshot.pid,
                        "kill": "exact-terminate",
                        "exit_code": code,
                        "exit_abnormal": abnormal,
                    })
                    .to_string(),
                );
                if !abnormal {
                    eprintln!("measure: hide writer for {caller} exited normally; preserving");
                    return Err(1);
                }
                Ok(WriterLoss {
                    record_name: record_name.to_string(),
                    pid: snapshot.pid,
                    exe: snapshot.exe,
                    creation: snapshot.creation,
                    user: snapshot.user,
                    session: snapshot.session,
                    integrity: snapshot.integrity,
                    exit_code: code,
                })
            }
            None => {
                eprintln!("measure: hide writer for {caller} survived exact kill; preserving");
                std::mem::forget(child);
                Err(1)
            }
        }
    }

    // -- Area 5: hide/reveal forced-writer-loss + hidden-arrival -------------
    //
    // Baseline (visible, geometry, show-state) is persisted PRE-hide in the
    // hide record; the writer is genuinely killed; the fresh recoverer
    // restores baseline visibility/geometry/show-state plus the marker.
    // Reads retain hidden/alive/visible/cloak/DWM/frame/tag/PID-start facts
    // and distinguish hide (IsWindowVisible false, cloak false) from cloak.
    // A hidden move brackets output/frame/visibility/lifetime before/after
    // and the reveal. Single-output run; cross-output records a limitation.
    // The public DwmSetWindowAttribute(DWMWA_CLOAK) probe on the own fixture
    // window records HRESULT + native readback + restore; refusal is a
    // recorded limitation, never a blocker.
    fn measure_hide_loss(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 hide-loss: --fixture-exe is required");
            return 2;
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 hide-loss: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 hide-loss: PMv2 not active");
            return 1;
        }
        let caller = "hd1";
        let handoff = match run_setup(&fixture_exe, &run_dir, caller, &[]) {
            Ok(h) => h,
            Err(code) => return code,
        };
        let probe_exe = match current_exe_or(1) {
            Ok(p) => p,
            Err(code) => return code,
        };
        let entry = match roster_entry(&handoff, caller) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 hide-loss: {e}; preserving");
                return 1;
            }
        };
        let fulls = monitor_fulls();
        let _ = std::fs::write(
            run_dir.join("monitors.json"),
            monitors_value(&fulls).to_string(),
        );
        let before = match capture_frame(&handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 hide-loss before: {e}; preserving");
                return 1;
            }
        };
        if !before.visible || before.marker != Some(entry.marker) {
            eprintln!(
                "measure123 hide-loss: baseline not acquired-clean (visible={}, marker={:?} vs {}); preserving",
                before.visible, before.marker, entry.marker
            );
            return 1;
        }
        if before.cloak == Some(true) {
            eprintln!("measure123 hide-loss: baseline cloaked; preserving");
            return 1;
        }
        let record_name = format!("hide-write-{caller}.json");
        let loss = match forced_hide_writer_loss(
            &probe_exe,
            &run_dir,
            &run_raw,
            caller,
            &record_name,
            &format!("{caller}.ready"),
            30_000,
        ) {
            Ok(l) => l,
            Err(code) => return code,
        };
        // Post-loss reads: hidden + alive + cloak-false (hide, not cloak).
        let hidden = match capture_frame(&handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 hide-loss post-loss read: {e}; preserving");
                return 1;
            }
        };
        let alive = tiler_windows::ffi::window::is_window(entry.hwnd);
        let pid_still_fixture =
            tiler_windows::ffi::window::window_pid(entry.hwnd) == handoff.fixture_pid;
        let expected_marker = marker_effect_for(entry.marker);
        if hidden.visible || !alive || !pid_still_fixture {
            eprintln!(
                "measure123 hide-loss: post-loss liveness unexpected (visible={}, alive={alive}, pid_fixture={pid_still_fixture}); preserving",
                hidden.visible
            );
            return 1;
        }
        if hidden.cloak == Some(true) {
            eprintln!(
                "measure123 hide-loss: post-loss cloaked (expected hide, not cloak); preserving"
            );
            return 1;
        }
        if hidden.marker != Some(expected_marker) {
            eprintln!(
                "measure123 hide-loss: post-loss marker unexpected ({:?}); preserving",
                hidden.marker
            );
            return 1;
        }
        // Hide-record survival: decodes and carries the marker preimage.
        let record_ok = (|| -> Result<bool, String> {
            let raw = std::fs::read_to_string(run_dir.join(&record_name))
                .map_err(|e| format!("read record: {e}"))?;
            let record =
                decode_hide_record(&raw).map_err(|_| "hide record rejected".to_string())?;
            let opaque = unhex(&record.opaque_hex).ok_or("opaque unhex failed".to_string())?;
            Ok(decode_marker_opaque(&opaque) == Some(entry.marker)
                && record.preimage_visible == before.visible
                && record.preimage_outer == before.outer_px)
        })();
        let record_ok = match record_ok {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 hide-loss record survival: {e}; preserving");
                return 1;
            }
        };
        if !record_ok {
            eprintln!("measure123 hide-loss: hide record does not carry the baseline; preserving");
            return 1;
        }
        // Hidden move while hidden: app-own position set, bracketed.
        let hidden_before_move = hidden.clone();
        let mut move_detail = serde_json::json!({"skipped": false});
        let (move_before_outer, move_after_outer) = match before.outer_px {
            Some([l, t, r, b]) => {
                let (nw, nh) = (r - l, b - t);
                let target = [l + 60, t + 40, l + 60 + nw, t + 40 + nh];
                match app_set(
                    &handoff,
                    caller,
                    serde_json::json!({"position_outer_px": [target[0], target[1]]}),
                ) {
                    Ok(reply) => {
                        move_detail["reply"] = reply;
                        let (settled_frame, settled) = match settle_frame(&handoff, entry, 8_000) {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!(
                                    "measure123 hide-loss settle-after-move: {e}; preserving"
                                );
                                return 1;
                            }
                        };
                        move_detail["settled"] = serde_json::json!(settled);
                        move_detail["visible_during"] = serde_json::json!(settled_frame.visible);
                        (
                            Some(hidden_before_move.outer_px),
                            Some(settled_frame.outer_px),
                        )
                    }
                    Err(e) => {
                        move_detail["error"] = serde_json::json!(e);
                        (
                            Some(hidden_before_move.outer_px),
                            hidden_before_move.outer_px.into(),
                        )
                    }
                }
            }
            None => {
                move_detail["skipped"] = serde_json::json!("baseline outer unreadable");
                (None, hidden_before_move.outer_px.into())
            }
        };
        let coverage_before = coverage_verdict(&fulls, &hidden_before_move.dwm_px);
        let live_after_move =
            capture_frame(&handoff, entry).unwrap_or_else(|_| hidden_before_move.clone());
        let coverage_after = coverage_verdict(&fulls, &live_after_move.dwm_px);
        // Public cloak probe on the own fixture window (external caller, not
        // the product path): set/restore with HRESULT + native readback.
        let cloak_base = tiler_windows::ffi::window::cloaked(entry.hwnd);
        let hr_set = ffi_ext::dwm_set_cloak(entry.hwnd, true);
        let cloak_set = tiler_windows::ffi::window::cloaked(entry.hwnd);
        let hr_clear = ffi_ext::dwm_set_cloak(entry.hwnd, false);
        let cloak_restored = tiler_windows::ffi::window::cloaked(entry.hwnd);
        let cloak_refused = hr_set != 0 || hr_clear != 0;
        let _ = std::fs::write(
            run_dir.join("cloak-probe.json"),
            serde_json::json!({
                "baseline_cloak": cloak_base,
                "set_hr": hr_set,
                "cloak_after_set": cloak_set,
                "clear_hr": hr_clear,
                "cloak_after_restore": cloak_restored,
                "refused": cloak_refused,
                "note": "public DwmSetWindowAttribute(DWMWA_CLOAK) on the own fixture window; refusal is a recorded limitation",
            })
            .to_string(),
        );
        // Fresh recoverer restores baseline visibility/geometry/show/marker.
        let status = match run_recoverer(
            &probe_exe,
            &run_dir,
            &run_raw,
            &record_name,
            "recoverer-hide.log",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("measure123 hide-loss recoverer: {e}; preserving");
                return 1;
            }
        };
        let recover_exit = status.as_ref().and_then(|s| s.code());
        if recover_exit != Some(0) {
            eprintln!("measure123 hide-loss: recoverer exit {recover_exit:?}; preserving");
            return 1;
        }
        let after = match capture_frame(&handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 hide-loss post-recover read: {e}; preserving");
                return 1;
            }
        };
        if after.visible != before.visible
            || after.outer_px != before.outer_px
            || after.placement_show != before.placement_show
            || after.marker != Some(entry.marker)
        {
            eprintln!(
                "measure123 hide-loss: post-recover mismatch (visible {}->{}, outer {:?}->{:?}, show {:?}->{:?}, marker {:?}); preserving",
                before.visible,
                after.visible,
                before.outer_px,
                after.outer_px,
                before.placement_show,
                after.placement_show,
                after.marker
            );
            return 1;
        }
        if after.cloak == Some(true) {
            eprintln!("measure123 hide-loss: post-recover cloaked; preserving");
            return 1;
        }
        let summary = serde_json::json!({
            "leg": "hide-loss",
            "baseline": {"visible": before.visible, "cloak": before.cloak, "outer_px": before.outer_px, "dwm_px": before.dwm_px, "show": before.placement_show, "marker": before.marker},
            "writer": {"pid": loss.pid, "exit_code": loss.exit_code, "exit_abnormal": true},
            "hidden_after_loss": {"visible": hidden.visible, "alive": alive, "pid_fixture": pid_still_fixture, "cloak": hidden.cloak, "dwm_px": hidden.dwm_px, "outer_px": hidden.outer_px, "tag_present": hidden.tag_present, "marker": hidden.marker, "marker_expected": expected_marker},
            "hide_vs_cloak": "hidden reads visible=false with cloak=false: a ShowWindow hide, not a DWM cloak",
            "record_survives_with_baseline": record_ok,
            "hidden_move": {"before_outer": move_before_outer, "after_outer": move_after_outer, "detail": move_detail, "visible_before": hidden_before_move.visible, "visible_after": live_after_move.visible, "coverage_before": coverage_before, "coverage_after": coverage_after},
            "cloak_probe": {"set_hr": hr_set, "cloak_after_set": cloak_set, "clear_hr": hr_clear, "cloak_after_restore": cloak_restored, "refused": cloak_refused},
            "recoverer_exit": recover_exit,
            "revealed": {"visible": after.visible, "outer_px": after.outer_px, "dwm_px": after.dwm_px, "show": after.placement_show, "cloak": after.cloak, "marker": after.marker, "marker_expected": entry.marker},
            "limitation": "single-output run; cross-output hidden arrival not attempted; cloak refusal (if any) is a recorded limitation",
        });
        let _ = std::fs::write(run_dir.join("hide-loss.json"), summary.to_string());
        match std::fs::read_to_string(run_dir.join("recovered-hide.json")) {
            Ok(raw) => {
                let writes = serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|v| v.get("policy_writes").and_then(|w| w.as_u64()))
                    .unwrap_or(0);
                if writes < 2 {
                    eprintln!(
                        "measure123 hide-loss: recovered-hide policy_writes {writes}, expected >=2; preserving"
                    );
                    return 1;
                }
            }
            Err(_) => {
                eprintln!("measure123 hide-loss: recovered-hide.json absent; preserving");
                return 1;
            }
        }
        teardown_best_effort(&fixture_exe, &run_dir, &handoff)
    }

    // -- Area 6: ordinary medium identity ------------------------------------
    //
    // HeldProcess identity/integrity/exe/session plus checked
    // style/exstyle/class/DWM/cloak/owner/parent facts with exact own-fixture
    // lifetime joins (PID/creation/title/tag/HWND). Exe comparison uses the
    // production ASCII slash+lower normalization. Raw paths land only in the
    // external run dir, never in the repository. Null/unreadable vs
    // unsupported routes are recorded distinctly. Elevated execution stays a
    // user step; this leg never invokes UAC and refuses non-medium integrity.
    fn measure_identity(args: &[String]) -> i32 {
        let Some(fixture_exe) = flag(args, "--fixture-exe") else {
            eprintln!("measure123 identity: --fixture-exe is required");
            return 2;
        };
        let Some(run_raw) = flag(args, "--run-dir") else {
            eprintln!("measure123 identity: --run-dir is required");
            return 2;
        };
        let run_dir = PathBuf::from(&run_raw);
        if require_pm_v2().is_err() {
            eprintln!("measure123 identity: PMv2 not active");
            return 1;
        }
        let caller = "id1";
        let handoff = match run_setup(&fixture_exe, &run_dir, caller, &[]) {
            Ok(h) => h,
            Err(code) => return code,
        };
        let entry = match roster_entry(&handoff, caller) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("measure123 identity: {e}; preserving");
                return 1;
            }
        };
        if verify_fixture(&handoff).is_err() || verify_fixture_medium(&handoff).is_err() {
            eprintln!("measure123 identity: fixture identity not verified; preserving");
            return 1;
        }
        let frame = match capture_frame(&handoff, entry) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("measure123 identity frame: {e}; preserving");
                return 1;
            }
        };
        // Exact own-fixture lifetime joins.
        let held = match HeldProcess::open(handoff.fixture_pid) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("measure123 identity: fixture process absent ({e}); preserving");
                return 1;
            }
        };
        let id = match held.identity() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 identity: fixture identity unreadable ({e}); preserving");
                return 1;
            }
        };
        let integrity = match held.integrity() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("measure123 identity: fixture integrity unreadable ({e}); preserving");
                return 1;
            }
        };
        let session = tiler_windows::ffi::session_of_pid(handoff.fixture_pid).unwrap_or(u32::MAX);
        let joins = serde_json::json!({
            "pid_matches": id.pid == handoff.fixture_pid,
            "creation_matches": id.process_creation == handoff.fixture_creation,
            "user_matches": id.user_sid == handoff.fixture_user,
            "session_matches": id.session_id == handoff.fixture_session && session == handoff.fixture_session,
            "exe_matches_handoff": id.exe_path == handoff.fixture_exe,
            "integrity_matches": integrity == handoff.fixture_integrity,
            "integrity_is_medium": integrity == MEDIUM_RID,
            "title_exact": frame.hwnd == entry.hwnd,
            "tag_present": frame.tag_present,
            "window_alive": tiler_windows::ffi::window::is_window(entry.hwnd),
            "window_pid_matches": tiler_windows::ffi::window::window_pid(entry.hwnd) == handoff.fixture_pid,
        });
        // Production normalization (ASCII slash+lower; non-ASCII preserved,
        // dot-segments unresolved): compare both raw and normalized sides.
        let exe_norm_equal =
            tiler_windows::lifecycle::exe_paths_equal(&id.exe_path, &handoff.fixture_exe);
        // Checked facts with null/unreadable vs unsupported distinct.
        let style = tiler_windows::ffi::window::window_style_checked(entry.hwnd);
        let exstyle = ffi_ext::window_exstyle_checked(entry.hwnd);
        let class_try = tiler_windows::ffi::window::try_class_name(entry.hwnd);
        let class_route = match &class_try {
            Some(_) => "readable",
            None => "unreadable",
        };
        let dwm = tiler_windows::ffi::window::extended_frame_bounds(entry.hwnd)
            .map(|r| [r.left, r.top, r.right, r.bottom]);
        let dwm_route = if dwm.is_some() {
            "readable"
        } else {
            "unreadable"
        };
        let cloak = tiler_windows::ffi::window::cloaked(entry.hwnd);
        let cloak_route = if cloak.is_some() {
            "readable"
        } else {
            "unreadable"
        };
        let owner = tiler_windows::ffi::window::owner_of(entry.hwnd);
        let parent = tiler_windows::ffi::window::parent_of(entry.hwnd);
        let summary = serde_json::json!({
            "leg": "identity",
            "fixture": {"pid": id.pid, "exe": id.exe_path, "creation": id.process_creation, "user": id.user_sid, "session": id.session_id, "integrity": integrity, "integrity_medium_rid": MEDIUM_RID},
            "exe_normalized_equal": exe_norm_equal,
            "session_live": session,
            "joins": joins,
            "style": style,
            "style_route": if style.is_some() { "readable" } else { "unreadable" },
            "exstyle": exstyle,
            "exstyle_route": if exstyle.is_some() { "readable" } else { "unreadable" },
            "class": class_try,
            "class_route": class_route,
            "dwm_px": dwm,
            "dwm_route": dwm_route,
            "cloak": cloak,
            "cloak_route": cloak_route,
            "owner": owner,
            "parent": parent,
            "frame": frame,
            "note": "raw exe path lives only in this external run dir; elevated execution stays a user step (no UAC here); unreadable routes are null/unreadable, never unsupported claims",
        });
        let _ = std::fs::write(run_dir.join("identity.json"), summary.to_string());
        let ok = joins.get("pid_matches") == Some(&serde_json::Value::Bool(true))
            && joins.get("creation_matches") == Some(&serde_json::Value::Bool(true))
            && joins.get("user_matches") == Some(&serde_json::Value::Bool(true))
            && joins.get("session_matches") == Some(&serde_json::Value::Bool(true))
            && joins.get("integrity_is_medium") == Some(&serde_json::Value::Bool(true))
            && joins.get("tag_present") == Some(&serde_json::Value::Bool(true))
            && joins.get("window_pid_matches") == Some(&serde_json::Value::Bool(true))
            && exe_norm_equal
            && style.is_some()
            && exstyle.is_some();
        if !ok {
            eprintln!("measure123 identity: join/fact mismatch; preserving");
            return 1;
        }
        teardown_best_effort(&fixture_exe, &run_dir, &handoff)
    }

    /// Exact locked version of a crate from Cargo.lock text.
    fn lock_version(lock: &str, name: &str) -> Option<String> {
        let mut lines = lock.lines().peekable();
        while let Some(line) = lines.next() {
            if line.trim() == format!("name = \"{name}\"") {
                for follow in lines.by_ref().take(3) {
                    let follow = follow.trim();
                    if let Some(version) = follow.strip_prefix("version = \"")
                        && let Some(end) = version.find('"')
                    {
                        return Some(version[..end].to_string());
                    }
                }
            }
        }
        None
    }

    /// All locked versions of a crate from Cargo.lock text (the lock can
    /// carry several major versions of one crate).
    fn lock_versions(lock: &str, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut lines = lock.lines().peekable();
        while let Some(line) = lines.next() {
            if line.trim() == format!("name = \"{name}\"") {
                for follow in lines.by_ref().take(3) {
                    let follow = follow.trim();
                    if let Some(version) = follow.strip_prefix("version = \"")
                        && let Some(end) = version.find('"')
                    {
                        out.push(version[..end].to_string());
                        break;
                    }
                }
            }
        }
        out
    }

    /// Cached `.crate` file names (names only, never local paths).
    fn cached_crate_names() -> Vec<String> {
        let mut roots = Vec::new();
        if let Ok(home) = std::env::var("CARGO_HOME") {
            roots.push(PathBuf::from(home).join("registry").join("cache"));
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            roots.push(
                PathBuf::from(profile)
                    .join(".cargo")
                    .join("registry")
                    .join("cache"),
            );
        }
        let mut names = Vec::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(inner) = std::fs::read_dir(entry.path()) else {
                    continue;
                };
                for file in inner.flatten() {
                    if let Some(name) = file.file_name().to_str()
                        && name.ends_with(".crate")
                    {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names.sort();
        names.dedup();
        names
    }

    /// `where.exe` presence probe (read-only, no paths recorded).
    fn tool_present(tool: &str) -> bool {
        std::process::Command::new("where.exe")
            .arg(tool)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Offline unit tests: pure policy/codec/record logic only. No Windows
    // APIs, no window creation, no fixture processes.
    // -----------------------------------------------------------------------
    #[cfg(test)]
    mod tests {
        use super::*;

        fn key(hwnd: u64) -> WindowKey {
            WindowKey {
                hwnd,
                pid: 4242,
                creation: "creation-1".to_string(),
            }
        }

        fn good_facts() -> GuardFacts {
            GuardFacts {
                stored_key: key(100),
                live_hwnd: 100,
                live_pid: 4242,
                live_creation: "creation-1".to_string(),
                expected_exe: "C:\\probe\\tiler-live-fixture.exe".to_string(),
                live_exe: "C:\\probe\\tiler-live-fixture.exe".to_string(),
                expected_user: "S-1-5-21-1".to_string(),
                live_user: "S-1-5-21-1".to_string(),
                expected_session: 1,
                live_session: 1,
                expected_integrity: 0x2000,
                live_integrity: 0x2000,
                expected_caller: "a".to_string(),
                live_caller: "a".to_string(),
                expected_title: expected_title("a", "run7", None),
                live_title: expected_title("a", "run7", None),
                stored_tag: "0123456789abcdef".to_string(),
                live_tag: LiveTag::Value(0x0123_4567_89ab_cdef),
                exstyle_readable: true,
                bootstrap_tag_establishment: false,
            }
        }

        fn fake_with_window_live_true() -> FakeBackend {
            let mut b = FakeBackend::default();
            b.exstyle.insert(100, WS_EX_TOPMOST_BIT);
            b.topmost.insert(100, true);
            b.tags.insert(100, 0x0123_4567_89ab_cdef);
            b.markers.insert(100, 0x2222_2222_2222_2222);
            b
        }

        fn spec<'a>(
            effect: bool,
            preimage: Option<bool>,
            full_title: &'a str,
            opaque: &'a [u8],
        ) -> RecordSpec<'a> {
            RecordSpec {
                schema: RECORD_SCHEMA,
                fixture_exe: "C:\\probe\\tiler-live-fixture.exe",
                fixture_pid: 4242,
                fixture_creation: "creation-1",
                fixture_user: "S-1-5-21-1",
                fixture_session: 1,
                fixture_integrity: 0x2000,
                caller: "a",
                full_title,
                tag: "0123456789abcdef",
                hwnd: 100,
                effect_topmost: effect,
                preimage_topmost: preimage,
                opaque,
            }
        }

        fn title_a() -> String {
            expected_title("a", "run7", None)
        }

        fn marker_bytes(v: u64) -> Vec<u8> {
            encode_marker_opaque(v)
        }

        fn scripted(mut backend: FakeBackend, facts: &GuardFacts, n: usize) -> FakeBackend {
            for _ in 0..n {
                backend.observe_queue.push_back(Some(facts.clone()));
            }
            backend
        }

        #[test]
        fn guard_allows_exact_join() {
            assert!(guard_allow(&good_facts()));
        }

        #[test]
        fn parse_title_identity_rejoins_caller_and_run_tag() {
            let full = expected_title("a", "run7", Some("suffix text"));
            let (caller, run) = parse_title_identity(&full).unwrap();
            assert_eq!((caller.as_str(), run.as_str()), ("a", "run7"));
            assert!(parse_title_identity("garbage").is_none());
            assert!(parse_title_identity(&format!("{TITLE_SENTINEL}:onlyone")).is_none());
        }

        #[test]
        fn each_mismatch_dimension_refuses_with_zero_writes() {
            let cases: Vec<(&str, Box<dyn Fn(&mut GuardFacts)>)> = vec![
                ("hwnd", Box::new(|f| f.live_hwnd = 101)),
                ("pid", Box::new(|f| f.live_pid = 9999)),
                (
                    "creation",
                    Box::new(|f| f.live_creation = "creation-2".to_string()),
                ),
                (
                    "exe",
                    Box::new(|f| f.live_exe = "C:\\other\\app.exe".to_string()),
                ),
                ("user", Box::new(|f| f.live_user = "S-1-5-21-2".to_string())),
                ("session", Box::new(|f| f.live_session = 2)),
                ("medium", Box::new(|f| f.live_integrity = 0x3000)),
                ("caller", Box::new(|f| f.live_caller = "b".to_string())),
                (
                    "title",
                    Box::new(|f| f.live_title = expected_title("b", "run7", None)),
                ),
                (
                    "tag-value",
                    Box::new(|f| f.live_tag = LiveTag::Value(0xdead_beef_0000_0001)),
                ),
                ("tag-absent", Box::new(|f| f.live_tag = LiveTag::Absent)),
                (
                    "tag-unreadable",
                    Box::new(|f| f.live_tag = LiveTag::Unreadable),
                ),
                ("exstyle", Box::new(|f| f.exstyle_readable = false)),
            ];
            for (name, mutate) in cases {
                let expected = good_facts();
                let mut fresh = good_facts();
                mutate(&mut fresh);
                assert!(!guard_allow(&fresh), "dimension {name} must refuse");
                // Chokepoint refreshes inside: scripted fresh observation is
                // the mutated one, so zero writes.
                let mut backend = fake_with_window_live_true();
                backend.observe_queue.push_back(Some(fresh));
                let mut counts = EffectCounts::default();
                let outcome = guarded_topmost_write(&mut backend, &expected, true, &mut counts);
                assert_eq!(outcome, WriteOutcome::Refused, "{name}");
                assert!(backend.writes.is_empty(), "{name}: zero backend writes");
                assert_eq!(counts.policy_writes, 0, "{name}");
            }
        }

        #[test]
        fn guarded_write_verifies_postcheck_and_counts() {
            let expected = good_facts();
            // Live true, request true: postcheck passes.
            let mut backend = fake_with_window_live_true();
            backend.topmost.insert(100, false);
            backend = scripted(backend, &expected, 2);
            let mut counts = EffectCounts::default();
            assert_eq!(
                guarded_topmost_write(&mut backend, &expected, true, &mut counts),
                WriteOutcome::AppliedVerified
            );
            assert_eq!(counts.policy_writes, 1);
            // Write failure refuses with zero count increment beyond... (fail
            // path returns Refused and the write journal still records the
            // attempt; policy count only increments on attempted write, so
            // check Refused here with fail_writes).
            let mut backend = fake_with_window_live_true();
            backend.fail_writes = true;
            backend = scripted(backend, &expected, 1);
            let mut counts = EffectCounts::default();
            assert_eq!(
                guarded_topmost_write(&mut backend, &expected, true, &mut counts),
                WriteOutcome::Refused
            );
            assert_eq!(counts.policy_writes, 0);
        }

        #[test]
        fn observe_change_after_commit_writes_zero() {
            // Record published against good facts; the pre-write observe moved
            // (stale tag) so the chokepoint must refuse with zero writes.
            let expected = good_facts();
            let mut moved = good_facts();
            moved.live_tag = LiveTag::Value(0x9999_8888_7777_6666);
            let mut backend = fake_with_window_live_true();
            backend.observe_queue.push_back(Some(moved));
            let mut counts = EffectCounts::default();
            assert_eq!(
                guarded_topmost_write(&mut backend, &expected, false, &mut counts),
                WriteOutcome::Refused
            );
            assert!(backend.writes.is_empty());
            assert_eq!(counts.policy_writes, 0);
        }

        #[test]
        fn bootstrap_exception_needs_absent_tag_and_full_identity() {
            let mut facts = good_facts();
            facts.bootstrap_tag_establishment = true;
            facts.live_tag = LiveTag::Absent;
            assert!(guard_allow(&facts));
            // Present tag under bootstrap still refuses.
            let mut present = facts.clone();
            present.live_tag = LiveTag::Value(0x0123_4567_89ab_cdef);
            assert!(!guard_allow(&present));
            // Identity mismatch still refuses under bootstrap.
            let mut wrong = facts.clone();
            wrong.live_pid = 1;
            assert!(!guard_allow(&wrong));
            // Bootstrap counts as baseline setup, never a policy write.
            let mut backend = FakeBackend::default();
            backend.exstyle.insert(100, 0);
            backend.observe_queue.push_back(Some(facts.clone()));
            // Postcheck observe: tag now equals stored.
            let mut post = facts.clone();
            post.live_tag = LiveTag::Value(0x0123_4567_89ab_cdef);
            backend.observe_queue.push_back(Some(post));
            let mut counts = EffectCounts::default();
            assert!(bootstrap_tag(&mut backend, &facts, &mut counts));
            assert_eq!(counts.baseline_setups, 1);
            assert_eq!(counts.policy_writes, 0);
        }

        #[test]
        fn preimage_restore_falseward_from_live_true() {
            // P=false, E=true, L=true => guarded set false => Restored.
            let title = title_a();
            let marker: u64 = 0x2222_2222_2222_2222;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let facts = good_facts();
            let mut backend = fake_with_window_live_true();
            backend.markers.insert(100, marker_effect_for(marker));
            backend = scripted(backend, &facts, 5);
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::Restored
            );
            assert_eq!(backend.read_topmost(100), Some(false));
            assert_eq!(backend.read_marker(100), Some(marker));
        }

        #[test]
        fn live_false_already_at_preimage_writes_zero() {
            let title = title_a();
            let marker: u64 = 0x3333_3333_3333_3333;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let facts = good_facts();
            let mut backend = FakeBackend::default();
            backend.topmost.insert(100, false);
            backend.tags.insert(100, 0x0123_4567_89ab_cdef);
            backend.markers.insert(100, marker);
            backend.observe_queue.push_back(Some(facts));
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::NoWriteAlreadyAtPreimage
            );
            assert!(backend.writes.is_empty());
        }

        #[test]
        fn preowned_branch_writes_zero_policy() {
            assert_eq!(acquire_for_write(true), AcquireOutcome::PreownedNoWrite);
            assert_eq!(acquire_for_write(false), AcquireOutcome::Acquired);
            // A preowned recovery performs no backend writes at all, even
            // when live==true and the record is otherwise valid.
            let title = title_a();
            let marker: u64 = 0x4444_4444_4444_4444;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let mut backend = fake_with_window_live_true();
            let mut counts = EffectCounts::default();
            let outcome = recover_from_record(&record, true, &mut backend, &mut counts);
            assert_eq!(outcome, RecoverOutcome::NoWritePreowned);
            assert!(backend.writes.is_empty());
            assert!(backend.observe_queue.is_empty());
            assert_eq!(counts.policy_writes, 0);
        }

        #[test]
        fn external_true_to_true_reverts_to_preimage_false() {
            // External set L=true on a P=false window is not ownership proof;
            // the preimage rule still reverts to false (observable mechanism,
            // no veto, no latch).
            let title = title_a();
            let marker: u64 = 0x5555_5555_5555_5555;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let facts = good_facts();
            let mut backend = fake_with_window_live_true();
            backend.topmost.insert(100, true);
            backend.markers.insert(100, marker_effect_for(marker));
            backend = scripted(backend, &facts, 5);
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::Restored
            );
            assert_eq!(backend.read_topmost(100), Some(false));
        }

        #[test]
        fn topmost_codec_distinct_nonzero_roundtrip() {
            assert_ne!(TOPMOST_FALSE_CODE, 0);
            assert_ne!(TOPMOST_TRUE_CODE, 0);
            assert_ne!(TOPMOST_FALSE_CODE, TOPMOST_TRUE_CODE);
            assert_eq!(decode_topmost(encode_topmost(false)), Some(false));
            assert_eq!(decode_topmost(encode_topmost(true)), Some(true));
            assert_eq!(decode_topmost(0), None);
            assert_eq!(decode_topmost(0x1234), None);
        }

        #[test]
        fn marker_opaque_roundtrip_and_rejects_bad_body() {
            let v: u64 = 0x1234_5678_9abc_def0;
            let bytes = encode_marker_opaque(v);
            assert!(bytes.len() <= MAX_OPAQUE_BYTES);
            assert_eq!(decode_marker_opaque(&bytes), Some(v));
            assert_eq!(decode_marker_opaque(b"not-json"), None);
            assert_eq!(decode_marker_opaque(b"{\"marker_preimage\":0}"), None);
            // A stuffed opaque body rejects the whole record.
            let title = title_a();
            let bad = encode_record(&spec(true, Some(false), &title, b"stuffed")).unwrap_err();
            assert_eq!(bad, RecordReject::Mismatch);
        }

        #[test]
        fn record_rejects_corrupt_truncated_version_oversize() {
            let title = expected_title("a", "r", None);
            let marker: u64 = 0x6666_6666_6666_6666;
            let good =
                encode_record(&spec(false, Some(false), &title, &marker_bytes(marker))).unwrap();
            // Truncated.
            assert!(decode_record(&good[..good.len() / 2]).is_err());
            // Corrupt checksum.
            let mut tampered: serde_json::Value = serde_json::from_str(&good).unwrap();
            tampered["effect_topmost"] = serde_json::json!(true);
            assert_eq!(
                decode_record(&serde_json::to_string(&tampered).unwrap()),
                Err(RecordReject::Mismatch)
            );
            // Unknown version.
            let mut versioned: serde_json::Value = serde_json::from_str(&good).unwrap();
            versioned["schema"] = serde_json::json!(2);
            assert_eq!(
                decode_record(&serde_json::to_string(&versioned).unwrap()),
                Err(RecordReject::Version)
            );
            // Oversize opaque.
            let big = vec![0u8; MAX_OPAQUE_BYTES + 1];
            let mut over = spec(false, None, "t", &big);
            over.caller = "a";
            assert_eq!(encode_record(&over), Err(RecordReject::Oversize));
            // Incomplete (missing required field).
            assert_eq!(
                decode_record(r#"{"schema":1}"#),
                Err(RecordReject::Incomplete)
            );
        }

        #[test]
        fn stale_tag_and_recycled_hwnd_rejected() {
            let title = title_a();
            let marker: u64 = 0x7777_7777_7777_7777;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let base = FreshObs {
                hwnd: 100,
                pid: 4242,
                creation: "creation-1".to_string(),
                exe: "C:\\probe\\tiler-live-fixture.exe".to_string(),
                user: "S-1-5-21-1".to_string(),
                session: 1,
                integrity: 0x2000,
                caller: "a".to_string(),
                title: expected_title("a", "run7", None),
                live_tag: LiveTag::Value(0x0123_4567_89ab_cdef),
            };
            assert!(record_authorizes(&decode_record(&record).unwrap(), &base));
            // Stale tag (same-process recycled HWND keeps the triple).
            let mut stale = base.clone();
            stale.live_tag = LiveTag::Value(0x9999_8888_7777_6666);
            assert!(!record_authorizes(&decode_record(&record).unwrap(), &stale));
            // Recycled HWND number in a new process.
            let mut recycled = base.clone();
            recycled.pid = 5555;
            recycled.creation = "creation-9".to_string();
            assert!(!record_authorizes(
                &decode_record(&record).unwrap(),
                &recycled
            ));
            // Absent/unreadable tags never authorize (old record after
            // close/recreate refuses with zero writes).
            let mut absent = base.clone();
            absent.live_tag = LiveTag::Absent;
            assert!(!record_authorizes(
                &decode_record(&record).unwrap(),
                &absent
            ));
            let mut unread = base;
            unread.live_tag = LiveTag::Unreadable;
            assert!(!record_authorizes(
                &decode_record(&record).unwrap(),
                &unread
            ));
            // Same-PID/HWND with a mismatched tag refuses recovery outright.
            let facts = good_facts();
            let mut backend = FakeBackend::default();
            backend.topmost.insert(100, true);
            backend.tags.insert(100, 0x9999_8888_7777_6666);
            backend.markers.insert(100, marker);
            let mut stale_facts = facts.clone();
            stale_facts.live_tag = LiveTag::Value(0x9999_8888_7777_6666);
            backend.observe_queue.push_back(Some(stale_facts));
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::RefusedNoWrites
            );
            assert!(backend.writes.is_empty());
        }

        #[test]
        fn nonascii_title_exact_match_preserves_chars() {
            let full = expected_title("a", "run7", Some("lab-日本語-✓"));
            assert!(full.contains("日本語"));
            assert_eq!(full, expected_title("a", "run7", Some("lab-日本語-✓")));
            assert_ne!(
                expected_title("a", "run7", None),
                expected_title("b", "run7", None)
            );
            let (caller, run) = parse_title_identity(&full).unwrap();
            assert_eq!((caller.as_str(), run.as_str()), ("a", "run7"));
        }

        #[test]
        fn exact_identity_match_requires_all_fields() {
            let good = ProcSnapshot {
                pid: 4242,
                exe: "C:\\x\\f.exe".to_string(),
                creation: "c1".to_string(),
                user: "S-1".to_string(),
                session: 1,
                integrity: MEDIUM_RID,
            };
            assert!(exact_identity_match(&good, &good));
            let mut wrong = good.clone();
            wrong.pid = 1;
            assert!(!exact_identity_match(&good, &wrong));
            wrong = good.clone();
            wrong.exe = "C:\\y\\g.exe".to_string();
            assert!(!exact_identity_match(&good, &wrong));
            wrong = good.clone();
            wrong.creation = "c2".to_string();
            assert!(!exact_identity_match(&good, &wrong));
            wrong = good.clone();
            wrong.user = "S-2".to_string();
            assert!(!exact_identity_match(&good, &wrong));
            wrong = good.clone();
            wrong.session = 2;
            assert!(!exact_identity_match(&good, &wrong));
            wrong = good.clone();
            wrong.integrity = 0x3000;
            assert!(!exact_identity_match(&good, &wrong));
            // Empty expected sides refuse (fail-closed, zero kills).
            for empty in [
                ProcSnapshot {
                    pid: 0,
                    ..good.clone()
                },
                ProcSnapshot {
                    exe: String::new(),
                    ..good.clone()
                },
                ProcSnapshot {
                    creation: String::new(),
                    ..good.clone()
                },
                ProcSnapshot {
                    user: String::new(),
                    ..good.clone()
                },
                ProcSnapshot {
                    integrity: 0,
                    ..good.clone()
                },
            ] {
                assert!(!exact_identity_match(&empty, &good));
            }
        }

        #[test]
        fn planned_closes_needs_every_caller_clean() {
            assert_eq!(planned_closes(&[]), None);
            assert_eq!(
                planned_closes(&[("a", true), ("b", true)]),
                Some(vec!["a".to_string(), "b".to_string()])
            );
            assert_eq!(planned_closes(&[("a", true), ("b", false)]), None);
            assert_eq!(planned_closes(&[("a", false)]), None);
        }

        #[test]
        fn old_record_after_recreate_refuses_with_zero_writes() {
            // Tag absent (window destroyed; HWND number recycled without the
            // tag) authorizes nothing.
            let title = title_a();
            let marker: u64 = 0x8888_8888_8888_8888;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let mut absent_facts = good_facts();
            absent_facts.live_tag = LiveTag::Absent;
            let mut backend = FakeBackend::default();
            backend.topmost.insert(100, true);
            backend.markers.insert(100, marker.wrapping_add(3));
            backend.observe_queue.push_back(Some(absent_facts));
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::RefusedNoWrites
            );
            assert!(backend.writes.is_empty());
            assert_eq!(counts.policy_writes, 0);
        }

        #[test]
        fn unreadable_live_state_refuses_with_no_writes() {
            let title = title_a();
            let marker: u64 = 0x9999_9999_9999_9999;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let mut backend = FakeBackend::default();
            backend.observe_queue.push_back(None);
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::RefusedNoWrites
            );
            assert!(backend.writes.is_empty());
        }

        #[test]
        fn bootstrap_live_adapter_fresh_without_flag_allows_absent() {
            // LiveRoster/rejoin_live always reports fresh facts with
            // `bootstrap_tag_establishment == false`; the single dedicated
            // establishment must still succeed on a fresh absent tag with
            // full identity and a valid nonzero stored tag.
            let mut expected = good_facts();
            expected.bootstrap_tag_establishment = true;
            expected.live_tag = LiveTag::Absent;
            let mut fresh = expected.clone();
            fresh.bootstrap_tag_establishment = false;
            let mut post = expected.clone();
            post.bootstrap_tag_establishment = false;
            post.live_tag = LiveTag::Value(0x0123_4567_89ab_cdef);
            let mut backend = FakeBackend::default();
            backend.exstyle.insert(100, 0);
            backend.observe_queue.push_back(Some(fresh));
            backend.observe_queue.push_back(Some(post));
            let mut counts = EffectCounts::default();
            assert!(bootstrap_tag(&mut backend, &expected, &mut counts));
            assert_eq!(counts.baseline_setups, 1);
            assert_eq!(counts.policy_writes, 0);
            // Present tag under the same live-adapter shape still refuses.
            let mut expected2 = good_facts();
            expected2.bootstrap_tag_establishment = true;
            let mut fresh_present = expected2.clone();
            fresh_present.bootstrap_tag_establishment = false;
            let mut backend2 = FakeBackend::default();
            backend2.exstyle.insert(100, 0);
            backend2.observe_queue.push_back(Some(fresh_present));
            let mut counts2 = EffectCounts::default();
            assert!(!bootstrap_tag(&mut backend2, &expected2, &mut counts2));
            assert_eq!(counts2.baseline_setups, 0);
        }

        #[test]
        fn topmost_at_preimage_restores_marker_only() {
            // Interrupted recovery leftover: topmost already false, marker
            // still at the exact intended effect. Only the marker write runs.
            let title = title_a();
            let marker: u64 = 0xAAAA_AAAA_AAAA_AAAA;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let facts = good_facts();
            let mut backend = FakeBackend::default();
            backend.topmost.insert(100, false);
            backend.tags.insert(100, 0x0123_4567_89ab_cdef);
            backend.markers.insert(100, marker_effect_for(marker));
            backend.exstyle.insert(100, 0);
            backend = scripted(backend, &facts, 3);
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::Restored
            );
            assert_eq!(backend.read_topmost(100), Some(false));
            assert_eq!(backend.read_marker(100), Some(marker));
            assert_eq!(counts.policy_writes, 1);
            assert!(
                backend
                    .writes
                    .iter()
                    .all(|(_, k)| *k != WriteKind::Topmost(true) && *k != WriteKind::Topmost(false))
            );
            assert!(backend.writes.contains(&(100, WriteKind::Marker(marker))));
        }

        #[test]
        fn interrupted_recovery_repeat_reports_partial_then_completes() {
            // First pass: topmost restores, marker pre-write observes a stale
            // tag and refuses. The earlier topmost effect must surface as
            // partial, never as a zero-write refusal.
            let title = title_a();
            let marker: u64 = 0xBBBB_BBBB_BBBB_BBBB;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let facts = good_facts();
            let mut stale = good_facts();
            stale.live_tag = LiveTag::Value(0x9999_8888_7777_6666);
            let mut backend = fake_with_window_live_true();
            backend.topmost.insert(100, true);
            backend.markers.insert(100, marker_effect_for(marker));
            backend.observe_queue.push_back(Some(facts.clone()));
            backend.observe_queue.push_back(Some(facts.clone()));
            backend.observe_queue.push_back(Some(facts.clone()));
            backend.observe_queue.push_back(Some(stale));
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::PartiallyRestored
            );
            assert_eq!(backend.read_topmost(100), Some(false));
            assert_eq!(backend.read_marker(100), Some(marker_effect_for(marker)));
            assert_eq!(counts.policy_writes, 1);
            // Repeat: marker still at the intended effect, topmost already at
            // preimage. Marker-only restore completes.
            let mut backend2 = FakeBackend::default();
            backend2.topmost.insert(100, false);
            backend2.tags.insert(100, 0x0123_4567_89ab_cdef);
            backend2.markers.insert(100, marker_effect_for(marker));
            backend2.exstyle.insert(100, 0);
            backend2 = scripted(backend2, &facts, 3);
            let mut counts2 = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend2, &mut counts2),
                RecoverOutcome::Restored
            );
            assert_eq!(backend2.read_marker(100), Some(marker));
        }

        #[test]
        fn marker_unexpected_drift_preserved_with_zero_marker_writes() {
            // Unknown drift (neither preimage nor exact intended effect) is
            // preserved. Topmost still reconciles independently, so a needed
            // topmost restore surfaces as partial with the marker untouched.
            let title = title_a();
            let marker: u64 = 0xCCCC_CCCC_CCCC_CCCC;
            let record =
                encode_record(&spec(true, Some(false), &title, &marker_bytes(marker))).unwrap();
            let facts = good_facts();
            let mut backend = fake_with_window_live_true();
            backend.topmost.insert(100, true);
            backend.markers.insert(100, 0xDEAD_BEEF_DEAD_BEEF);
            assert_ne!(0xDEAD_BEEF_DEAD_BEEFu64, marker);
            assert_ne!(0xDEAD_BEEF_DEAD_BEEFu64, marker_effect_for(marker));
            backend = scripted(backend, &facts, 3);
            let mut counts = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend, &mut counts),
                RecoverOutcome::PartiallyRestored
            );
            assert_eq!(backend.read_topmost(100), Some(false));
            assert_eq!(backend.read_marker(100), Some(0xDEAD_BEEF_DEAD_BEEF));
            assert!(
                backend
                    .writes
                    .iter()
                    .all(|(_, k)| !matches!(k, WriteKind::Marker(_)))
            );
            // Topmost already at preimage plus unknown drift: zero writes.
            let mut backend2 = FakeBackend::default();
            backend2.topmost.insert(100, false);
            backend2.tags.insert(100, 0x0123_4567_89ab_cdef);
            backend2.markers.insert(100, 0xDEAD_BEEF_DEAD_BEEF);
            backend2.exstyle.insert(100, 0);
            backend2.observe_queue.push_back(Some(facts));
            let mut counts2 = EffectCounts::default();
            assert_eq!(
                recover_from_record(&record, false, &mut backend2, &mut counts2),
                RecoverOutcome::RefusedNoWrites
            );
            assert!(backend2.writes.is_empty());
            assert_eq!(counts2.policy_writes, 0);
        }

        #[test]
        fn frame_within_reports_exact_and_containment() {
            assert!(frame_within([0, 0, 1920, 1080], [0, 0, 1920, 1080]));
            assert!(frame_within([0, 0, 1920, 1080], [100, 100, 500, 500]));
            assert!(!frame_within([0, 0, 1920, 1080], [0, 0, 1921, 1080]));
            assert!(!frame_within([0, 0, 1920, 1080], [-10, 0, 1920, 1080]));
        }

        #[test]
        fn parse_spawn_modes_accepts_known_modes() {
            let parsed = parse_spawn_modes(Some("a=windowed, b=borderless_fullscreen")).unwrap();
            assert_eq!(
                parsed,
                vec![
                    ("a".to_string(), "windowed".to_string()),
                    ("b".to_string(), "borderless_fullscreen".to_string()),
                ]
            );
            assert!(parse_spawn_modes(None).unwrap().is_empty());
            assert!(parse_spawn_modes(Some("a=exclusive_fullscreen")).is_err());
            assert!(parse_spawn_modes(Some("a")).is_err());
            assert!(parse_spawn_modes(Some("=windowed")).is_err());
        }

        #[test]
        fn coverage_verdict_labels_exact_within_and_miss() {
            let fulls = vec![([0, 0, 1920, 1080], true)];
            let verdict = coverage_verdict(&fulls, &Some([0, 0, 1920, 1080]));
            assert_eq!(
                verdict.get("verdict").and_then(|v| v.as_str()),
                Some("covers-monitor-exact")
            );
            let verdict = coverage_verdict(&fulls, &Some([100, 100, 500, 500]));
            assert_eq!(
                verdict.get("verdict").and_then(|v| v.as_str()),
                Some("within-monitor")
            );
            let verdict = coverage_verdict(&fulls, &Some([2000, 0, 2500, 500]));
            assert_eq!(
                verdict.get("verdict").and_then(|v| v.as_str()),
                Some("no-cover")
            );
            let verdict = coverage_verdict(&fulls, &None);
            assert_eq!(
                verdict.get("verdict").and_then(|v| v.as_str()),
                Some("unreadable")
            );
        }

        #[test]
        fn hide_record_roundtrip_and_rejects_bad_body() {
            let title = title_a();
            let marker: u64 = 0xD00D_D00D_D00D_D00D;
            let spec = HideSpec {
                schema: RECORD_SCHEMA,
                fixture_exe: "C:\\probe\\tiler-live-fixture.exe",
                fixture_pid: 4242,
                fixture_creation: "creation-1",
                fixture_user: "S-1-5-21-1",
                fixture_session: 1,
                fixture_integrity: 0x2000,
                caller: "a",
                full_title: &title,
                tag: "0123456789abcdef",
                hwnd: 100,
                preimage_visible: true,
                preimage_show: Some(1),
                preimage_outer: Some([0, 0, 640, 480]),
                preimage_dwm: Some([0, 0, 640, 480]),
                opaque: &marker_bytes(marker),
            };
            let json = encode_hide_record(&spec).unwrap();
            let back = decode_hide_record(&json).unwrap();
            assert_eq!(back.kind, HIDE_RECORD_KIND);
            assert!(back.preimage_visible);
            assert_eq!(back.preimage_outer, Some([0, 0, 640, 480]));
            // Topmost decoder must not accept a hide record and vice versa.
            assert!(decode_record(&json).is_err());
            assert!(
                decode_hide_record(
                    &encode_record(&super::RecordSpec {
                        schema: RECORD_SCHEMA,
                        fixture_exe: "C:\\probe\\tiler-live-fixture.exe",
                        fixture_pid: 4242,
                        fixture_creation: "creation-1",
                        fixture_user: "S-1-5-21-1",
                        fixture_session: 1,
                        fixture_integrity: 0x2000,
                        caller: "a",
                        full_title: &title,
                        tag: "0123456789abcdef",
                        hwnd: 100,
                        effect_topmost: true,
                        preimage_topmost: Some(false),
                        opaque: &marker_bytes(marker),
                    })
                    .unwrap()
                )
                .is_err()
            );
            // Tampered visibility checksum rejects.
            let mut tampered: serde_json::Value = serde_json::from_str(&json).unwrap();
            tampered["preimage_visible"] = serde_json::json!(false);
            assert_eq!(
                decode_hide_record(&serde_json::to_string(&tampered).unwrap()),
                Err(RecordReject::Mismatch)
            );
        }

        #[test]
        fn exe_normalization_ascii_slash_lower_nonnascii_dotsegments() {
            use tiler_windows::lifecycle::exe_paths_equal;
            // Production rule: slash folding + ASCII lower only.
            assert!(exe_paths_equal(
                "C:/Probe/TILER-live-fixture.EXE",
                "c:\\probe\\tiler-live-fixture.exe"
            ));
            // Non-ASCII case is preserved (no match forced across cases the
            // ASCII fold does not cover).
            assert!(!exe_paths_equal(
                "C:\\app\\\u{00E9}X.exe",
                "C:\\app\\\u{00C9}X.exe"
            ));
            assert!(exe_paths_equal(
                "C:\\app\\\u{00E9}X.exe",
                "C:\\app\\\u{00E9}X.exe"
            ));
            // Dot-segments are never resolved.
            assert!(!exe_paths_equal(
                "C:\\app\\sub\\..\\app.exe",
                "C:\\app\\app.exe"
            ));
        }

        fn k5_sample(dt_ms: f64, rect: Option<[i32; 4]>) -> K5Sample {
            K5Sample {
                dt_ms,
                tick: 0,
                rect,
            }
        }

        fn k5_cb(dt_ms: f64, event: u32) -> K5Callback {
            K5Callback {
                dt_ms,
                tick: 0,
                event,
                hwnd: 7,
                id_object: 0,
                thread: 1,
                time: 0,
            }
        }

        #[test]
        fn k5_transitions_detect_changes_and_skip_unreadable() {
            let samples = vec![
                k5_sample(0.0, Some([0, 0, 100, 100])),
                k5_sample(5.0, Some([0, 0, 100, 100])),
                k5_sample(10.0, Some([10, 10, 110, 110])),
                k5_sample(15.0, None),
                k5_sample(20.0, Some([10, 10, 110, 110])),
                k5_sample(25.0, Some([20, 20, 120, 120])),
            ];
            assert_eq!(k5_transitions(&samples), vec![(1, 2), (4, 5)]);
        }

        #[test]
        fn k5_bracket_labels_covered_ambiguous_uncovered() {
            let samples = vec![
                k5_sample(0.0, Some([0, 0, 100, 100])),
                k5_sample(10.0, Some([5, 5, 105, 105])),
                k5_sample(20.0, Some([5, 5, 105, 105])),
                k5_sample(30.0, Some([9, 9, 109, 109])),
                k5_sample(40.0, Some([9, 9, 109, 109])),
                k5_sample(50.0, Some([12, 12, 112, 112])),
            ];
            let transitions = k5_transitions(&samples);
            assert_eq!(transitions, vec![(0, 1), (2, 3), (4, 5)]);
            let callbacks = vec![k5_cb(10.0, K5_EVENT_LOCATIONCHANGE), k5_cb(30.0, 0x8002)];
            let (covered, uncovered, ambiguous, detail) =
                k5_bracket(&samples, &transitions, &callbacks, 5.0);
            assert_eq!((covered, uncovered, ambiguous), (1, 1, 1));
            let verdicts: Vec<&str> = detail.iter().map(|(_, _, v)| *v).collect();
            assert_eq!(verdicts, vec!["covered", "ambiguous", "uncovered"]);
        }

        #[test]
        fn k5_next_sample_lag_uses_first_sample_at_or_after() {
            let samples = vec![
                k5_sample(0.0, Some([0, 0, 10, 10])),
                k5_sample(5.0, Some([0, 0, 10, 10])),
                k5_sample(12.0, Some([1, 1, 11, 11])),
            ];
            let callbacks = vec![k5_cb(3.0, K5_EVENT_LOCATIONCHANGE), k5_cb(12.0, 0x000B)];
            let lags = k5_next_sample_lags(&callbacks, &samples);
            assert_eq!(lags, vec![2.0, 0.0]);
            // Past-the-end callbacks are dropped, never extrapolated.
            let late = vec![k5_cb(99.0, K5_EVENT_LOCATIONCHANGE)];
            assert!(k5_next_sample_lags(&late, &samples).is_empty());
        }

        #[test]
        fn k5_resize_classifier_covers_borders_and_corners_only() {
            for code in [10, 11, 12, 13, 14, 15, 16, 17] {
                assert!(k5_ht_is_resize(code), "HT{code} must classify resize");
            }
            for code in [0, 1, 2, 3, 8, 9, 18, 20, 0xFFFF] {
                assert!(!k5_ht_is_resize(code), "HT{code} must not classify resize");
            }
            assert!(k5_style_resizable(K5_WS_THICKFRAME));
            assert!(k5_style_resizable(K5_WS_THICKFRAME | 0x00CB_0000));
            assert!(!k5_style_resizable(0x00CB_0000));
            assert!(!k5_style_resizable(0));
        }
    }
}
