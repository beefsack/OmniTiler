#![forbid(unsafe_code)]

//! Read-only protected-window baseline compare for standalone live legs.
//!
//! Standalone legs (fixture bundle plus probe observations, no `fixture-leg`
//! product run) capture raw before/after records with `cg-inspect` (the
//! title-free unfiltered `CGWindowListCopyWindowInfo` snapshot: PID, window
//! ID, layer, onscreen, bounds) and compare them here. The compare is pure
//! file I/O plus data joins: it never touches the window server, AX, input,
//! Spaces, or settings, so it runs offline on retained records.
//!
//! Every roster or geometry difference is a mismatch entry: windows missing
//! after the leg, windows added during the leg, and persistent per-window
//! drift (layer, onscreen, or bounds). Roster changes are never absorbed and
//! never classified clean: any missing, added, or changed entry makes
//! `clean:false`. The only non-clean exit-0 verdict is explicitly verified
//! benign host churn (see [`classify_benign_churn`]): every other mismatch
//! stays `verdict:"drift"` and exits nonzero. A duplicate `(pid, wid)` key
//! inside one file refuses (exit 2): the snapshot is ambiguous, never
//! silently joined.
//!
//! The `{"windows": [...]}` envelope check below is shape validation only,
//! never a completeness proof: a hand-written partial roster in the right
//! shape still loads and can compare false-clean. Only an actual unfiltered
//! `cg-inspect` capture of the live session assures a complete roster; never
//! treat a clean report over a hand-assembled file as capture evidence.
//!
//! Mission Control rule (procedural, enforced by the live sequence, not this
//! tool): CG bounds sampled while Mission Control is shown reflect its
//! scaled thumbnails, not real moves. Never compare against a thumbnail
//! sample: close Mission Control first, re-sample, and compare the
//! post-close file.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ffi::{CgBounds, CgWindowBasic};

/// Verdict when every difference is an explicitly verified benign host-churn
/// pair and nothing else differs. Exact approved wording; greppable in
/// reports and records.
pub const BENIGN_CHURN_CLASSIFICATION: &str =
    "benign host churn: offscreen replacement, identical geometry";

/// One classified benign-churn pair: a missing window and an added window
/// from the same explicitly verified pid, both offscreen, with identical
/// layer and bounds. The raw entries stay in `missing`/`added`; this is an
/// annotation, never a deletion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenignChurnPair {
    pub pid: u32,
    pub missing_wid: u32,
    pub added_wid: u32,
    pub layer: i32,
    pub onscreen: bool,
    pub bounds: CgBounds,
}

/// One window present in both files whose layer, onscreen flag, or bounds
/// differ. `fields` names the drifted subset (`layer`, `onscreen`,
/// `bounds`); the full before/after records ride along for the residue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangedWindow {
    pub pid: u32,
    pub wid: u32,
    pub fields: Vec<String>,
    pub before: CgWindowBasic,
    pub after: CgWindowBasic,
}

/// Pure compare report over two title-free snapshots. Sorted by `(pid, wid)`
/// for deterministic diffing. No titles, paths, nonces, or content anywhere.
/// `clean` stays exact-match only: roster churn never sets it, even when
/// classified benign. `verdict` is `"clean"`, the benign-churn classification
/// sentence, or `"drift"`; only a `"drift"` verdict exits nonzero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffReport {
    pub before_count: usize,
    pub after_count: usize,
    pub missing: Vec<CgWindowBasic>,
    pub added: Vec<CgWindowBasic>,
    pub changed: Vec<ChangedWindow>,
    pub benign_churn: Vec<BenignChurnPair>,
    pub verdict: String,
    pub clean: bool,
    pub mutates: bool,
}

fn key_of(window: &CgWindowBasic) -> (u32, u32) {
    (window.pid, window.wid)
}

/// Pure join over plain data: missing, added, and drifted entries. A
/// duplicate key inside one file is an ambiguous snapshot and refuses.
pub fn diff_windows(
    before: &[CgWindowBasic],
    after: &[CgWindowBasic],
) -> Result<DiffReport, String> {
    let mut before_map: BTreeMap<(u32, u32), &CgWindowBasic> = BTreeMap::new();
    for window in before {
        if before_map.insert(key_of(window), window).is_some() {
            return Err(format!(
                "before snapshot carries pid {} wid {} twice; refusing an ambiguous join",
                window.pid, window.wid
            ));
        }
    }
    let mut after_map: BTreeMap<(u32, u32), &CgWindowBasic> = BTreeMap::new();
    for window in after {
        if after_map.insert(key_of(window), window).is_some() {
            return Err(format!(
                "after snapshot carries pid {} wid {} twice; refusing an ambiguous join",
                window.pid, window.wid
            ));
        }
    }
    let mut missing = Vec::new();
    let mut changed = Vec::new();
    for (key, before_window) in &before_map {
        match after_map.get(key) {
            None => missing.push(**before_window),
            Some(after_window) => {
                if *after_window != *before_window {
                    let mut fields = Vec::new();
                    if after_window.layer != before_window.layer {
                        fields.push("layer".to_owned());
                    }
                    if after_window.onscreen != before_window.onscreen {
                        fields.push("onscreen".to_owned());
                    }
                    if after_window.bounds != before_window.bounds {
                        fields.push("bounds".to_owned());
                    }
                    changed.push(ChangedWindow {
                        pid: key.0,
                        wid: key.1,
                        fields,
                        before: **before_window,
                        after: **after_window,
                    });
                }
            }
        }
    }
    let mut added = Vec::new();
    for (key, after_window) in &after_map {
        if !before_map.contains_key(key) {
            added.push(**after_window);
        }
    }
    // BTreeMap iteration is already key-ordered; the cloned vectors inherit it.
    let clean = missing.is_empty() && added.is_empty() && changed.is_empty();
    let verdict = if clean {
        "clean".to_owned()
    } else {
        "drift".to_owned()
    };
    Ok(DiffReport {
        before_count: before.len(),
        after_count: after.len(),
        missing,
        added,
        changed,
        benign_churn: Vec::new(),
        verdict,
        clean,
        mutates: false,
    })
}

/// Narrowly classify offscreen same-pid replacements at identical geometry
/// as benign host churn. `benign_pids` is explicit operator-verified input
/// (owner name/pid checked against the baseline plus read-only process
/// identity): only pids listed there are eligible, and each eligible pid
/// must show equal nonzero counts of missing and added entries that pair up
/// (sorted by wid) with both sides offscreen and identical layer and bounds.
/// Any onscreen entry, any layer/bounds mismatch, any count mismatch, or any
/// pid outside the allowlist (including live app windows) classifies nothing
/// for that pid. Raw `missing`/`added` are preserved; pairs only annotate.
/// A useful read-only check for the operator (never done inside this pure
/// offline compare, where the pid may be long gone or reused): confirm the
/// pid still maps to the same system/unrelated background executable.
pub fn classify_benign_churn(
    missing: &[CgWindowBasic],
    added: &[CgWindowBasic],
    benign_pids: &[u32],
) -> Vec<BenignChurnPair> {
    let mut pids: Vec<u32> = benign_pids.to_vec();
    pids.sort_unstable();
    pids.dedup();
    let mut pairs = Vec::new();
    for pid in pids {
        let mut gone: Vec<&CgWindowBasic> = missing.iter().filter(|w| w.pid == pid).collect();
        let mut fresh: Vec<&CgWindowBasic> = added.iter().filter(|w| w.pid == pid).collect();
        if gone.is_empty() || gone.len() != fresh.len() {
            continue;
        }
        gone.sort_by_key(|w| w.wid);
        fresh.sort_by_key(|w| w.wid);
        let eligible = gone.iter().zip(fresh.iter()).all(|(before, after)| {
            !before.onscreen
                && !after.onscreen
                && before.layer == after.layer
                && before.bounds == after.bounds
        });
        if !eligible {
            continue;
        }
        for (before, after) in gone.iter().zip(fresh.iter()) {
            pairs.push(BenignChurnPair {
                pid,
                missing_wid: before.wid,
                added_wid: after.wid,
                layer: before.layer,
                onscreen: false,
                bounds: before.bounds,
            });
        }
    }
    pairs.sort_by_key(|p| (p.pid, p.missing_wid, p.added_wid));
    pairs
}

/// Attach an explicit benign-churn classification to an exact report and
/// resolve its verdict. `clean` is untouched (exact-match only). The verdict
/// becomes the benign-churn classification sentence only when nothing else
/// differs: no `changed` entries and every `missing`/`added` entry consumed
/// by a classified pair. Anything else stays `"drift"`.
pub fn apply_benign_churn(mut report: DiffReport, benign_pids: &[u32]) -> DiffReport {
    report.benign_churn = classify_benign_churn(&report.missing, &report.added, benign_pids);
    let churn_only = !report.clean
        && report.changed.is_empty()
        && !report.benign_churn.is_empty()
        && report.missing.len() + report.added.len() == 2 * report.benign_churn.len();
    report.verdict = if report.clean {
        "clean".to_owned()
    } else if churn_only {
        BENIGN_CHURN_CLASSIFICATION.to_owned()
    } else {
        "drift".to_owned()
    };
    report
}

/// Load one `cg-inspect` dump: its `{"windows": [...]}` envelope of
/// title-free entries. Anything else refuses (exit 2): a guessed, partial,
/// or bare-array roster without the envelope is never compared as a complete
/// native capture. The envelope itself is shape validation only (see the
/// module docs): unknown input keys are ignored by design, the loader
/// projects the selected baseline fields (`wid`, `pid`, `layer`, `onscreen`,
/// `bounds`) and never rejects extra keys with `deny_unknown_fields`.
fn load_snapshot(path: &str, role: &str) -> Result<Vec<CgWindowBasic>, (i32, String)> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| (2, format!("cg-diff: {role} file refused ({e})")))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| (2, format!("cg-diff: {role} file is not JSON")))?;
    let list = match &value {
        serde_json::Value::Object(_) => value.get("windows").cloned().ok_or_else(|| {
            (
                2,
                format!("cg-diff: {role} file has no windows list (not a cg-inspect dump)"),
            )
        })?,
        _ => {
            return Err((2, format!("cg-diff: {role} file is not a cg-inspect dump")));
        }
    };
    serde_json::from_value(list)
        .map_err(|e| (2, format!("cg-diff: {role} windows list refused ({e})")))
}

/// Compare two retained `cg-inspect` dumps. Clean compares exit 0 with the
/// report on stdout; a fully classified benign-churn compare also exits 0
/// (verdict carries the classification sentence, raw roster retained).
/// Any other missing, added, or drifted window exits 1 with the same report
/// shape (never clean over roster changes) on stderr; unreadable
/// or ambiguous inputs refuse with stderr diagnostics and exit 2, as does an
/// internal report-serialization failure (never exit 1: it proves no
/// comparison at all). `benign_pids` is explicit operator-verified owner
/// input (see [`classify_benign_churn`]); empty disables classification.
pub fn run_cg_diff(
    before_path: &str,
    after_path: &str,
    benign_pids: &[u32],
) -> Result<String, (i32, String)> {
    let before = load_snapshot(before_path, "before")?;
    let after = load_snapshot(after_path, "after")?;
    let exact =
        diff_windows(&before, &after).map_err(|detail| (2, format!("cg-diff: {detail}")))?;
    let report = apply_benign_churn(exact, benign_pids);
    let json = serde_json::to_string(&report).map_err(|e| (2, e.to_string()))?;
    if report.verdict == "drift" {
        Err((1, json))
    } else {
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(pid: u32, wid: u32, x: f64, w: f64) -> CgWindowBasic {
        CgWindowBasic {
            wid,
            pid,
            layer: 0,
            onscreen: true,
            bounds: crate::ffi::CgBounds {
                x,
                y: 20.0,
                w,
                h: 600.0,
            },
        }
    }

    #[test]
    fn identical_rosters_compare_clean_in_any_order() {
        let before = vec![window(9, 3, 10.0, 800.0), window(7, 1, 0.0, 640.0)];
        let after = vec![window(7, 1, 0.0, 640.0), window(9, 3, 10.0, 800.0)];
        let report = diff_windows(&before, &after).expect("joins");
        assert!(report.clean);
        assert!(report.missing.is_empty());
        assert!(report.added.is_empty());
        assert!(report.changed.is_empty());
        assert_eq!(report.before_count, 2);
        assert_eq!(report.after_count, 2);
    }

    #[test]
    fn missing_added_and_drifted_each_mismatch() {
        let before = vec![
            window(7, 1, 0.0, 640.0),
            window(7, 2, 100.0, 640.0),
            window(9, 3, 10.0, 800.0),
        ];
        let mut drifted = window(9, 3, 10.0, 800.0);
        drifted.bounds.w = 801.0;
        drifted.onscreen = false;
        let after = vec![window(7, 1, 0.0, 640.0), drifted, window(11, 5, 0.0, 100.0)];
        let report = diff_windows(&before, &after).expect("joins");
        assert!(!report.clean);
        assert_eq!(report.missing, vec![window(7, 2, 100.0, 640.0)]);
        assert_eq!(report.added, vec![window(11, 5, 0.0, 100.0)]);
        assert_eq!(report.changed.len(), 1);
        assert_eq!(report.changed[0].pid, 9);
        assert_eq!(report.changed[0].wid, 3);
        assert_eq!(report.changed[0].fields, vec!["onscreen", "bounds"]);
        assert_eq!(report.changed[0].before.bounds.w, 800.0);
        assert_eq!(report.changed[0].after.bounds.w, 801.0);
    }

    #[test]
    fn roster_changes_alone_never_classify_clean() {
        let before = vec![window(7, 1, 0.0, 640.0)];
        let mut added = before.clone();
        added.push(window(7, 2, 0.0, 100.0));
        let report = diff_windows(&before, &added).expect("joins");
        assert!(!report.clean);
        assert_eq!(report.added.len(), 1);
        assert!(report.missing.is_empty() && report.changed.is_empty());
        let removed = vec![window(7, 1, 0.0, 640.0)];
        let report = diff_windows(&added, &removed).expect("joins");
        assert!(!report.clean);
        assert_eq!(report.missing.len(), 1);
    }

    #[test]
    fn layer_only_drift_names_layer() {
        let before = vec![window(7, 1, 0.0, 640.0)];
        let mut moved = window(7, 1, 0.0, 640.0);
        moved.layer = 5;
        let report = diff_windows(&before, &[moved]).expect("joins");
        assert!(!report.clean);
        assert_eq!(report.changed[0].fields, vec!["layer"]);
    }

    #[test]
    fn duplicate_keys_refuse_before_any_join() {
        let before = vec![window(7, 1, 0.0, 640.0), window(7, 1, 5.0, 640.0)];
        let after = vec![window(7, 1, 0.0, 640.0)];
        assert!(diff_windows(&before, &after).is_err());
        assert!(diff_windows(&after, &before).is_err());
    }

    #[test]
    fn report_carries_no_title_content_or_identity_keys() {
        let before = vec![window(7, 1, 0.0, 640.0)];
        let report = diff_windows(&before, &before).expect("joins");
        let flat = serde_json::to_string(&report).expect("report flattens");
        for leaked in [
            "title",
            "nonce",
            "bundle",
            "executable",
            "socket",
            "content",
        ] {
            assert!(!flat.contains(leaked), "report leaks {leaked}");
        }
    }

    #[test]
    fn loader_accepts_envelope_but_refuses_bare_and_other_shapes() {
        // Unique create-new scratch dir (never a predictable PID-only path):
        // no preemptive remove_dir_all, so a pre-existing dir is never
        // discarded; cleanup removes only the files this test created.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut dir = std::path::PathBuf::new();
        let mut created = false;
        for attempt in 0..100 {
            let candidate = std::env::temp_dir().join(format!(
                "cg-diff-{}-{}-{}",
                std::process::id(),
                stamp,
                attempt
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => {
                    dir = candidate;
                    created = true;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("test dir creates: {e}"),
            }
        }
        assert!(created, "test dir creates");
        let envelope = dir.join("envelope.json");
        let bare = dir.join("bare.json");
        let windows = vec![window(7, 1, 0.0, 640.0)];
        std::fs::write(
            &envelope,
            serde_json::json!({"windows": windows, "mutates": false}).to_string(),
        )
        .expect("envelope writes");
        std::fs::write(&bare, serde_json::json!(windows).to_string()).expect("bare writes");
        assert_eq!(
            load_snapshot(envelope.to_str().unwrap(), "before").expect("envelope loads"),
            windows
        );
        // A bare array is not a cg-inspect dump: without the envelope it
        // proves no complete native capture, so it refuses.
        assert!(load_snapshot(bare.to_str().unwrap(), "before").is_err());
        let bad_shape = dir.join("bad.json");
        std::fs::write(&bad_shape, r#"{"session":{}}"#).expect("bad writes");
        assert!(load_snapshot(bad_shape.to_str().unwrap(), "before").is_err());
        let not_json = dir.join("not.json");
        std::fs::write(&not_json, "not json").expect("text writes");
        assert!(load_snapshot(not_json.to_str().unwrap(), "before").is_err());
        assert!(load_snapshot(dir.join("absent.json").to_str().unwrap(), "before").is_err());
        for stale in [&envelope, &bare, &bad_shape, &not_json] {
            let _ = std::fs::remove_file(stale);
        }
        let _ = std::fs::remove_dir(&dir);
    }

    fn offscreen(pid: u32, wid: u32) -> CgWindowBasic {
        CgWindowBasic {
            wid,
            pid,
            layer: 0,
            onscreen: false,
            bounds: crate::ffi::CgBounds {
                x: 0.0,
                y: 1196.0,
                w: 64.0,
                h: 64.0,
            },
        }
    }

    #[test]
    fn verified_offscreen_identical_replacement_classifies_but_keeps_raw_roster() {
        let before = vec![window(7, 1, 0.0, 640.0), offscreen(83633, 801)];
        let after = vec![window(7, 1, 0.0, 640.0), offscreen(83633, 829)];
        let exact = diff_windows(&before, &after).expect("joins");
        assert!(!exact.clean);
        assert_eq!(exact.verdict, "drift");
        assert!(exact.benign_churn.is_empty());
        let noted = apply_benign_churn(exact, &[83633]);
        assert!(!noted.clean);
        assert_eq!(noted.verdict, BENIGN_CHURN_CLASSIFICATION);
        // Raw roster distinction preserved: nothing absorbed or deleted.
        assert_eq!(noted.missing, vec![offscreen(83633, 801)]);
        assert_eq!(noted.added, vec![offscreen(83633, 829)]);
        assert!(noted.changed.is_empty());
        assert_eq!(noted.benign_churn.len(), 1);
        let pair = &noted.benign_churn[0];
        assert_eq!(pair.pid, 83633);
        assert_eq!((pair.missing_wid, pair.added_wid), (801, 829));
        assert!(!pair.onscreen);
        // Classification carries no owner identity beyond the pids already
        // in the raw roster: the fixed sentence names no process.
        let flat = serde_json::to_string(&noted).expect("report flattens");
        assert!(flat.contains(BENIGN_CHURN_CLASSIFICATION));
        for leaked in [
            "title",
            "nonce",
            "bundle",
            "executable",
            "socket",
            "content",
        ] {
            assert!(!flat.contains(leaked), "report leaks {leaked}");
        }
    }

    #[test]
    fn churn_verdict_requires_explicit_allowlist_entry() {
        let before = vec![offscreen(83633, 801)];
        let after = vec![offscreen(83633, 829)];
        let exact = diff_windows(&before, &after).expect("joins");
        // Same geometry, but no verified pid supplied: stays drift.
        let unlisted = apply_benign_churn(exact.clone(), &[]);
        assert_eq!(unlisted.verdict, "drift");
        assert!(unlisted.benign_churn.is_empty());
        // A different pid allowlisted: an app-window replacement never waives.
        let other = apply_benign_churn(exact.clone(), &[99999]);
        assert_eq!(other.verdict, "drift");
        assert!(other.benign_churn.is_empty());
        // Raw roster identical in every case.
        assert_eq!(unlisted.missing, exact.missing);
        assert_eq!(other.added, exact.added);
    }

    #[test]
    fn onscreen_geometry_layer_or_count_changes_never_classify() {
        // Onscreen replacement at identical geometry: drift.
        let mut shown_before = offscreen(83633, 801);
        shown_before.onscreen = true;
        let mut shown_after = offscreen(83633, 829);
        shown_after.onscreen = true;
        let report = apply_benign_churn(
            diff_windows(&[shown_before], &[shown_after]).expect("joins"),
            &[83633],
        );
        assert_eq!(report.verdict, "drift");
        assert!(report.benign_churn.is_empty());
        // Bounds change: drift.
        let mut moved = offscreen(83633, 829);
        moved.bounds.w = 65.0;
        let report = apply_benign_churn(
            diff_windows(&[offscreen(83633, 801)], &[moved]).expect("joins"),
            &[83633],
        );
        assert_eq!(report.verdict, "drift");
        assert!(report.benign_churn.is_empty());
        // Layer change: drift.
        let mut relayered = offscreen(83633, 829);
        relayered.layer = 1;
        let report = apply_benign_churn(
            diff_windows(&[offscreen(83633, 801)], &[relayered]).expect("joins"),
            &[83633],
        );
        assert_eq!(report.verdict, "drift");
        assert!(report.benign_churn.is_empty());
        // Count mismatch (two missing, one added): no guessing, drift.
        let report = apply_benign_churn(
            diff_windows(
                &[offscreen(83633, 801), offscreen(83633, 802)],
                &[offscreen(83633, 829)],
            )
            .expect("joins"),
            &[83633],
        );
        assert_eq!(report.verdict, "drift");
        assert!(report.benign_churn.is_empty());
    }

    #[test]
    fn persistent_drift_blocks_churn_verdict_but_pair_still_annotates() {
        let mut drifted = window(7, 1, 0.0, 640.0);
        drifted.bounds.w = 641.0;
        let before = vec![window(7, 1, 0.0, 640.0), offscreen(83633, 801)];
        let after = vec![drifted, offscreen(83633, 829)];
        let report = apply_benign_churn(diff_windows(&before, &after).expect("joins"), &[83633]);
        assert_eq!(report.changed.len(), 1);
        assert_eq!(report.benign_churn.len(), 1);
        assert_eq!(report.verdict, "drift");
        assert!(!report.clean);
    }
}
