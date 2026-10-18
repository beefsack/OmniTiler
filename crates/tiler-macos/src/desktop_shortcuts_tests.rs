//! Fake-backed Desktop-N ownership tests: preimage discipline, resume,
//! locking, hostile files, and transfer-usability warnings. No live touch
//! on any path: every store is a fake and every path is a temp file; the
//! live SPI is never resolved here (see LiveStore::without_spi for the
//! refusal seam).

use std::path::PathBuf;

use crate::desktop_shortcuts::*;

const CG_CTRL: u64 = 0x0004_0000;

fn enabled(keycode: u32) -> ShortcutEntry {
    ShortcutEntry {
        enabled: true,
        keycode,
        modifiers: CG_CTRL,
    }
}

fn disabled(keycode: u32) -> ShortcutEntry {
    ShortcutEntry {
        enabled: false,
        keycode,
        modifiers: CG_CTRL,
    }
}

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "omnitiler-desktop-test-{}-{}.json",
        std::process::id(),
        name
    ))
}

fn clean(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(lock_path_for(path));
    let _ = std::fs::remove_file(path.with_extension(format!("tmp-{}", std::process::id())));
}

fn read_bytes(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[test]
fn symbolic_ids_follow_silica_base() {
    assert_eq!(desktop_symbolic_id(1), Some(118));
    assert_eq!(desktop_symbolic_id(9), Some(126));
    assert_eq!(desktop_symbolic_id(16), Some(133));
    assert_eq!(desktop_symbolic_id(0), None);
    assert_eq!(desktop_symbolic_id(17), None);
    assert_eq!(symbolic_id_desktop(118), Some(1));
    assert_eq!(symbolic_id_desktop(133), Some(16));
    assert_eq!(symbolic_id_desktop(117), None);
    assert_eq!(symbolic_id_desktop(134), None);
    for desktop in 1..=DESKTOP_MAX {
        let id = desktop_symbolic_id(desktop).unwrap();
        assert_eq!(symbolic_id_desktop(id), Some(desktop));
    }
}

#[test]
fn digit_keycodes_pin_ansi_positions() {
    // kVK_ANSI_* positions in Events.h (CGKeyCode space).
    assert_eq!(desktop_digit_keycode(1), Some(0x12));
    assert_eq!(desktop_digit_keycode(2), Some(0x13));
    assert_eq!(desktop_digit_keycode(3), Some(0x14));
    assert_eq!(desktop_digit_keycode(4), Some(0x15));
    assert_eq!(desktop_digit_keycode(5), Some(0x17));
    assert_eq!(desktop_digit_keycode(6), Some(0x16));
    assert_eq!(desktop_digit_keycode(7), Some(0x1A));
    assert_eq!(desktop_digit_keycode(8), Some(0x1C));
    assert_eq!(desktop_digit_keycode(9), Some(0x19));
    assert_eq!(desktop_digit_keycode(10), Some(0x1D));
    assert_eq!(desktop_digit_keycode(0), None);
    assert_eq!(desktop_digit_keycode(11), None);
    assert_eq!(desktop_digit_keycode(16), None);
    for keycode in [
        KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_7, KEY_8, KEY_9, KEY_0,
    ] {
        assert!(is_digit_keycode(keycode));
    }
    assert!(!is_digit_keycode(0x04));
    assert!(!is_digit_keycode(0x7B));
}

#[test]
fn applied_keeps_the_observed_chord() {
    let custom = ShortcutEntry {
        enabled: false,
        keycode: KEY_2,
        modifiers: CG_CTRL | 0x0002_0000,
    };
    assert_eq!(
        custom.applied(),
        ShortcutEntry {
            enabled: true,
            ..custom
        }
    );
}

#[test]
fn restore_decision_matrix() {
    let applied = enabled(KEY_2);
    let preimage = disabled(KEY_2);
    // Still applied: restore the preimage enabled flag.
    assert_eq!(
        decide_restore(preimage, applied, applied),
        RestoreAction::SetEnabled(false)
    );
    // Already at preimage: no write either way.
    assert_eq!(
        decide_restore(preimage, applied, preimage),
        RestoreAction::AlreadyRestored
    );
    // Apply was a no-op (already enabled): current equals both — the
    // preimage wins, no write.
    assert_eq!(
        decide_restore(applied, applied, applied),
        RestoreAction::AlreadyRestored
    );
    // External edits (different chord, flipped enabled, removed-then-
    // reappearing foreign entry) are preserved, never overwritten.
    let foreign_chord = ShortcutEntry {
        enabled: true,
        keycode: KEY_3,
        modifiers: CG_CTRL,
    };
    assert_eq!(
        decide_restore(preimage, applied, foreign_chord),
        RestoreAction::KeepExternal
    );
    assert_eq!(
        decide_restore(preimage, applied, enabled(KEY_2).applied()),
        RestoreAction::SetEnabled(false)
    );
    let externally_disabled = disabled(KEY_2);
    // Disabled current with a disabled preimage reads as restored…
    assert_eq!(
        decide_restore(preimage, applied, externally_disabled),
        RestoreAction::AlreadyRestored
    );
    // …while a chord change under the same flag is external.
    let same_flag_other_chord = ShortcutEntry {
        enabled: false,
        keycode: KEY_3,
        modifiers: CG_CTRL,
    };
    assert_eq!(
        decide_restore(preimage, applied, same_flag_other_chord),
        RestoreAction::KeepExternal
    );
}

#[test]
fn apply_publishes_preimage_then_enables_with_readback() {
    let path = test_path("apply");
    clean(&path);
    let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
    // Desktop 2 unresolvable: per-ID failure, never a guess.
    let outcome = apply_with_store_to(&mut store, 2, &path).unwrap();
    assert_eq!(outcome.results.len(), 1);
    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].stage, "read");
    assert!(outcome.external_modified.is_empty());
    assert!(!outcome.resumed);
    assert!(outcome.session_only);
    assert_eq!(store.get(118), Some(enabled(KEY_1)));
    // Durable preimage published before the write: the exact observed
    // session subset, applied as the same chord enabled.
    let file = load_preimage_file_from(&path).unwrap();
    assert_eq!(file.version, PREIMAGE_SCHEMA_VERSION);
    assert_eq!(file.desktops.len(), 1);
    assert_eq!(file.desktops[0].preimage, disabled(KEY_1));
    assert_eq!(file.desktops[0].applied, enabled(KEY_1));
    clean(&path);
}

#[test]
fn apply_rejects_counts_outside_digits_before_any_touch() {
    let path = test_path("badcount");
    clean(&path);
    let mut store = FakeStore::new();
    // Capped to digit-supported IDs: 0 and 11+ are usage errors with
    // no file and no writes.
    assert!(apply_with_store_to(&mut store, 0, &path).is_err());
    assert!(apply_with_store_to(&mut store, 11, &path).is_err());
    assert!(apply_with_store_to(&mut store, 16, &path).is_err());
    assert_eq!(store.writes, 0);
    assert!(!path.exists(), "no preimage on validation refusal");
    clean(&path);
}

#[test]
fn apply_partial_failure_keeps_preimage_and_siblings() {
    let path = test_path("partial");
    clean(&path);
    let mut store = FakeStore::new()
        .with_entry(118, disabled(KEY_1))
        .with_entry(119, disabled(KEY_2))
        .with_entry(120, disabled(KEY_3));
    store.fail_sets.insert(119);
    let outcome = apply_with_store_to(&mut store, 3, &path).unwrap();
    assert_eq!(outcome.results.len(), 2);
    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].symbolic_id, 119);
    assert_eq!(outcome.failures[0].stage, "write");
    assert_eq!(store.get(118), Some(enabled(KEY_1)));
    assert_eq!(store.get(120), Some(enabled(KEY_3)));
    // The failed ID kept its preimage value; the file is retained for
    // the resume retry.
    assert_eq!(store.get(119), Some(disabled(KEY_2)));
    assert!(path.exists());
    clean(&path);
}

#[test]
fn apply_publish_failure_writes_nothing() {
    // Unwritable preimage path: the durable publication refuses before
    // the first settings write, so the store is untouched.
    let bad = std::path::PathBuf::from("/nonexistent-dir-omnitiler/desktop.json");
    let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
    assert!(apply_with_store_to(&mut store, 1, &bad).is_err());
    assert_eq!(store.get(118), Some(disabled(KEY_1)));
    assert_eq!(store.writes, 0);
}

#[test]
fn second_apply_resumes_without_rewriting_or_overwriting() {
    let path = test_path("resume");
    clean(&path);
    let mut store = FakeStore::new()
        .with_entry(118, disabled(KEY_1))
        .with_entry(119, disabled(KEY_2));
    store.fail_sets.insert(119);
    let first = apply_with_store_to(&mut store, 2, &path).unwrap();
    assert_eq!(first.failures.len(), 1);
    let bytes_before = read_bytes(&path);
    let writes_before = store.writes;
    // Retry after the fault clears: the failed ID applies from its
    // stored first-observed preimage; the applied ID verifies without
    // a rewrite.
    store.fail_sets.remove(&119);
    let second = apply_with_store_to(&mut store, 2, &path).unwrap();
    assert!(second.resumed);
    assert!(second.failures.is_empty());
    assert!(second.external_modified.is_empty());
    assert_eq!(second.results.len(), 2);
    assert!(second.results.iter().all(|r| r.resumed));
    assert_eq!(store.get(118), Some(enabled(KEY_1)));
    assert_eq!(store.get(119), Some(enabled(KEY_2)));
    // Exactly one enable write happened on resume (the previously
    // failed ID); the original backup file is byte-identical.
    assert_eq!(store.writes, writes_before + 1);
    assert_eq!(read_bytes(&path), bytes_before);
    clean(&path);
}

#[test]
fn second_apply_with_narrower_count_keeps_extra_records() {
    let path = test_path("narrow");
    clean(&path);
    let mut store = FakeStore::new()
        .with_entry(118, disabled(KEY_1))
        .with_entry(119, disabled(KEY_2));
    let first = apply_with_store_to(&mut store, 2, &path).unwrap();
    assert!(first.failures.is_empty());
    // Narrowing to Desktop 1 resumes against the stored preimage; the
    // Desktop 2 record stays owned and applied for a later full Revert.
    let second = apply_with_store_to(&mut store, 1, &path).unwrap();
    assert!(second.resumed);
    assert!(second.failures.is_empty());
    assert_eq!(second.results.len(), 1);
    let file = load_preimage_file_from(&path).unwrap();
    assert_eq!(file.desktops.len(), 2);
    assert_eq!(store.get(119), Some(enabled(KEY_2)));
    clean(&path);
}

#[test]
fn resume_never_restores_or_overwrites_external_edits() {
    let path = test_path("resume-external");
    clean(&path);
    let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
    let first = apply_with_store_to(&mut store, 1, &path).unwrap();
    assert!(first.failures.is_empty());
    // An external chord change after Apply: the resume must preserve
    // it, never re-apply over it.
    let edited = ShortcutEntry {
        enabled: true,
        keycode: KEY_3,
        modifiers: CG_CTRL,
    };
    store.entries.insert(118, edited);
    let second = apply_with_store_to(&mut store, 1, &path).unwrap();
    assert_eq!(second.results.len(), 0);
    assert_eq!(second.external_modified, vec![118]);
    assert_eq!(second.failures.len(), 1);
    assert_eq!(second.failures[0].stage, "external-preserved");
    assert_eq!(store.get(118), Some(edited));
    clean(&path);
}

#[test]
fn revert_restores_enabled_flags() {
    let path = test_path("revert");
    clean(&path);
    // Desktop 1 starts disabled (Apply flips it); Desktop 2 starts
    // enabled (Apply is a verified no-op, still owned).
    let mut store = FakeStore::new()
        .with_entry(118, disabled(KEY_1))
        .with_entry(119, enabled(KEY_2));
    let applied = apply_with_store_to(&mut store, 2, &path).unwrap();
    assert!(applied.failures.is_empty());
    let revert = revert_with_store_from(&mut store, &path).unwrap();
    assert!(revert.failures.is_empty());
    assert!(revert.external_modified.is_empty());
    assert!(!revert.preimage_retained, "clean revert removes the file");
    assert!(!path.exists());
    assert_eq!(store.get(118), Some(disabled(KEY_1)));
    assert_eq!(store.get(119), Some(enabled(KEY_2)));
    clean(&path);
}

#[test]
fn revert_preserves_later_external_edits() {
    let path = test_path("external");
    clean(&path);
    let mut store = FakeStore::new()
        .with_entry(118, disabled(KEY_1))
        .with_entry(119, disabled(KEY_2));
    let applied = apply_with_store_to(&mut store, 2, &path).unwrap();
    assert!(applied.failures.is_empty());
    // User changes Desktop 1's chord after Apply; admin rebinds
    // Desktop 2's chord externally (same enabled flag, other chord).
    let edited = ShortcutEntry {
        enabled: true,
        keycode: KEY_1,
        modifiers: CG_CTRL | 0x0002_0000,
    };
    let rebound = ShortcutEntry {
        enabled: true,
        keycode: KEY_9,
        modifiers: CG_CTRL,
    };
    store.entries.insert(118, edited);
    store.entries.insert(119, rebound);
    let revert = revert_with_store_from(&mut store, &path).unwrap();
    assert!(revert.failures.is_empty());
    assert_eq!(revert.external_modified, vec![118, 119]);
    assert!(revert.preimage_retained, "external edits retain the file");
    assert!(path.exists());
    // Neither external edit was overwritten.
    assert_eq!(store.get(118), Some(edited));
    assert_eq!(store.get(119), Some(rebound));
    clean(&path);
}

#[test]
fn revert_without_preimage_refuses_before_store_touch() {
    let path = test_path("missing");
    clean(&path);
    let mut store = FakeStore::new().with_entry(118, enabled(KEY_1));
    let reads_before = store.reads;
    assert!(revert_with_store_from(&mut store, &path).is_err());
    assert_eq!(store.reads, reads_before, "no store touch on refusal");
    assert_eq!(store.get(118), Some(enabled(KEY_1)));
    clean(&path);
}

#[test]
fn revert_write_failure_retains_preimage() {
    let path = test_path("revertfail");
    clean(&path);
    let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
    let applied = apply_with_store_to(&mut store, 1, &path).unwrap();
    assert!(applied.failures.is_empty());
    store.fail_sets.insert(118);
    let revert = revert_with_store_from(&mut store, &path).unwrap();
    assert_eq!(revert.failures.len(), 1);
    assert_eq!(revert.failures[0].stage, "write");
    assert!(revert.preimage_retained);
    assert!(path.exists());
    clean(&path);
}

#[test]
fn hostile_preimage_files_refuse_without_store_touch() {
    let path = test_path("hostile");
    let store = FakeStore::new().with_entry(118, disabled(KEY_1));
    let writes_before = store.writes;
    let refuses = |raw: &str| {
        clean(&path);
        std::fs::write(&path, raw).unwrap();
        let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
        assert!(
            revert_with_store_from(&mut store, &path).is_err(),
            "hostile file accepted: {raw}"
        );
        // A hostile file must also block Apply resume before writes.
        let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
        assert!(apply_with_store_to(&mut store, 1, &path).is_err());
    };
    refuses("{not json");
    refuses(&serde_json::json!({"version": 99, "desktops": []}).to_string());
    refuses(&serde_json::json!({"version": 1, "desktops": []}).to_string());
    // Out-of-range ID.
    refuses(
        &serde_json::to_string(&DesktopPreimageFile {
            version: 1,
            desktops: vec![DesktopPreimageRecord {
                symbolic_id: 79,
                desktop: 1,
                preimage: disabled(KEY_1),
                applied: enabled(KEY_1),
            }],
        })
        .unwrap(),
    );
    // Desktop/ID mismatch.
    refuses(
        &serde_json::to_string(&DesktopPreimageFile {
            version: 1,
            desktops: vec![DesktopPreimageRecord {
                symbolic_id: 118,
                desktop: 2,
                preimage: disabled(KEY_1),
                applied: enabled(KEY_1),
            }],
        })
        .unwrap(),
    );
    // Duplicate IDs.
    refuses(
        &serde_json::to_string(&DesktopPreimageFile {
            version: 1,
            desktops: vec![
                DesktopPreimageRecord {
                    symbolic_id: 118,
                    desktop: 1,
                    preimage: disabled(KEY_1),
                    applied: enabled(KEY_1),
                },
                DesktopPreimageRecord {
                    symbolic_id: 118,
                    desktop: 1,
                    preimage: disabled(KEY_1),
                    applied: enabled(KEY_1),
                },
            ],
        })
        .unwrap(),
    );
    // Keycode out of range.
    refuses(
        &serde_json::to_string(&DesktopPreimageFile {
            version: 1,
            desktops: vec![DesktopPreimageRecord {
                symbolic_id: 118,
                desktop: 1,
                preimage: ShortcutEntry {
                    enabled: false,
                    keycode: 999,
                    modifiers: CG_CTRL,
                },
                applied: enabled(KEY_1),
            }],
        })
        .unwrap(),
    );
    // Disabled applied entry.
    refuses(
        &serde_json::to_string(&DesktopPreimageFile {
            version: 1,
            desktops: vec![DesktopPreimageRecord {
                symbolic_id: 118,
                desktop: 1,
                preimage: disabled(KEY_1),
                applied: disabled(KEY_1),
            }],
        })
        .unwrap(),
    );
    // Applied chord differing from the preimage chord (the enable-only
    // invariant): a file pretending a chord write is refused.
    refuses(
        &serde_json::to_string(&DesktopPreimageFile {
            version: 1,
            desktops: vec![DesktopPreimageRecord {
                symbolic_id: 118,
                desktop: 1,
                preimage: disabled(KEY_1),
                applied: enabled(KEY_3),
            }],
        })
        .unwrap(),
    );
    // None of the hostile files moved the store.
    assert_eq!(store.get(118), Some(disabled(KEY_1)));
    assert_eq!(store.writes, writes_before);
    clean(&path);
}

#[test]
fn publish_refuses_overwrite() {
    let path = test_path("overwrite");
    clean(&path);
    let file = DesktopPreimageFile {
        version: PREIMAGE_SCHEMA_VERSION,
        desktops: vec![DesktopPreimageRecord {
            symbolic_id: 118,
            desktop: 1,
            preimage: disabled(KEY_1),
            applied: enabled(KEY_1),
        }],
    };
    publish_preimage_file_to(&path, &file).unwrap();
    let bytes = read_bytes(&path);
    // A second publish — even of identical content — refuses rather
    // than overwriting the original backup.
    assert!(publish_preimage_file_to(&path, &file).is_err());
    assert_eq!(read_bytes(&path), bytes);
    clean(&path);
}

#[test]
fn live_store_without_spi_refuses_every_op() {
    let mut live = LiveStore::without_spi();
    let read_err = live.read(118).unwrap_err();
    assert!(read_err.contains("unavailable"), "{read_err}");
    let write_err = live.set_enabled(118, true).unwrap_err();
    assert!(write_err.contains("unavailable"), "{write_err}");
}

#[test]
fn digit_corroboration_needs_exact_enabled_ctrl_digits() {
    use crate::ffi::SymbolicHotKeyEntry;
    let entry = |keycode: u32, mods: u32, enabled: bool| SymbolicHotKeyEntry {
        keycode,
        carbon_modifiers: mods,
        enabled,
    };
    let ctrl = crate::symbolic_hotkeys::CARBON_CTRL_BIT;
    let entries = vec![
        entry(KEY_2, ctrl | crate::symbolic_hotkeys::CARBON_FN_BIT, true),
        entry(KEY_3, ctrl, false),
        entry(KEY_3, ctrl | 0x200, true),
        entry(0x04, ctrl, true),
    ];
    let report = corroborate_desktop_digits(&entries);
    assert_eq!(report.enabled, vec![2]);
    assert_eq!(report.entries_seen, 4);
    let empty = corroborate_desktop_digits(&[]);
    assert!(empty.enabled.is_empty());
}

#[test]
fn preimage_round_trip_is_exact() {
    let file = DesktopPreimageFile {
        version: PREIMAGE_SCHEMA_VERSION,
        desktops: vec![
            DesktopPreimageRecord {
                symbolic_id: 118,
                desktop: 1,
                preimage: disabled(KEY_1),
                applied: enabled(KEY_1),
            },
            DesktopPreimageRecord {
                symbolic_id: 119,
                desktop: 2,
                preimage: enabled(KEY_2),
                applied: enabled(KEY_2),
            },
        ],
    };
    let json = serde_json::to_string(&file).unwrap();
    let back: DesktopPreimageFile = serde_json::from_str(&json).unwrap();
    assert_eq!(file, back);
    assert!(validate_preimage_file(&back).is_ok());
}

#[test]
fn ctrl_domain_pins_the_synthetic_switch_shape() {
    // The default expected chord posts exactly the tap's switch shape:
    // digit keycode plus the CG Control bit.
    assert_eq!(CG_EVENT_CTRL_BIT, crate::input_match::SWITCH_CTRL_BIT);
    assert_eq!(
        default_expected_chord(2),
        Some(ShortcutEntry {
            enabled: true,
            keycode: KEY_2,
            modifiers: CG_EVENT_CTRL_BIT,
        })
    );
    assert_eq!(default_expected_chord(11), None);
    assert!(transfer_usable(2, enabled(KEY_2)));
    assert!(!transfer_usable(
        2,
        ShortcutEntry {
            enabled: false,
            keycode: KEY_2,
            modifiers: CG_EVENT_CTRL_BIT | 0x0002_0000,
        }
    ));
}

#[test]
fn numbered_ordinals_come_from_sls_position() {
    // 1-based SLS position; unmapped SIDs refuse (never guessed).
    assert_eq!(numbered_desktop_for_sid(&[277, 1], 277), Some(1));
    assert_eq!(numbered_desktop_for_sid(&[277, 1], 1), Some(2));
    // Corrected live shape: SLS [1, 277] puts original 277 at ordinal 2
    // (Desktop 2) and temp 1 at ordinal 1 (Desktop 1).
    assert_eq!(numbered_desktop_for_sid(&[1, 277], 277), Some(2));
    assert_eq!(numbered_desktop_for_sid(&[1, 277], 1), Some(1));
    assert_eq!(numbered_desktop_for_sid(&[1, 277], 999), None);
    assert_eq!(numbered_desktop_for_sid(&[7], 7), Some(1));
    assert_eq!(numbered_desktop_for_sid(&[7], 8), None);
    assert_eq!(numbered_desktop_for_sid(&[], 7), None);
    // Ordinals past 10 resolve here but stay unmappable downstream (no
    // default digit chord): the live gate refuses through resolve below.
    assert_eq!(
        numbered_desktop_for_sid(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], 11),
        Some(11)
    );
}

#[test]
fn numbered_chord_needs_enabled_standard_holder() {
    // Standard enabled holder: Control-only digit post shape.
    assert_eq!(
        resolve_numbered_chord(2, enabled(KEY_2)),
        Ok((KEY_2, CG_CTRL))
    );
    assert_eq!(
        resolve_numbered_chord(2, enabled(KEY_2)).unwrap().1,
        crate::input_match::SWITCH_CTRL_BIT,
        "digits post Control-only (arrows alone carry SecondaryFn)"
    );
    // Disabled holder refuses (never a settings write to enable it).
    assert!(resolve_numbered_chord(2, disabled(KEY_2)).is_err());
    // Customized binding refuses even when enabled (never post a chord
    // the holder does not own).
    assert!(
        resolve_numbered_chord(
            2,
            ShortcutEntry {
                enabled: true,
                keycode: KEY_2,
                modifiers: CG_CTRL | 0x0002_0000,
            }
        )
        .is_err()
    );
    assert!(
        resolve_numbered_chord(
            2,
            ShortcutEntry {
                enabled: true,
                keycode: KEY_3,
                modifiers: CG_CTRL,
            }
        )
        .is_err()
    );
    // Unrepresentable keycode refuses rather than wrapping at the post.
    assert!(
        resolve_numbered_chord(
            2,
            ShortcutEntry {
                enabled: true,
                keycode: 256,
                modifiers: CG_CTRL,
            }
        )
        .is_err()
    );
    // Desktops without a default digit chord are unmappable (never
    // guessed), even for an enabled standard-looking holder.
    assert!(resolve_numbered_chord(0, enabled(KEY_2)).is_err());
    assert!(resolve_numbered_chord(11, enabled(KEY_2)).is_err());
    assert!(resolve_numbered_chord(16, enabled(KEY_2)).is_err());
}

#[test]
fn numbered_pair_resolves_1_277_outward_ctrl1_inverse_ctrl2() {
    // Corrected live shape: SLS [1, 277] with original 277 at ordinal 2
    // (Desktop 2, holder 119) and temp 1 at ordinal 1 (Desktop 1,
    // holder 118). Outward posts the temp chord (Ctrl+1), the inverse the
    // original chord (Ctrl+2).
    let mut store = FakeStore::new()
        .with_entry(119, enabled(KEY_2))
        .with_entry(118, enabled(KEY_1));
    let pair = resolve_numbered_pair(&mut store, &[1, 277], 277, 1).unwrap();
    assert_eq!(pair.outward_desktop, 1);
    assert_eq!(pair.return_desktop, 2);
    assert_eq!(pair.outward_symbolic, 118);
    assert_eq!(pair.return_symbolic, 119);
    assert_eq!(pair.outward_key, KEY_1);
    assert_eq!(pair.return_key, KEY_2);
    assert_eq!(pair.outward_flags, CG_CTRL);
    assert_eq!(pair.return_flags, CG_CTRL);
    assert_eq!(store.writes, 0, "resolution never writes settings");
}

#[test]
fn numbered_pair_refuses_before_any_post_with_injected_store() {
    // Unmapped temp SID: refuses before touching the store at all.
    let mut store = FakeStore::new()
        .with_entry(119, enabled(KEY_2))
        .with_entry(118, enabled(KEY_1));
    let err = resolve_numbered_pair(&mut store, &[1, 277], 277, 999).unwrap_err();
    assert!(matches!(err, NumberedResolveError::Gate(_)), "{err:?}");
    assert_eq!(store.reads, 0, "unmapped SID touches no holder");
    assert_eq!(store.writes, 0);
    // Unreadable original holder (injected read failure): refuses as a
    // system error with zero writes (the live runner preserves the split).
    let mut store = FakeStore::new()
        .with_entry(119, enabled(KEY_2))
        .with_entry(118, enabled(KEY_1));
    store.fail_reads.insert(119);
    let err = resolve_numbered_pair(&mut store, &[1, 277], 277, 1).unwrap_err();
    assert!(
        matches!(err, NumberedResolveError::Unreadable(_)),
        "{err:?}"
    );
    assert_eq!(store.writes, 0);
    // Disabled temp holder: gate refusal, zero writes.
    let mut store = FakeStore::new()
        .with_entry(119, enabled(KEY_2))
        .with_entry(118, disabled(KEY_1));
    let err = resolve_numbered_pair(&mut store, &[1, 277], 277, 1).unwrap_err();
    assert!(matches!(err, NumberedResolveError::Gate(_)), "{err:?}");
    assert_eq!(store.writes, 0);
    // Customized temp holder (Ctrl+Shift+1): gate refusal, never rewritten.
    let mut store = FakeStore::new().with_entry(119, enabled(KEY_2)).with_entry(
        118,
        ShortcutEntry {
            enabled: true,
            keycode: KEY_1,
            modifiers: CG_CTRL | 0x0002_0000,
        },
    );
    let err = resolve_numbered_pair(&mut store, &[1, 277], 277, 1).unwrap_err();
    assert!(matches!(err, NumberedResolveError::Gate(_)), "{err:?}");
    assert_eq!(store.writes, 0);
    assert_eq!(store.get(118).unwrap().modifiers, CG_CTRL | 0x0002_0000);
    // Unmappable ordinal 11 (holder exists, no default digit chord): gate
    // refusal even for an enabled standard-looking holder.
    let order: Vec<u64> = (1..=11).collect();
    let mut store = FakeStore::new().with_entry(118, enabled(KEY_1)).with_entry(
        128,
        ShortcutEntry {
            enabled: true,
            keycode: KEY_1,
            modifiers: CG_CTRL,
        },
    );
    let err = resolve_numbered_pair(&mut store, &order, 1, 11).unwrap_err();
    assert!(matches!(err, NumberedResolveError::Gate(_)), "{err:?}");
    assert_eq!(store.writes, 0);
    // Ordinal past Desktop 16: no holder id at all, gate refusal.
    let order: Vec<u64> = (1..=17).collect();
    let mut store = FakeStore::new().with_entry(118, enabled(KEY_1));
    let err = resolve_numbered_pair(&mut store, &order, 1, 17).unwrap_err();
    assert!(matches!(err, NumberedResolveError::Gate(_)), "{err:?}");
    assert_eq!(store.writes, 0);
}

#[test]
fn transfer_mismatch_warns_without_rewriting_chords() {
    let path = test_path("mismatch");
    clean(&path);
    // A customized holder chord (Ctrl+Shift+1): Apply enables it but must
    // flag it unusable for the default transfer — never rewrite it.
    let custom = ShortcutEntry {
        enabled: false,
        keycode: KEY_1,
        modifiers: CG_EVENT_CTRL_BIT | 0x0002_0000,
    };
    let mut store = FakeStore::new().with_entry(118, custom);
    let outcome = apply_with_store_to(&mut store, 1, &path).unwrap();
    assert!(outcome.failures.is_empty());
    assert_eq!(outcome.results.len(), 1);
    let result = &outcome.results[0];
    assert!(!result.transfer_usable);
    assert_eq!(result.expected_chord, default_expected_chord(1));
    assert_eq!(outcome.transfer_warnings.len(), 1);
    assert!(outcome.transfer_warnings[0].contains("desktop 1"));
    // Enabled in place; the custom chord is byte-identical.
    assert_eq!(
        store.get(118),
        Some(ShortcutEntry {
            enabled: true,
            ..custom
        })
    );
    // Revert restores the flag, keeps the chord, and still warns.
    let revert = revert_with_store_from(&mut store, &path).unwrap();
    assert!(revert.failures.is_empty());
    assert_eq!(revert.transfer_warnings.len(), 1);
    assert_eq!(store.get(118), Some(custom));
    clean(&path);
}

#[test]
fn lock_releases_on_drop_and_relocks() {
    // The design relies on close-on-drop release (and OS release on
    // process loss): dropping the guard must free the leaf for the next
    // transaction, with no stale state.
    let path = test_path("lock-release");
    clean(&path);
    let lock_path = lock_path_for(&path);
    {
        let _held = lock_leaf_file(&lock_path).expect("first acquire");
        assert_eq!(
            lock_leaf_file(&lock_path).expect_err("held must refuse"),
            LockRefusal::Held
        );
    }
    lock_leaf_file(&lock_path).expect("re-acquire after drop");
    clean(&path);
}

#[test]
fn lock_contention_refuses_apply_and_revert_with_zero_writes() {
    let path = test_path("locked");
    clean(&path);
    let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
    // Hold the transaction lock out-of-band (as a second process would).
    let _held = lock_leaf_file(&lock_path_for(&path)).expect("test lock");
    let writes_before = store.writes;
    let error = apply_with_store_to(&mut store, 1, &path).unwrap_err();
    assert!(error.contains("lock"), "{error}");
    assert_eq!(store.writes, writes_before, "zero writes under contention");
    assert_eq!(store.get(118), Some(disabled(KEY_1)));
    assert!(!path.exists(), "no backup published under contention");
    // Revert refuses the same way (file or no file: the lock comes first).
    let error = revert_with_store_from(&mut store, &path).unwrap_err();
    assert!(error.contains("lock"), "{error}");
    assert_eq!(store.writes, writes_before);
    drop(_held);
    clean(&path);
}

#[test]
fn concurrent_apply_serializes_first_preimage_and_all_records() {
    let path = test_path("concurrent");
    clean(&path);
    let (acquired_tx, acquired_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let lock_path = lock_path_for(&path);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let _held = lock_leaf_file(&lock_path).expect("test lock");
            acquired_tx.send(()).expect("signal held");
            release_rx.recv().expect("wait release");
        });
        acquired_rx.recv().expect("lock held by sibling");
        // While the sibling holds the transaction, our Apply refuses with
        // zero writes and no backup — the first preimage cannot be torn.
        let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
        assert!(apply_with_store_to(&mut store, 1, &path).is_err());
        assert_eq!(store.writes, 0);
        assert!(!path.exists());
        release_tx.send(()).expect("release sibling");
    });
    // After release the same Apply completes; a later resume keeps every
    // record and the first preimage byte-identically.
    let mut store = FakeStore::new().with_entry(118, disabled(KEY_1));
    let outcome = apply_with_store_to(&mut store, 1, &path).unwrap();
    assert!(outcome.failures.is_empty());
    let bytes_before = read_bytes(&path);
    let second = apply_with_store_to(&mut store, 1, &path).unwrap();
    assert!(second.resumed);
    assert!(second.failures.is_empty());
    assert_eq!(read_bytes(&path), bytes_before);
    let file = load_preimage_file_from(&path).unwrap();
    assert_eq!(file.desktops.len(), 1);
    assert_eq!(file.desktops[0].preimage, disabled(KEY_1));
    clean(&path);
}

fn test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "omnitiler-desktop-dir-{}-{}",
        std::process::id(),
        name
    ))
}

#[test]
fn canonical_paths_derive_from_durable_app_support() {
    // Pure derivation, no FS touch: the recovery backup and its lock live
    // under the per-user Application Support subdirectory (the parent of
    // the settings file, shared convention), never the temporary probe
    // session dir. No home means no path — never a tmp fallback.
    assert_eq!(
        recovery_dir_for(Some("/fake-home")),
        Some(PathBuf::from(
            "/fake-home/Library/Application Support/OmniTiler"
        ))
    );
    assert_eq!(
        canonical_preimage_path_for(Some("/fake-home")),
        Some(PathBuf::from(
            "/fake-home/Library/Application Support/OmniTiler/desktop-shortcuts-preimage.json"
        ))
    );
    assert_eq!(RECOVERY_FILE_NAME, "desktop-shortcuts-preimage.json");
    assert_eq!(recovery_dir_for(None), None);
    assert_eq!(canonical_preimage_path_for(None), None);
    // The durable dir is the settings file's parent: the two conventions
    // cannot drift apart.
    let settings =
        crate::dev_selection::settings_path_for(Some("/fake-home")).expect("settings path");
    assert_eq!(
        settings.parent().map(|p| p.to_path_buf()),
        recovery_dir_for(Some("/fake-home"))
    );
}

#[test]
fn ensure_private_dir_hardens_fake_dirs() {
    // Fake temp paths only: fresh dirs create private, existing private
    // dirs pass, symlinks and non-dirs refuse.
    let dir = test_dir("ensure");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&dir);
    ensure_private_dir(&dir).expect("fresh dir creates");
    assert!(dir.is_dir());
    ensure_private_dir(&dir).expect("existing dir passes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "recovery dir must be private");
    }
    let _ = std::fs::remove_dir_all(&dir);
    // A regular file at the dir path refuses.
    std::fs::write(&dir, b"not a dir").unwrap();
    assert!(ensure_private_dir(&dir).is_err());
    let _ = std::fs::remove_file(&dir);
    #[cfg(unix)]
    {
        // A symlink at the dir path refuses.
        std::os::unix::fs::symlink("/tmp", &dir).unwrap();
        assert!(ensure_private_dir(&dir).is_err());
        let _ = std::fs::remove_file(&dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn publish_leaves_0600_backup() {
    // Fake temp path only: the published durable backup is owner-only.
    let path = test_path("mode0600");
    clean(&path);
    let file = DesktopPreimageFile {
        version: PREIMAGE_SCHEMA_VERSION,
        desktops: vec![DesktopPreimageRecord {
            symbolic_id: 118,
            desktop: 1,
            preimage: disabled(KEY_1),
            applied: enabled(KEY_1),
        }],
    };
    publish_preimage_file_to(&path, &file).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "durable backup must be owner-only");
    }
    clean(&path);
}

#[test]
#[cfg(unix)]
fn parent_sync_ok_and_refusal_are_honest() {
    // The durability gate itself: an existing parent syncs, a missing
    // parent refuses (publish/merge/remove propagate this as a
    // zero-further-writes failure).
    let path = test_path("parentsync");
    clean(&path);
    assert!(sync_parent_dir(&path).is_ok());
    let missing = PathBuf::from("/nonexistent-dir-omnitiler/child.json");
    assert!(sync_parent_dir(&missing).is_err());
}
