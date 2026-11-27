#![forbid(unsafe_code)]

//! Owned-Space destination planning and post-only owned-Space establishment
//! for `fixture-leg`.
//!
//! `fixture-leg` never posts on a user Space. Numbered and relative
//! space-mutating chords address appended ordinary destinations planned here
//! under the Desktop-9 holder mapping: with a baseline row of `n` native
//! Spaces the controller retains `n + 1..`, and a chord that names a user
//! ordinal, Desktop 10, or an unmappable position refuses before any effect.
//!
//! A post-only leg creates exactly one owned Space, one append past the fresh
//! starting-display row, proves the switch (current SID, observed ordinal,
//! exactly one append), and records IDs and ordinals only, never titles.
//! Registration of the created Space precedes every later refusal, so
//! teardown removes exactly what this leg created.
//!
//! The occupancy inventory behind the `n` audit fact is read-only: one CG/SLS
//! membership read per window, no synthetic posts, no writes. Fixture-scope
//! auxiliary windows are excluded (this leg's own scaffolding is not
//! occupancy), and an unreadable non-fixture membership degrades to the
//! explicit row-length upper bound, counted and named in `n_kind`, rather than
//! guessing where that window sits.
//!
//! The placement proof is bounded and re-checked: every joined fixture window
//! must be readable on the owned Space before the product launches and again
//! immediately before any chord posts, and a refusal names each wid with its
//! observed membership or read failure.

use crate::fixture_leg::FIXTURE_SCOPE;
use crate::live_leg::{AdmissionScope, LiveLeg, LiveLegTransport, required_scope};
use crate::space_probe::NativeSpaces;

/// Static Desktop-N ordinal for one space-mutating chord (`select-N` /
/// `send-N`: `0` is Desktop 10; relative/previous chords have no static
/// ordinal). Pure; unit-tested.
fn static_chord_ordinal(chord: &str) -> Option<u32> {
    let rest = chord
        .strip_prefix("select-")
        .or_else(|| chord.strip_prefix("send-"))?;
    if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
        let index = u32::from(rest.as_bytes()[0] - b'0');
        Some(if index == 0 { 10 } else { index })
    } else {
        None
    }
}

/// Plan the retained owned destinations for space-mutating chords: with a
/// baseline row of `baseline_count` native Spaces, static numbered chords
/// must address appended ordinary positions (`baseline_count + 1..`), never
/// user Spaces, and every position must stay within the Desktop-9 holder
/// mapping (position 10+ is unmappable: the send path refuses past
/// Desktop-9). Returns the owned ordinal range to append (count). Pure;
/// unit-tested.
pub(crate) fn plan_owned_destinations(
    chords: &[String],
    baseline_count: usize,
) -> Result<std::ops::RangeInclusive<usize>, String> {
    if baseline_count == 0 {
        return Err(
            "fixture-leg: baseline native row is empty; refusing without owned destinations"
                .to_owned(),
        );
    }
    if baseline_count >= 9 {
        return Err(format!(
            "fixture-leg: {baseline_count} user Spaces leave no representable ordinary target (holders map 1..=9); refusing instead of using user Spaces"
        ));
    }
    let mut static_ordinals: Vec<usize> = Vec::new();
    for chord in chords {
        if required_scope(chord) != Some(AdmissionScope::SpaceMutating) {
            continue;
        }
        match static_chord_ordinal(chord) {
            Some(10) => {
                return Err(format!(
                    "fixture-leg: chord '{chord}' names Desktop 10 (trailing creation is controller-owned); refusing"
                ));
            }
            Some(ordinal) => static_ordinals.push(ordinal as usize),
            // Relative/previous chords address no static ordinal: the
            // actuator gate still protects every dispatch at runtime.
            None => {}
        }
    }
    static_ordinals.sort_unstable();
    static_ordinals.dedup();
    for ordinal in &static_ordinals {
        if *ordinal <= baseline_count {
            return Err(format!(
                "fixture-leg: chord addresses user-Space ordinal {ordinal} (owned range starts at {}); refusing instead of using user Spaces",
                baseline_count + 1
            ));
        }
        if *ordinal > 9 {
            return Err(format!(
                "fixture-leg: chord ordinal {ordinal} is past Desktop-9 (unmappable); refusing"
            ));
        }
    }
    let max_ordinal = static_ordinals.into_iter().max();
    // Relative-only (or post-only, though this plans only space-mutating
    // legs) still retains a bounded pair of destinations for the latency
    // plan; static chords retain exactly up to their max ordinal.
    let top = max_ordinal.unwrap_or((baseline_count + 2).min(9));
    if top > 9 {
        return Err(format!(
            "fixture-leg: owned range would exceed Desktop-9 (baseline {baseline_count}); refusing"
        ));
    }
    if top <= baseline_count {
        return Err(format!(
            "fixture-leg: chord ordinals name only user Spaces (baseline {baseline_count}); refusing"
        ));
    }
    Ok((baseline_count + 1)..=top)
}

/// One-send arm pin: the exact controller-owned source and distinct owned
/// target SIDs plus the target's Desktop-N ordinal (1-based native row
/// position, which names both the send-follow chord digit and the held
/// holder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OneSendPin {
    pub(crate) source: u64,
    pub(crate) target: u64,
    pub(crate) desktop: u32,
    pub(crate) send_chord: String,
}

impl OneSendPin {
    pub(crate) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "source_sid": self.source,
            "target_sid": self.target,
            "target_desktop": self.desktop,
            "send_chord": self.send_chord,
        })
    }
}

/// Pin the one-send arm's exact owned source and distinct owned target on
/// the fresh starting-display row. The source must be the controller-owned
/// Space that is currently frontmost; the target must be a different
/// controller-owned SID present exactly once in the row at a 1-based
/// position within the Desktop-9 holder mapping. The send-follow chord is
/// derived from that observed position, never from a hardcoded user
/// ordinal. Pure; unit-tested. Refuses before any spawn or post effect.
pub(crate) fn pin_one_send(
    row: &crate::space_probe::SpaceTopologyDisplay,
    owned: &[u64],
    source: u64,
    target: u64,
) -> Result<OneSendPin, String> {
    if source == 0 || target == 0 {
        return Err("one-send: zero source/target SID; refusing".to_owned());
    }
    if source == target {
        return Err(format!(
            "one-send: pinned target SID {target} is not distinct from the pinned source; refusing"
        ));
    }
    if row.current != source {
        return Err(format!(
            "one-send: pinned source SID {source} is not the current Space (observed {}); refusing",
            row.current
        ));
    }
    if !owned.contains(&source) {
        return Err(format!(
            "one-send: pinned source SID {source} is not controller-owned; refusing"
        ));
    }
    if !owned.contains(&target) {
        return Err(format!(
            "one-send: pinned target SID {target} is not controller-owned; refusing"
        ));
    }
    let occurrences = row.spaces.iter().filter(|sid| **sid == target).count();
    if occurrences != 1 {
        return Err(format!(
            "one-send: pinned target SID {target} appears {occurrences} times in the fresh row; refusing"
        ));
    }
    let target_ordinal = row
        .spaces
        .iter()
        .position(|sid| *sid == target)
        .map(|index| index + 1)
        .expect("target occurrence proved above");
    if target_ordinal > 9 {
        return Err(format!(
            "one-send: pinned target ordinal {target_ordinal} is past Desktop-9 (unmappable); refusing"
        ));
    }
    Ok(OneSendPin {
        source,
        target,
        desktop: target_ordinal as u32,
        send_chord: format!("send-{target_ordinal}"),
    })
}

/// Fresh occupancy inventory for the post-only owned-Space path: every
/// non-fixture window whose membership includes a starting-display SID maps
/// to that SID's 1-based ordinal within the row, and `n` is the highest such
/// ordinal. Fixture-scope windows are auxiliary and never occupancy.
/// Unreadable non-fixture membership is never guessed at: `n` degrades to
/// the full row length (an explicit audit upper bound) and the unreadable
/// count is recorded. Read-only; IDs and ordinals only.
fn post_only_occupancy<'a>(
    spaces_before: &[u64],
    members: impl IntoIterator<Item = (&'a str, &'a Result<Vec<u64>, String>)>,
) -> PostOnlyOccupancy {
    let mut occupied = 0;
    let mut unreadable = 0;
    for (bundle, spaces) in members {
        if bundle == FIXTURE_SCOPE {
            continue;
        }
        match spaces {
            Ok(spaces) => {
                for sid in spaces {
                    if let Some(index) = spaces_before.iter().position(|candidate| candidate == sid)
                    {
                        occupied = occupied.max(index + 1);
                    }
                }
            }
            Err(_) => unreadable += 1,
        }
    }
    if unreadable > 0 {
        PostOnlyOccupancy {
            n: spaces_before.len(),
            n_kind: "upper_bound",
            n_unreadable_windows: unreadable,
        }
    } else {
        PostOnlyOccupancy {
            n: occupied,
            n_kind: "occupied",
            n_unreadable_windows: 0,
        }
    }
}

/// Plan the single post-only owned Space: exactly one append after the
/// fresh starting-display row, refused when that ordinal would leave the
/// Desktop-9 holder mapping (`spaces_before.len() >= 9`, the same ceiling
/// [`plan_owned_destinations`] applies to appended destinations; position
/// 10+ is unmappable by the send path).
fn post_only_owned_ordinal(spaces_before: &[u64]) -> Result<usize, String> {
    let ordinal = spaces_before.len() + 1;
    if ordinal > 9 {
        return Err(format!(
            "fixture-leg: {} user Spaces leave no representable post-only owned Space (holders map 1..=9); refusing instead of using user Spaces",
            spaces_before.len()
        ));
    }
    Ok(ordinal)
}

/// Fresh occupancy inventory result (see [`post_only_occupancy`]).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PostOnlyOccupancy {
    n: usize,
    n_kind: &'static str,
    n_unreadable_windows: usize,
}

/// Established post-only owned-Space evidence: the starting display, the
/// fresh occupancy inventory, the created SID at its append ordinal, the
/// recorded starting SID, and the row before/after. IDs and ordinals only,
/// no titles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostOnlyOwnedSpace {
    display: usize,
    n: usize,
    n_kind: &'static str,
    n_unreadable_windows: usize,
    sid: u64,
    ordinal: usize,
    starting_sid: u64,
    spaces_before: Vec<u64>,
    spaces_after: Vec<u64>,
}

impl PostOnlyOwnedSpace {
    pub(crate) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "display": self.display,
            "n": self.n,
            "n_kind": self.n_kind,
            "n_unreadable_windows": self.n_unreadable_windows,
            "sid": self.sid,
            "ordinal": self.ordinal,
            "starting_sid": self.starting_sid,
            "spaces_before": self.spaces_before,
            "spaces_after": self.spaces_after,
        })
    }

    /// The created owned Space's SID: what the placement checks compare the
    /// current Space and every joined window's membership against.
    pub(crate) fn sid(&self) -> u64 {
        self.sid
    }

    /// Starting display's current SID recorded before the append.
    pub(crate) fn starting_sid(&self) -> u64 {
        self.starting_sid
    }

    /// The starting display's exact row after the append, used by the
    /// two-launch controller to refuse before its second product launch.
    pub(crate) fn spaces_after(&self) -> &[u64] {
        &self.spaces_after
    }
}

/// Bounded settle for the post-only occupancy proof after the window join
/// (milliseconds): the join already proved CG existence, so only the SLS
/// membership read can lag.
const POST_ONLY_PLACEMENT_POLL_MS: u64 = 2_000;

/// One joined window's fresh membership read: the wid plus either readable
/// SIDs or the read failure (never a guess).
#[cfg(target_os = "macos")]
pub(crate) type WindowMembership = (u32, Result<Vec<u64>, String>);

/// One fresh membership read per joined window: `Ok(spaces)` when readable,
/// `Err` with the read failure otherwise. A window absent from the
/// inventory is unreadable, never assumed placed.
#[cfg(target_os = "macos")]
fn joined_window_memberships(wids: &[u32]) -> Vec<WindowMembership> {
    match crate::mc_observe::enumerate_window_members() {
        Ok(members) => {
            let mut by_wid: std::collections::HashMap<u32, Result<Vec<u64>, String>> = members
                .into_iter()
                .map(|member| (member.wid, member.spaces))
                .collect();
            wids.iter()
                .map(|wid| {
                    let spaces = by_wid.remove(wid).unwrap_or_else(|| {
                        Err("window absent from the fresh CG/SLS inventory".to_owned())
                    });
                    (*wid, spaces)
                })
                .collect()
        }
        Err(error) => wids
            .iter()
            .map(|wid| {
                (
                    *wid,
                    Err(format!("CG/SLS membership inventory unreadable ({error})")),
                )
            })
            .collect(),
    }
}

/// Bounded poll proving every joined window is readable on the owned Space.
/// Returns the last observed memberships when the deadline passes, so the
/// refusal names each wid and what was read (never a guess).
#[cfg(target_os = "macos")]
pub(crate) fn await_joined_on_owned_space(
    joined: &[(String, u32)],
    owned_sid: u64,
) -> Result<(), Vec<WindowMembership>> {
    let wids: Vec<u32> = joined.iter().map(|(_, wid)| *wid).collect();
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(POST_ONLY_PLACEMENT_POLL_MS);
    loop {
        let observed = joined_window_memberships(&wids);
        if observed.iter().all(|(_, spaces)| {
            spaces
                .as_ref()
                .is_ok_and(|spaces| spaces.contains(&owned_sid))
        }) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(observed);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Refusal body for the post-only placement checks: the established
/// owned-Space evidence plus one entry per joined window (wid with its
/// observed membership or read failure; no titles).
#[cfg(target_os = "macos")]
pub(crate) fn post_only_placement_refusal(
    evidence: &PostOnlyOwnedSpace,
    observed: &[WindowMembership],
    error: &str,
    current_owned_space: Option<bool>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "stage": "post-only-owned-space",
        "error": error,
        "owned_space": evidence.evidence(),
        "windows": observed
            .iter()
            .map(|(wid, spaces)| match spaces {
                Ok(spaces) => serde_json::json!({"wid": wid, "spaces": spaces}),
                Err(read_error) => serde_json::json!({"wid": wid, "membership_error": read_error}),
            })
            .collect::<Vec<_>>(),
    });
    if let Some(current) = current_owned_space {
        body["current_owned_space"] = current.into();
    }
    body
}

/// One post-only owned-Space establishment: the created SID, the starting
/// display's current SID it was appended to, and the recorded evidence.
pub(crate) struct EstablishedOwnedSpace {
    pub(crate) sid: u64,
    pub(crate) starting_sid: u64,
    pub(crate) evidence: PostOnlyOwnedSpace,
}

/// Establish the single post-only owned Space: prove the return route, read
/// the fresh starting-display row and the read-only occupancy inventory,
/// append exactly one Space at the planned ordinal, register it for owned
/// teardown before any later refusal, switch to it, and prove the switch
/// landed at that ordinal.
#[cfg(target_os = "macos")]
pub(crate) fn establish_post_only_owned_space(
    leg: &mut LiveLeg<LiveLegTransport>,
) -> Result<EstablishedOwnedSpace, (i32, String)> {
    establish_post_only_owned_space_on(leg, 0)
}

#[cfg(target_os = "macos")]
pub(crate) fn establish_post_only_owned_space_on(
    leg: &mut LiveLeg<LiveLegTransport>,
    display: usize,
) -> Result<EstablishedOwnedSpace, (i32, String)> {
    leg.prove_return_on(display)?;
    let before = leg.transport_mut().space_snapshot().map_err(|e| {
        (
            1,
            format!("fixture-leg: post-only starting snapshot unreadable ({e})"),
        )
    })?;
    let row = before.displays.get(display).ok_or_else(|| {
        (
            1,
            "fixture-leg: post-only starting snapshot has no display; refusing".to_owned(),
        )
    })?;
    let spaces_before = row.spaces.clone();
    let ordinal = post_only_owned_ordinal(&spaces_before).map_err(|e| (2, e))?;
    // Fresh occupancy inventory, read-only: no synthetic posts, no
    // writes. Fixture auxiliary windows are excluded; unreadable
    // non-fixture membership degrades to the explicit row upper bound.
    let members = crate::mc_observe::enumerate_window_members().map_err(|e| {
        (
            1,
            format!("fixture-leg: post-only occupancy inventory unreadable ({e})"),
        )
    })?;
    let occupancy = post_only_occupancy(
        &spaces_before,
        members
            .iter()
            .map(|member| (member.bundle.as_str(), &member.spaces)),
    );
    let created = leg.transport_mut().space_create(display).map_err(|e| {
        (
            1,
            format!("fixture-leg: post-only owned Space creation refused ({e})"),
        )
    })?;
    let sid = created.sid;
    // Register before any later refusal so teardown removes exactly this
    // owned Space.
    leg.note_owned_space(display, sid);
    if let Some(shortfall) = created.mc.shortfall_text("post-only owned Space create") {
        return Err((
            1,
            format!("fixture-leg: post-only owned Space creation: {shortfall}"),
        ));
    }
    leg.transport_mut().space_goto(display, sid).map_err(|e| {
        (
            1,
            format!("fixture-leg: post-only owned Space switch refused ({e})"),
        )
    })?;
    let after = leg.transport_mut().space_snapshot().map_err(|e| {
        (
            1,
            format!("fixture-leg: post-only owned Space switch unverified ({e})"),
        )
    })?;
    let after_row = after.displays.get(display).ok_or_else(|| {
        (
            1,
            "fixture-leg: post-only switch snapshot has no display; refusing".to_owned(),
        )
    })?;
    let evidence = PostOnlyOwnedSpace {
        display,
        n: occupancy.n,
        n_kind: occupancy.n_kind,
        n_unreadable_windows: occupancy.n_unreadable_windows,
        sid,
        ordinal,
        starting_sid: row.current,
        spaces_before,
        spaces_after: after_row.spaces.clone(),
    };
    let observed_ordinal = after_row
        .spaces
        .iter()
        .position(|candidate| *candidate == sid)
        .map(|index| index + 1);
    // The switch must land on the owned SID at exactly one append past
    // the fresh row; anything else refuses (teardown restores/removes).
    if after_row.current != sid
        || observed_ordinal != Some(ordinal)
        || after_row.spaces.len() != evidence.spaces_before.len() + 1
    {
        return Err((
            1,
            serde_json::json!({
                "stage": "post-only-owned-space",
                "error": format!(
                    "post-only owned Space switch unverified (current {}, observed ordinal {:?}, expected ordinal {ordinal} after exactly one append)",
                    after_row.current, observed_ordinal
                ),
                "owned_space": evidence.evidence(),
            })
            .to_string(),
        ));
    }
    Ok(EstablishedOwnedSpace {
        sid,
        starting_sid: row.current,
        evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_destination_planning_never_targets_user_spaces() {
        let chords = |names: Vec<&str>| names.into_iter().map(str::to_owned).collect::<Vec<_>>();
        // Baseline of two: static chords at 3..=4 retain exactly those.
        assert_eq!(
            plan_owned_destinations(&chords(vec!["select-3", "send-4"]), 2).expect("plans"),
            3..=4
        );
        // A chord naming a user-Space ordinal refuses planning (instead of
        // using user Spaces at runtime).
        assert!(plan_owned_destinations(&chords(vec!["select-1"]), 2).is_err());
        assert!(plan_owned_destinations(&chords(vec!["send-2"]), 2).is_err());
        // Nine user Spaces leave no representable ordinary target.
        assert!(plan_owned_destinations(&chords(vec!["rel-next"]), 9).is_err());
        assert!(plan_owned_destinations(&chords(vec!["select-3"]), 9).is_err());
        // Desktop-10 (trailing) is controller-owned creation: refused.
        assert!(plan_owned_destinations(&chords(vec!["select-0"]), 2).is_err());
        assert!(plan_owned_destinations(&chords(vec!["send-0"]), 2).is_err());
        // Relative-only legs still retain a bounded pair (actuator-gated).
        assert_eq!(
            plan_owned_destinations(&chords(vec!["rel-prev", "rel-next"]), 2).expect("plans"),
            3..=4
        );
        // Static ordinals must fit Desktop-9.
        assert!(plan_owned_destinations(&chords(vec!["select-3"]), 8).is_err());
        // Empty baseline refuses.
        assert!(plan_owned_destinations(&chords(vec!["select-3"]), 0).is_err());
        // Static ordinal mapping: digits map 1:1, trailing 0 is Desktop 10.
        assert_eq!(static_chord_ordinal("select-3"), Some(3));
        assert_eq!(static_chord_ordinal("send-1"), Some(1));
        assert_eq!(static_chord_ordinal("select-0"), Some(10));
        assert_eq!(static_chord_ordinal("rel-prev"), None);
        assert_eq!(static_chord_ordinal("focus-h"), None);
    }

    #[test]
    fn post_only_occupancy_excludes_fixture_windows_and_bounds_unreadable() {
        let spaces_before = [10, 11, 12];
        let readable = |sids: &[u64]| Ok::<Vec<u64>, String>(sids.to_vec());
        let unreadable = Err::<Vec<u64>, String>("SLS read refused".to_owned());
        // Windows map to the starting row's 1-based ordinal; the max is n.
        let occupancy = post_only_occupancy(
            &spaces_before,
            [
                ("com.apple.Safari", &readable(&[10])),
                ("com.apple.Terminal", &readable(&[11, 12])),
            ],
        );
        assert_eq!(
            occupancy,
            PostOnlyOccupancy {
                n: 3,
                n_kind: "occupied",
                n_unreadable_windows: 0,
            }
        );
        // A window on a SID outside the starting row is not occupancy.
        let occupancy =
            post_only_occupancy(&spaces_before, [("com.apple.Safari", &readable(&[99]))]);
        assert_eq!(occupancy.n, 0);
        // Fixture auxiliary windows never count as occupancy, even when
        // their membership read failed.
        let occupancy = post_only_occupancy(
            &spaces_before,
            [
                (FIXTURE_SCOPE, &unreadable),
                (FIXTURE_SCOPE, &readable(&[12])),
            ],
        );
        assert_eq!(
            occupancy,
            PostOnlyOccupancy {
                n: 0,
                n_kind: "occupied",
                n_unreadable_windows: 0,
            }
        );
        // Any unreadable non-fixture membership degrades to the explicit
        // row-length upper bound and is counted, never guessed at.
        let occupancy = post_only_occupancy(
            &spaces_before,
            [
                ("com.apple.Safari", &readable(&[10])),
                ("com.apple.Terminal", &unreadable),
            ],
        );
        assert_eq!(
            occupancy,
            PostOnlyOccupancy {
                n: 3,
                n_kind: "upper_bound",
                n_unreadable_windows: 1,
            }
        );
    }

    #[test]
    fn post_only_owned_ordinal_is_one_append_within_desktop_nine() {
        // Exactly one append past the fresh row.
        assert_eq!(post_only_owned_ordinal(&[]).expect("empty row"), 1);
        assert_eq!(post_only_owned_ordinal(&[10]).expect("one Space"), 2);
        assert_eq!(
            post_only_owned_ordinal(&[10, 11, 12, 13, 14, 15, 16, 17]).expect("eight Spaces"),
            9
        );
        // Position 10+ is unmappable by the holders: the same ceiling
        // plan_owned_destinations applies to appended destinations.
        let nine = [10, 11, 12, 13, 14, 15, 16, 17, 18];
        assert!(post_only_owned_ordinal(&nine).is_err());
        let no_chords: Vec<String> = Vec::new();
        assert!(plan_owned_destinations(&no_chords, 9).is_err());
    }

    #[test]
    fn one_send_pin_refuses_unowned_source_and_non_distinct_target_before_effects() {
        // A fresh row with one user baseline Space plus the controller's
        // owned source (current) and distinct owned target.
        let row = crate::space_probe::SpaceTopologyDisplay {
            spaces: vec![11, 101, 102],
            current: 101,
        };
        let owned = [101, 102];
        // Source admitted only when current and controller-owned.
        assert!(pin_one_send(&row, &owned, 101, 102).is_ok());
        assert!(pin_one_send(&row, &[102], 101, 102).is_err());
        assert!(pin_one_send(&row, &owned, 11, 102).is_err());
        // The target must be distinct from the pinned source.
        let err = pin_one_send(&row, &owned, 101, 101).unwrap_err();
        assert!(err.contains("not distinct"), "{err}");
        // Unowned and absent targets refuse too.
        assert!(pin_one_send(&row, &owned, 101, 11).is_err());
        assert!(pin_one_send(&row, &owned, 101, 999).is_err());
        // Past Desktop-9 is unmappable, never guessed.
        let ten = crate::space_probe::SpaceTopologyDisplay {
            spaces: (1..=10).collect(),
            current: 1,
        };
        assert!(pin_one_send(&ten, &[1, 10], 1, 10).is_err());
        // The pinned chord digit comes from the observed target position,
        // never a hardcoded user-Space ordinal.
        let pin = pin_one_send(&row, &owned, 101, 102).expect("pins");
        assert_eq!(pin.source, 101);
        assert_eq!(pin.target, 102);
        assert_eq!(pin.desktop, 3);
        assert_eq!(pin.send_chord, "send-3");
        // Then the existing gate admits exactly the two owned SIDs (never
        // the user baseline) before any product launch.
        let gate = crate::fixture_gate::FixtureSpaceGate::new(&owned, None).expect("gate builds");
        assert!(gate.allows(pin.source));
        assert!(gate.allows(pin.target));
        assert!(!gate.allows(11));
    }

    #[test]
    fn established_owned_space_evidence_is_ids_and_ordinals() {
        let evidence = serde_json::json!({
            "display": 0,
            "n": 2,
            "n_kind": "occupied",
            "n_unreadable_windows": 0,
            "sid": 9001,
            "ordinal": 4,
            "starting_sid": 11,
            "spaces_before": [11, 12, 13],
            "spaces_after": [11, 12, 13, 9001],
        });
        let built = PostOnlyOwnedSpace {
            display: 0,
            n: 2,
            n_kind: "occupied",
            n_unreadable_windows: 0,
            sid: 9001,
            ordinal: 4,
            starting_sid: 11,
            spaces_before: vec![11, 12, 13],
            spaces_after: vec![11, 12, 13, 9001],
        };
        assert_eq!(built.evidence(), evidence);
    }
}
