#![forbid(unsafe_code)]

//! Receipt-bound recovery of one proven owned Space whose only occupants are
//! positively authenticated system surfaces.
//!
//! A stopped owned-Space leg can leave its created Desktop behind when a
//! Mission Control chrome window (for example an offscreen WindowManager
//! backing row at layer 0) blocks the default emptiness gate. That default
//! gate stays untouched: this path never widens [`crate::mc_observe::verify_space_empty`]
//! or [`crate::mc_probe::classify_window_full`]. Instead it authorizes one
//! exact removal only when every one of these independent facts is re-read
//! live immediately before the press:
//!
//! 1. the retained receipt set (private `live-leg-baseline.json` plus its
//!    beside receipt `live-leg-residue.json`) records the exact target SID
//!    as an owned Space whose removal was refused, and the target is absent
//!    from the baseline preimage;
//! 2. stripping only receipt-recorded owned SIDs from the fresh SLS rows
//!    recovers exactly the preimage's order and currents; the target is
//!    present on exactly one row and is not current;
//! 3. a fresh `SLSSpaceGetType` read proves the target is a User Space;
//! 4. a fresh title-free CG/SLS census proves each target member is
//!    authenticated offscreen system chrome or the exact Finder desktop-
//!    icon layer. All memberships must be readable and target members
//!    must belong only to fresh User Spaces.
//!
//! The retained receipt supplies the owned SIDs and their common preimage.
//! It is never treated as live evidence, and no read here is cached across
//! the removal dispatch.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::live_baseline::{LiveBaseline, NativeBaseline};
use crate::mc_probe::{McError, TopologySnapshot};

/// Exact WindowManager bundle identity for the offscreen desktop backing.
pub const WINDOWMANAGER_BUNDLE_ID: &str = "com.apple.WindowManager";
/// Exact WindowManager executable for that backing.
pub const WINDOWMANAGER_EXE: &str =
    "/System/Library/CoreServices/WindowManager.app/Contents/MacOS/WindowManager";
/// Exact Finder bundle identity for the desktop surface.
pub const FINDER_BUNDLE_ID: &str = "com.apple.finder";
/// Exact Finder executable for that surface.
pub const FINDER_EXE: &str = "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder";
const FINDER_DESKTOP_LAYER: i32 = -2_147_483_603;
/// Exact Dock executable (never a bundle-only match).
pub const DOCK_EXE: &str = "/System/Library/CoreServices/Dock.app/Contents/MacOS/Dock";

/// Prefix this codebase writes for a refused owned-Space removal
/// (`live_leg` teardown step 5). The retained residue artifact is the only
/// persisted record of the in-memory owned-Space list.
pub const OWNED_SPACE_RESIDUE_PREFIX: &str = "owned Space ";

/// One plain-data CG/SLS row for the recovery membership gate. Mirrors the
/// title-free fields [`crate::mc_observe::enumerate_window_members`] emits so
/// captured evidence can be replayed offline byte-for-byte. `spaces` is
/// `None` when the membership read was unreadable: unknown always blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryMemberRow {
    pub wid: u32,
    pub pid: u32,
    pub bundle: String,
    pub layer: i32,
    pub onscreen: bool,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub exe: Option<String>,
    #[serde(default)]
    pub exe_alias: Option<String>,
    #[serde(default)]
    pub ax_windows_absent: Option<bool>,
    /// `None` means unreadable (never an empty membership).
    #[serde(default)]
    pub spaces: Option<Vec<u64>>,
}

/// One positively authenticated system surface on the target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthenticatedMember {
    pub wid: u32,
    pub pid: u32,
    pub bundle: String,
    pub layer: i32,
    pub class: String,
    pub detail: String,
}

/// Outcome of the membership gate: the audit evidence plus how many rows
/// carried a readable membership (including rows on other Spaces).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthenticatedMembers {
    pub checked: usize,
    pub members: Vec<AuthenticatedMember>,
}

/// One removal target within the retained receipt's created-SID set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedSpaceReceipt {
    pub sid: u64,
    pub created: Vec<u64>,
    pub preimage: NativeBaseline,
}

/// The unique target binding proven against the fresh topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OwnedSpaceBinding {
    pub display: usize,
    pub sid: u64,
}

/// Full pre-dispatch proof: the located display, the live membership audit,
/// and how many rows carried a readable membership. Carries evidence only
/// (IDs, identity, class), never titles. The ownership claim itself lives
/// with the caller: the retained receipt preimage for the manual route, or
/// the `LiveLeg` in-memory created-Space registration for active teardown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OwnedSpaceRecoveryProof {
    pub display: usize,
    pub sid: u64,
    pub current: u64,
    pub checked_windows: usize,
    pub members: Vec<AuthenticatedMember>,
}

/// Distinct nonzero owned SIDs named by the retained residue artifact's
/// `residue` strings. This is the only persisted record of the leg's
/// in-memory owned-Space list; anything else is not an authorization.
#[must_use]
pub fn owned_sids_in_residue(residue: &serde_json::Value) -> Vec<u64> {
    let mut sids = Vec::new();
    let Some(entries) = residue.get("residue").and_then(serde_json::Value::as_array) else {
        return sids;
    };
    for entry in entries {
        let Some(text) = entry.as_str() else {
            continue;
        };
        let Some(rest) = text.strip_prefix(OWNED_SPACE_RESIDUE_PREFIX) else {
            continue;
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(sid) = digits.parse::<u64>() else {
            continue;
        };
        if sid != 0 && !sids.contains(&sid) {
            sids.push(sid);
        }
    }
    sids
}

/// Build the receipt authorization from the retained receipt set. Requires
/// the residue to retain the receipt and name the exact target SID, and the
/// baseline preimage to exclude all recorded created SIDs. Positional rows
/// must match their indexes with a readable current in each row.
pub fn receipt_from_persisted(
    baseline: &LiveBaseline,
    residue: &serde_json::Value,
    space: u64,
) -> Result<OwnedSpaceReceipt, String> {
    if space == 0 {
        return Err("owned-recovery: a nonzero target SID is required".to_owned());
    }
    if baseline.native.displays.is_empty() {
        return Err("owned-recovery: the receipt preimage has no displays; refusing".to_owned());
    }
    if residue.get("receipt_retained") != Some(&serde_json::Value::Bool(true)) {
        return Err(
            "owned-recovery: the residue artifact does not retain the receipt; no proven owned SID — refusing"
                .to_owned(),
        );
    }
    let named = owned_sids_in_residue(residue);
    if !named.contains(&space) {
        return Err(format!(
            "owned-recovery: the retained residue names no owned Space {space} (named: {named:?}); an unrecorded SID is never authorized — refusing"
        ));
    }
    for (index, display) in baseline.native.displays.iter().enumerate() {
        if display.index != index {
            return Err(format!(
                "owned-recovery: receipt display at position {index} records index {}; non-positional preimage — refusing",
                display.index
            ));
        }
        if !display.spaces.contains(&display.current) {
            return Err(format!(
                "owned-recovery: receipt display {index} current {} is not in its Space set {:?}; refusing",
                display.current, display.spaces
            ));
        }
        if let Some(sid) = display.spaces.iter().find(|sid| named.contains(sid)) {
            return Err(format!(
                "owned-recovery: recorded Space {sid} is already in the receipt preimage; refusing"
            ));
        }
    }
    Ok(OwnedSpaceReceipt {
        sid: space,
        created: named,
        preimage: baseline.native.clone(),
    })
}

/// Read the canonical retained receipt set and build the authorization for
/// `space`. Refuses symlinked/non-regular/corrupt artifacts; the residue
/// must sit beside the baseline receipt in the private recovery directory.
pub fn load_persisted_owned_receipt(space: u64) -> Result<(OwnedSpaceReceipt, LiveBaseline), String> {
    let baseline_path = crate::live_baseline::canonical_baseline_path()
        .ok_or("owned-recovery: HOME is unset; no canonical receipt directory")?;
    let baseline = crate::live_baseline::load_baseline_from(&baseline_path)?;
    let residue_path = baseline_path.with_file_name(crate::live_leg::RESIDUE_FILE_NAME);
    let residue = load_residue_value(&residue_path)?;
    let receipt = receipt_from_persisted(&baseline, &residue, space)?;
    Ok((receipt, baseline))
}

fn load_residue_value(path: &Path) -> Result<serde_json::Value, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| {
        "owned-recovery: no residue artifact beside the retained receipt; no proven owned SID — refusing"
            .to_owned()
    })?;
    if meta.file_type().is_symlink() {
        return Err("owned-recovery: residue artifact is a symlink; refusing".to_owned());
    }
    if !meta.is_file() {
        return Err("owned-recovery: residue artifact is not a regular file; refusing".to_owned());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|_| "owned-recovery: residue artifact unreadable; refusing".to_owned())?;
    serde_json::from_str(&raw)
        .map_err(|_| "owned-recovery: residue artifact corrupt; refusing".to_owned())
}

/// Strip only receipt-recorded additions. Already removed SIDs may be
/// absent, but the requested SID must be present, unique and noncurrent.
fn prove_recorded_ownership(
    preimage: &NativeBaseline,
    created: &[u64],
    sid: u64,
    snapshot: &TopologySnapshot,
) -> Result<OwnedSpaceBinding, McError> {
    if sid == 0 || !created.contains(&sid) {
        return Err(McError::Ownership(
            "owned-recovery: target is not a recorded owned SID; refusing".to_owned(),
        ));
    }
    if preimage.displays.is_empty() || preimage.displays.len() != snapshot.displays.len() {
        return Err(McError::Ownership(
            "owned-recovery: display shape changed; refusing".to_owned(),
        ));
    }
    for (index, (before, after)) in preimage.displays.iter().zip(&snapshot.displays).enumerate() {
        if before.index != index || before.spaces.iter().any(|sid| created.contains(sid)) {
            return Err(McError::Ownership(format!(
                "owned-recovery: display {index} has an invalid receipt preimage; refusing"
            )));
        }
        if after.current != Some(before.current) {
            return Err(McError::Blocked(format!(
                "owned-recovery: display {index} current {:?} differs from receipt current {}; refusing",
                after.current, before.current
            )));
        }
        if !after
            .spaces
            .iter()
            .filter(|sid| !created.contains(sid))
            .eq(&before.spaces)
        {
            return Err(McError::Ownership(format!(
                "owned-recovery: display {index} is not exactly the receipt preimage after excluding recorded owned SIDs; refusing"
            )));
        }
    }
    let (display, _) = snapshot.locate(sid).ok_or_else(|| {
        McError::Ownership(format!(
            "owned-recovery: recorded Space {sid} is absent or ambiguous; refusing"
        ))
    })?;
    Ok(OwnedSpaceBinding { display, sid })
}

/// Authenticate one system owner by bundle plus exact executable.
/// WindowServer lacks a bundle; its identity is its canonical executable.
#[must_use]
pub fn authenticate_system_owner(row: &RecoveryMemberRow) -> Option<(&'static str, String)> {
    if windowserver_identity(row) {
        return Some((
            "windowserver-compositor",
            format!(
                "WindowServer canonical executable (pid {})",
                row.pid
            ),
        ));
    }
    if row.bundle == crate::mc_probe::DOCK_BUNDLE_ID
        && row.exe.as_deref() == Some(DOCK_EXE)
    {
        return Some((
            "dock",
            format!("exact Dock pid {}, bundle {}, exe {DOCK_EXE}", row.pid, row.bundle),
        ));
    }
    if row.bundle == WINDOWMANAGER_BUNDLE_ID && row.exe.as_deref() == Some(WINDOWMANAGER_EXE) {
        return Some((
            "windowmanager",
            format!(
                "exact WindowManager pid {}, bundle {WINDOWMANAGER_BUNDLE_ID}, exe {WINDOWMANAGER_EXE}",
                row.pid
            ),
        ));
    }
    if row.bundle == FINDER_BUNDLE_ID && row.exe.as_deref() == Some(FINDER_EXE) {
        return Some((
            "finder",
            format!(
                "exact Finder pid {}, bundle {FINDER_BUNDLE_ID}, exe {FINDER_EXE}",
                row.pid
            ),
        ));
    }
    if row.bundle == crate::mc_probe::CONTROLCENTER_BUNDLE
        && row.exe.as_deref() == Some(crate::mc_probe::CONTROLCENTER_EXE)
        && crate::mc_probe::controlcenter_shape_matches(
            &row.bundle,
            row.exe.as_deref(),
            row.layer,
            row.onscreen,
            row.w,
            row.h,
        )
        && row.ax_windows_absent == Some(true)
    {
        return Some((
            "controlcenter",
            format!(
                "exact ControlCenter pid {}, bundle {}, exe {}, offscreen layer {} 120x30, owner AX inventory readable empty",
                row.pid,
                row.bundle,
                crate::mc_probe::CONTROLCENTER_EXE,
                row.layer
            ),
        ));
    }
    None
}

fn windowserver_identity(row: &RecoveryMemberRow) -> bool {
    let canonical = crate::mc_probe::WINDOWSERVER_EXE;
    if row.exe.as_deref() == Some(canonical) || row.exe_alias.as_deref() == Some(canonical) {
        return true;
    }
    let Some(raw) = row.exe.as_deref().or(row.exe_alias.as_deref()) else {
        return false;
    };
    let Ok(canonical_path) = std::fs::canonicalize(canonical) else {
        return false;
    };
    crate::mc_probe::canonical_exe_equals(raw, &canonical_path)
}

/// Pure pre-removal gate over fresh title-free rows: every membership must be
/// readable, and every window on `sid` must be authenticated system chrome
/// with membership wholly inside the fresh User set. Only the Finder icon
/// plane may be onscreen. Ordinary or unknown owners always block.
pub fn authorize_owned_members(
    rows: &[RecoveryMemberRow],
    sid: u64,
    user_spaces: &[u64],
) -> Result<AuthenticatedMembers, McError> {
    if sid == 0 {
        return Err(McError::Ownership(
            "owned-recovery: zero target SID; refusing".to_owned(),
        ));
    }
    if !user_spaces.contains(&sid) {
        return Err(McError::Ownership(format!(
            "owned-recovery: Space {sid} is not in the fresh User Space set {user_spaces:?}; ownership uncertain — refusing"
        )));
    }
    let mut checked = 0usize;
    let mut members = Vec::new();
    for row in rows {
        let Some(spaces) = row.spaces.as_deref() else {
            return Err(McError::Blocked(format!(
                "owned-recovery: window {} (pid {}) SLS membership unreadable; unknown membership can hide Space {sid} — refusing",
                row.wid, row.pid
            )));
        };
        checked += 1;
        if !spaces.contains(&sid) {
            continue;
        }
        if row.pid <= 1 {
            return Err(McError::Blocked(format!(
                "owned-recovery: window {} reports reserved pid {} on Space {sid}; refusing",
                row.wid, row.pid
            )));
        }
        // The Finder icon plane is visible on the current Desktop even
        // when its membership includes an inactive owned Desktop. Ordinary
        // Finder windows do not have this layer.
        if row.onscreen
            && !(row.layer == FINDER_DESKTOP_LAYER
                && row.bundle == FINDER_BUNDLE_ID
                && row.exe.as_deref() == Some(FINDER_EXE))
        {
            return Err(McError::Blocked(format!(
                "owned-recovery: window {} (pid {}, bundle '{}') is onscreen on Space {sid}; refusing",
                row.wid, row.pid, row.bundle
            )));
        }
        if let Some(foreign) = spaces.iter().find(|space| !user_spaces.contains(space)) {
            return Err(McError::Blocked(format!(
                "owned-recovery: window {} (pid {}) membership {spaces:?} includes non-user Space {foreign}; refusing",
                row.wid, row.pid
            )));
        }
        let Some((class, detail)) = authenticate_system_owner(row) else {
            return Err(McError::Blocked(format!(
                "owned-recovery: window {} (pid {}, bundle '{}', exe {:?}, layer {}, membership {spaces:?}) is not a positively authenticated system owner on Space {sid}; refusing",
                row.wid, row.pid, row.bundle, row.exe, row.layer
            )));
        };
        members.push(AuthenticatedMember {
            wid: row.wid,
            pid: row.pid,
            bundle: row.bundle.clone(),
            layer: row.layer,
            class: class.to_owned(),
            detail,
        });
    }
    Ok(AuthenticatedMembers { checked, members })
}

/// Fresh User-Space SID set from one already-read snapshot. Unreadable type
/// fails closed (the set is unprovable), matching the default emptiness gate.
fn user_spaces_of(snapshot: &TopologySnapshot) -> Result<Vec<u64>, McError> {
    let mut user = Vec::new();
    for sid in snapshot.all_sids() {
        match crate::ffi::sls_space_type(sid) {
            Ok(crate::ffi::SlsSpaceType::User) => {
                if !user.contains(&sid) {
                    user.push(sid);
                }
            }
            Ok(_) => {}
            Err(cause) => {
                return Err(McError::Unsupported(format!(
                    "owned-recovery: Space {sid} type unreadable ({cause}); the User set is unprovable — refusing"
                )));
            }
        }
    }
    Ok(user)
}

/// Independent live re-read of the receipt-bound authorization against one
/// caller-supplied fresh topology snapshot: the recorded-set binding, the
/// Space types, and the CG/SLS membership are all proven here. Nothing from
/// the captured evidence is reused as authorization except the recorded SID
/// and preimage. The removal seam calls this with the dispatch's own
/// eligibility snapshot so the last membership read precedes the press.
pub fn verify_owned_space_recovery(
    receipt: &OwnedSpaceReceipt,
    snapshot: &TopologySnapshot,
) -> Result<OwnedSpaceRecoveryProof, McError> {
    let binding =
        prove_recorded_ownership(&receipt.preimage, &receipt.created, receipt.sid, snapshot)?;
    membership_proof(receipt.sid, binding.display, snapshot)
}

/// Independent live re-read of the typed membership authorization alone,
/// against one fresh topology snapshot. This is the seam for a Space whose
/// ownership the caller already holds in memory (the active `LiveLeg`
/// created-Space registration): the SID must still be uniquely located, and
/// every member must pass the typed system-chrome gate with no unknown
/// membership. The default emptiness gate is neither consulted nor widened.
pub fn verify_owned_space_members(
    sid: u64,
    snapshot: &TopologySnapshot,
) -> Result<OwnedSpaceRecoveryProof, McError> {
    let (display, _) = snapshot.locate(sid).ok_or_else(|| {
        McError::Ownership(format!(
            "owned-recovery: Space {sid} is not uniquely located; ownership uncertain — refusing"
        ))
    })?;
    membership_proof(sid, display, snapshot)
}

fn membership_proof(
    sid: u64,
    display: usize,
    snapshot: &TopologySnapshot,
) -> Result<OwnedSpaceRecoveryProof, McError> {
    if sid == 0 {
        return Err(McError::Ownership(
            "owned-recovery: zero target SID; refusing".to_owned(),
        ));
    }
    let user_spaces = user_spaces_of(snapshot)?;
    let rows = live_member_rows()?;
    let authenticated = authorize_owned_members(&rows, sid, &user_spaces)?;
    let current = snapshot.displays[display].current.ok_or_else(|| {
        McError::Unsupported(format!(
            "owned-recovery: display {display} current unreadable; refusing"
        ))
    })?;
    Ok(OwnedSpaceRecoveryProof {
        display,
        sid,
        current,
        checked_windows: authenticated.checked,
        members: authenticated.members,
    })
}

fn live_member_rows() -> Result<Vec<RecoveryMemberRow>, McError> {
    let live = crate::mc_observe::enumerate_window_members()?;
    let mut rows = Vec::with_capacity(live.len());
    for member in live {
        let spaces = match member.spaces {
            Ok(spaces) => Some(spaces),
            Err(cause) => {
                return Err(McError::Blocked(format!(
                    "owned-recovery: window {} (pid {}) SLS membership unreadable ({cause}); unknown membership can hide the target — refusing",
                    member.wid, member.pid
                )));
            }
        };
        rows.push(RecoveryMemberRow {
            wid: member.wid,
            pid: member.pid,
            bundle: member.bundle,
            layer: member.layer,
            onscreen: member.onscreen,
            x: member.x,
            y: member.y,
            w: member.w,
            h: member.h,
            exe: member.exe,
            exe_alias: member.exe_alias,
            ax_windows_absent: member.ax_windows_absent,
            spaces,
        });
    }
    Ok(rows)
}

/// Remove exactly the receipt-bound owned Space. The authorization is
/// re-read live here, then the shared SID dispatch performs its own fresh
/// eligibility read, SID press, and exact-effect readback. `remove_dispatched`
/// records a sent press even when the effect stays unverified so cleanup
/// never replays it. Default emptiness is never consulted or relaxed.
pub fn remove_owned_sid_verified(
    receipt: &OwnedSpaceReceipt,
    budget: &mut crate::mc_probe::ActionBudget,
    cleanup: bool,
    remove_dispatched: &mut bool,
) -> Result<(OwnedSpaceRecoveryProof, Option<String>), McError> {
    let mut proof: Option<OwnedSpaceRecoveryProof> = None;
    let close = crate::mc_lifecycle::remove_sid_dispatched_checked(
        receipt.sid,
        budget,
        cleanup,
        remove_dispatched,
        |pre| {
            proof = Some(verify_owned_space_recovery(receipt, pre)?);
            Ok(())
        },
    )?;
    let proof = proof.ok_or_else(|| {
        McError::System(
            "owned-recovery: dispatch finished without its pre-press proof; refusing the result"
                .to_owned(),
        )
    })?;
    Ok((proof, close))
}

/// Remove one Space whose ownership the caller already holds in memory (the
/// active `LiveLeg` registration), using only the shared typed membership
/// authorization. Re-read live inside the dispatch immediately before the
/// press; default emptiness is never consulted. Kept separate from the
/// receipt route because no persisted preimage is needed once the leg's own
/// creation proof is the claim.
pub fn remove_owned_sid_typed(
    sid: u64,
    budget: &mut crate::mc_probe::ActionBudget,
    cleanup: bool,
    remove_dispatched: &mut bool,
) -> Result<(OwnedSpaceRecoveryProof, Option<String>), McError> {
    let mut proof: Option<OwnedSpaceRecoveryProof> = None;
    let close = crate::mc_lifecycle::remove_sid_dispatched_checked(
        sid,
        budget,
        cleanup,
        remove_dispatched,
        |pre| {
            proof = Some(verify_owned_space_members(sid, pre)?);
            Ok(())
        },
    )?;
    let proof = proof.ok_or_else(|| {
        McError::System(
            "owned-recovery: dispatch finished without its pre-press proof; refusing the result"
                .to_owned(),
        )
    })?;
    Ok((proof, close))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_baseline::NativeDisplayBaseline;

    /// The five actual `detached-capture-post-space305.json` members
    /// (Space 305, 2026-11-29): one Finder desktop row on [305, 1], one
    /// WindowManager desktop row, two WindowServer rows, and the offscreen
    /// layer-0 WindowManager row 10499 that blocked the default gate. All
    /// offscreen. Reproduced verbatim so the gate is tested against the
    /// captured shape rather than a hand-built imitation.
    const CAPTURED_305_MEMBERS: &str = r#"[
      {"wid":23,"pid":908,"bundle":"com.apple.finder","layer":-2147483603,"onscreen":false,"x":0.0,"y":0.0,"w":2240.0,"h":1260.0,"spaces":[305,1],"exe":"/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder","exe_alias":null,"ax_windows_absent":null},
      {"wid":10448,"pid":865,"bundle":"com.apple.WindowManager","layer":-2147483624,"onscreen":false,"x":0.0,"y":0.0,"w":2240.0,"h":1260.0,"spaces":[305],"exe":"/System/Library/CoreServices/WindowManager.app/Contents/MacOS/WindowManager","exe_alias":null,"ax_windows_absent":null},
      {"wid":10449,"pid":641,"bundle":"","layer":24,"onscreen":false,"x":0.0,"y":0.0,"w":2240.0,"h":1260.0,"spaces":[305],"exe":"/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer","exe_alias":"/System/Library/PrivateFrameworks/SkyLight.framework/Versions/A/Resources/WindowServer","ax_windows_absent":null},
      {"wid":10450,"pid":641,"bundle":"","layer":-2147483602,"onscreen":false,"x":0.0,"y":0.0,"w":2240.0,"h":1260.0,"spaces":[305],"exe":"/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer","exe_alias":"/System/Library/PrivateFrameworks/SkyLight.framework/Versions/A/Resources/WindowServer","ax_windows_absent":null},
      {"wid":10499,"pid":865,"bundle":"com.apple.WindowManager","layer":0,"onscreen":false,"x":0.0,"y":0.0,"w":2240.0,"h":1260.0,"spaces":[305],"exe":"/System/Library/CoreServices/WindowManager.app/Contents/MacOS/WindowManager","exe_alias":null,"ax_windows_absent":null}
    ]"#;

    fn captured_rows() -> Vec<RecoveryMemberRow> {
        serde_json::from_str(CAPTURED_305_MEMBERS).expect("captured rows deserialize")
    }

    fn captured_preimage() -> NativeBaseline {
        NativeBaseline {
            separate_spaces: true,
            displays: vec![
                NativeDisplayBaseline {
                    index: 0,
                    spaces: vec![1],
                    current: 1,
                },
                NativeDisplayBaseline {
                    index: 1,
                    spaces: vec![156],
                    current: 156,
                },
            ],
        }
    }

    fn captured_snapshot() -> TopologySnapshot {
        TopologySnapshot {
            displays: vec![
                crate::mc_probe::DisplaySpaces {
                    identifier: None,
                    spaces: vec![1, 305],
                    current: Some(1),
                },
                crate::mc_probe::DisplaySpaces {
                    identifier: None,
                    spaces: vec![156],
                    current: Some(156),
                },
            ],
        }
    }

    #[test]
    fn captured_305_rows_and_single_append_authorize() {
        let binding = prove_recorded_ownership(&captured_preimage(), &[305], 305, &captured_snapshot())
            .expect("exact captured append proves");
        assert_eq!(binding.display, 0);
        assert_eq!(binding.sid, 305);
        let authenticated = authorize_owned_members(&captured_rows(), 305, &[1, 156, 305])
            .expect("captured system-only membership authorizes");
        assert_eq!(authenticated.checked, 5);
        assert_eq!(authenticated.members.len(), 5);
        assert!(
            authenticated
                .members
                .iter()
                .all(|member| member.class != "user" && member.wid != 0),
            "every target member carries an authenticated class"
        );
        // The previously blocking layer-0 WindowManager row is admitted only
        // as its exact offscreen system identity.
        let blocking = authenticated
            .members
            .iter()
            .find(|member| member.wid == 10499)
            .expect("blocking row present");
        assert_eq!(blocking.class, "windowmanager");
        assert_eq!(blocking.layer, 0);
    }

    #[test]
    fn onscreen_member_refuses() {
        let mut rows = captured_rows();
        rows[4].onscreen = true;
        let err = authorize_owned_members(&rows, 305, &[1, 156, 305])
            .expect_err("onscreen chrome blocks");
        assert!(err.to_string().contains("onscreen"), "{err}");
    }

    #[test]
    fn foreign_member_refuses() {
        let mut rows = captured_rows();
        rows.push(RecoveryMemberRow {
            wid: 10500,
            pid: 9999,
            bundle: "com.example.foreign".to_owned(),
            layer: 0,
            onscreen: false,
            x: 0.0,
            y: 0.0,
            w: 640.0,
            h: 480.0,
            exe: Some("/Applications/Foreign.app/Contents/MacOS/Foreign".to_owned()),
            exe_alias: None,
            ax_windows_absent: None,
            spaces: Some(vec![305]),
        });
        let err = authorize_owned_members(&rows, 305, &[1, 156, 305])
            .expect_err("ordinary user window blocks");
        assert!(
            err.to_string().contains("not a positively authenticated"),
            "{err}"
        );
    }

    #[test]
    fn fixture_member_refuses() {
        let mut rows = captured_rows();
        rows.push(RecoveryMemberRow {
            wid: 10501,
            pid: 4577,
            bundle: crate::fixture_leg::FIXTURE_SCOPE.to_owned(),
            layer: 0,
            onscreen: false,
            x: 720.0,
            y: 157.0,
            w: 800.0,
            h: 632.0,
            exe: Some("/tmp/fixture/TilerFixture.app/Contents/MacOS/TilerFixture".to_owned()),
            exe_alias: None,
            ax_windows_absent: None,
            spaces: Some(vec![305]),
        });
        let err = authorize_owned_members(&rows, 305, &[1, 156, 305])
            .expect_err("fixture row blocks");
        assert!(
            err.to_string().contains("not a positively authenticated"),
            "{err}"
        );
    }

    #[test]
    fn unknown_membership_refuses() {
        let mut rows = captured_rows();
        rows[1].spaces = None;
        let err = authorize_owned_members(&rows, 305, &[1, 156, 305])
            .expect_err("unreadable membership blocks");
        assert!(err.to_string().contains("unreadable"), "{err}");
    }

    #[test]
    fn spoofed_windowserver_executable_refuses() {
        let mut rows = captured_rows();
        rows[2].exe = Some("/tmp/WindowServer".to_owned());
        rows[2].exe_alias = None;
        let err = authorize_owned_members(&rows, 305, &[1, 156, 305])
            .expect_err("non-canonical WindowServer path blocks");
        assert!(
            err.to_string().contains("not a positively authenticated"),
            "{err}"
        );
    }

    #[test]
    fn current_target_refuses() {
        let mut snapshot = captured_snapshot();
        snapshot.displays[0].current = Some(305);
        let err = prove_recorded_ownership(&captured_preimage(), &[305], 305, &snapshot)
            .expect_err("active Space blocks");
        assert!(err.to_string().contains("current"), "{err}");
    }

    #[test]
    fn extra_append_refuses() {
        let mut snapshot = captured_snapshot();
        snapshot.displays[0].spaces.push(306);
        let err = prove_recorded_ownership(&captured_preimage(), &[305], 305, &snapshot)
            .expect_err("two appends block");
        assert!(err.to_string().contains("exactly"), "{err}");
    }

    #[test]
    fn unrelated_display_change_refuses() {
        let mut snapshot = captured_snapshot();
        snapshot.displays[1].spaces.push(999);
        let err = prove_recorded_ownership(&captured_preimage(), &[305], 305, &snapshot)
            .expect_err("other display drift blocks");
        assert!(err.to_string().contains("display 1"), "{err}");
    }

    #[test]
    fn two_recorded_desktops_allow_one_removal_then_the_other() {
        let preimage = captured_preimage();
        let residue = serde_json::json!({"residue": [
            "owned Space 308: Finder desktop row onscreen",
            "owned Space 310: Finder desktop row onscreen"
        ]});
        let created = owned_sids_in_residue(&residue);
        let mut snap = captured_snapshot();
        snap.displays[0].spaces = vec![1, 308, 310];
        for sid in [308, 310] {
            assert_eq!(
                prove_recorded_ownership(&preimage, &created, sid, &snap)
                    .expect("recorded inactive SID authorizes")
                    .sid,
                sid
            );
            snap.displays[0].spaces.retain(|candidate| *candidate != sid);
        }
        assert!(prove_recorded_ownership(&preimage, &created, 310, &snap).is_err());
    }

    #[test]
    fn captured_onscreen_finder_desktop_is_not_a_file_window() {
        let mut finder = captured_rows().remove(0);
        finder.onscreen = true;
        finder.spaces = Some(vec![1, 308, 310]);
        for sid in [308, 310] {
            let proof =
                authorize_owned_members(std::slice::from_ref(&finder), sid, &[1, 156, 308, 310])
                    .expect("captured Finder icon plane is not an occupant");
            assert_eq!(proof.members[0].class, "finder");
        }
        finder.layer = 0;
        assert!(authorize_owned_members(&[finder], 308, &[1, 156, 308, 310]).is_err());
    }

    #[test]
    fn reordered_preexisting_spaces_refuse_even_if_target_is_last() {
        let mut preimage = captured_preimage();
        preimage.displays[0].spaces.push(2);
        let mut snapshot = captured_snapshot();
        snapshot.displays[0].spaces = vec![2, 1, 305];
        assert!(prove_recorded_ownership(&preimage, &[305], 305, &snapshot).is_err());
    }

    #[test]
    fn unrecorded_sid_refuses() {
        let mut preimage = captured_preimage();
        preimage.displays[0].spaces.push(305);
        let err = prove_recorded_ownership(&preimage, &[305], 305, &captured_snapshot())
            .expect_err("preimage membership is not an owned append");
        assert!(err.to_string().contains("preimage"), "{err}");
    }

    #[test]
    fn residue_names_only_owned_space_entries() {
        let residue = serde_json::json!({
            "receipt_retained": true,
            "residue": [
                "owned Space 305: owned-space removal: Space 305 not provably empty; never forced",
                "awake companion stop refused: timed out",
                "window 42 is not an owned Space"
            ]
        });
        assert_eq!(owned_sids_in_residue(&residue), vec![305]);
    }

    #[test]
    fn receipt_requires_retained_named_sid_and_clean_preimage() {
        let baseline = LiveBaseline {
            version: crate::live_baseline::BASELINE_SCHEMA_VERSION,
            protected_windows: Vec::new(),
            holders: Vec::new(),
            accent: crate::live_baseline::accent_from(None),
            processes: Vec::new(),
            native: captured_preimage(),
        };
        let residue = serde_json::json!({
            "receipt_retained": true,
            "residue": ["owned Space 305: refused"]
        });
        let receipt =
            receipt_from_persisted(&baseline, &residue, 305).expect("recorded owned SID binds");
        assert_eq!(receipt.sid, 305);
        assert_eq!(receipt.preimage.displays[0].spaces, vec![1]);

        let err = receipt_from_persisted(&baseline, &residue, 306)
            .expect_err("unrecorded SID refuses");
        assert!(err.contains("names no owned Space 306"), "{err}");

        let not_retained = serde_json::json!({
            "receipt_retained": false,
            "residue": ["owned Space 305: refused"]
        });
        assert!(receipt_from_persisted(&baseline, &not_retained, 305).is_err());

        let mut preimaged = baseline.clone();
        preimaged.native.displays[0].spaces.push(305);
        let err = receipt_from_persisted(&preimaged, &residue, 305)
            .expect_err("a preimage SID is not a created append");
        assert!(err.contains("preimage"), "{err}");
    }
}
