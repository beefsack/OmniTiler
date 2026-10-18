#![forbid(unsafe_code)]
use super::*;
use crate::settings::{
    NATIVE_PANELS, WINDOWS_FIELD_MAPPINGS, WINDOWS_ONLY_FIELDS, WINDOWS_UNSUPPORTED_FIELDS,
};
use std::collections::BTreeSet;
use tiler_core::settings_catalog::{ALL_FIELDS, ALL_TABS, find_section, find_tab};

fn px(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32
}

#[test]
fn four_tabs_come_from_the_catalog() {
    assert_eq!(TAB_COUNT, ALL_TABS.len());
    assert_eq!(TAB_COUNT, 4);
    let titles = tab_titles();
    assert_eq!(
        titles,
        [
            "Tiling & Workspaces",
            "Appearance",
            "Shortcuts",
            "Desktop Integration"
        ]
    );
    for (index, title) in titles.iter().enumerate() {
        let tab = find_tab(ALL_TABS[index].id).expect("known tab");
        assert_eq!(&tab.label, title);
    }
}

#[test]
fn windows_sections_point_at_known_tabs() {
    assert_eq!(WINDOWS_SECTIONS.len(), 6);
    let mut seen = BTreeSet::new();
    for section in WINDOWS_SECTIONS {
        assert!(seen.insert(section.id), "duplicate {}", section.id);
        assert!(find_tab(section.tab).is_some(), "{}", section.id);
        assert!(!section.label.is_empty(), "{}", section.id);
        assert!(section_tab_index(section.id).is_some(), "{}", section.id);
    }
}

#[test]
fn every_scalar_field_renders_exactly_once() {
    assert_eq!(WINDOWS_FIELD_MAPPINGS.len(), 23);
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for id in ALL_CONTROL_IDS {
        if let Some(field) = control_field(*id) {
            seen.insert(field);
        }
    }
    for entry in WINDOWS_FIELD_MAPPINGS {
        assert!(seen.contains(entry.field), "unrendered {}", entry.field);
    }
    assert_eq!(seen.len(), WINDOWS_FIELD_MAPPINGS.len());
    for field in ["gaps.inner", "border.color", "keyboard.takeover"] {
        let count = ALL_CONTROL_IDS
            .iter()
            .filter(|id| control_field(**id) == Some(field))
            .count();
        assert_eq!(count, 1, "{field}");
    }
    for (field, want) in [
        ("move.same-axis", 2),
        ("workspace.mode", 3),
        ("workspace.default-tiled", 2),
    ] {
        let count = ALL_CONTROL_IDS
            .iter()
            .filter(|id| control_field(**id) == Some(field))
            .count();
        assert_eq!(count, want, "{field}");
    }
    // Every enum option renders exactly one radio: option tables and
    // radio counts agree per field.
    for entry in WINDOWS_FIELD_MAPPINGS {
        let def = windows_field_def(entry.field).expect("descriptor");
        if matches!(
            def.field_type,
            FieldType::Enum if entry.field != "workspace.default-tiled"
        ) {
            let radios = ALL_CONTROL_IDS
                .iter()
                .filter(|id| control_field(**id) == Some(entry.field))
                .count();
            assert_eq!(radios, def.options.len(), "{}", entry.field);
        }
    }
}

#[test]
fn native_slots_consumed_once_each() {
    assert_eq!(NATIVE_PANELS.len(), 6);
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for id in ALL_CONTROL_IDS {
        if let Some(panel) = control_panel(*id) {
            seen.insert(panel);
        }
    }
    for panel in NATIVE_PANELS {
        assert!(seen.contains(panel.id), "unconsumed {}", panel.id);
        assert_eq!(panel_tab_index(panel.id), section_tab_index(panel.section));
    }
    assert_eq!(seen.len(), NATIVE_PANELS.len());
    // Scalar editors never double as action slots.
    for id in ALL_CONTROL_IDS {
        if control_field(*id).is_some() {
            let action = matches!(*id, ID_KEYBOARD_TAKEOVER | ID_ALLOW_WIN_L | ID_MOUSE_SNAP);
            if !action {
                assert_eq!(control_panel(*id), None, "scalar {id} must not trigger ops");
            }
        }
    }
    // Every action control sits inside its own panel's groupbox.
    for id in ALL_CONTROL_IDS {
        let Some(panel) = control_panel(*id) else {
            continue;
        };
        let center = control_center(*id).expect("rect");
        let owners: Vec<_> = GROUPS
            .iter()
            .filter(|entry| entry.0 == GroupKey::Panel(panel))
            .filter(|entry| group_contains(entry, center))
            .collect();
        assert_eq!(owners.len(), 1, "{id} outside its {panel} group");
    }
}

#[test]
fn control_text_comes_from_the_catalog() {
    for field in ALL_FIELDS.iter().chain(WINDOWS_ONLY_FIELDS.iter()) {
        // Portable fields without a Windows mapping stay off the dialog;
        // the unsupported partition is currently empty since the runtime
        // adoption maps both admission settings.
        if WINDOWS_UNSUPPORTED_FIELDS.contains(&field.id) {
            continue;
        }
        let rendered: Vec<u32> = ALL_CONTROL_IDS
            .iter()
            .copied()
            .filter(|id| control_field(*id) == Some(field.id))
            .collect();
        assert!(!rendered.is_empty(), "{}", field.id);
        let first = control_text(rendered[0]);
        if field.id == "workspace.default-tiled" {
            assert_eq!(control_text(rendered[0]), "Tiled");
            assert_eq!(control_text(rendered[1]), "Floating");
            continue;
        }
        match field.field_type {
            FieldType::Bool => assert_eq!(first, field.label, "{}", field.id),
            FieldType::Enum => {
                for (index, id) in rendered.iter().enumerate() {
                    assert_eq!(
                        control_text(*id),
                        field.options[index].label,
                        "{}",
                        field.id
                    );
                }
            }
            _ => {}
        }
        assert!(!field.help.is_empty(), "{}", field.id);
        assert!(!field.timing_help.is_empty(), "{}", field.id);
    }
    for section in tiler_core::settings_catalog::ALL_SECTIONS
        .iter()
        .map(|section| section.id)
    {
        let label = find_section(section).expect("section").label;
        assert!(!label.is_empty());
    }
}

#[test]
fn group_titles_come_from_metadata_and_admission_is_unambiguous() {
    let mut seen = BTreeSet::new();
    for (key, _, _, _, _) in GROUPS {
        let title = group_title(*key);
        assert!(!title.is_empty(), "{key:?}");
        assert!(seen.insert(title.clone()), "duplicate group title {title}");
        // Production derivation agrees with the stored owning tab.
        assert!(group_tab(*key).is_some(), "{key:?}");
    }
    // The four admission groups share one section id but read
    // distinctly through their field labels.
    let admissions: Vec<String> = GROUPS
        .iter()
        .filter(|(key, _, _, _, _)| matches!(key, GroupKey::SectionField("tiling.admission", _)))
        .map(|(key, _, _, _, _)| group_title(*key))
        .collect();
    assert_eq!(admissions.len(), 4);
    assert_ne!(admissions[0], admissions[1]);
    for title in &admissions {
        assert!(title.starts_with("Admission: "), "{title}");
    }
    // Every native panel owns exactly one groupbox.
    for panel in NATIVE_PANELS {
        let groups = GROUPS
            .iter()
            .filter(|(key, _, _, _, _)| *key == GroupKey::Panel(panel.id))
            .count();
        assert_eq!(groups, 1, "{}", panel.id);
    }
}

#[test]
fn control_tabs_derive_from_sections_not_parallel_lists() {
    // Spot checks across portable, Windows-only, and panel-driven ids.
    assert_eq!(control_tab(ID_INNER_GAP), Some(TAB_TILING));
    assert_eq!(control_tab(ID_BORDER_WIDTH), Some(TAB_APPEARANCE));
    assert_eq!(control_tab(ID_UNDERLAY_EXTENSION), Some(TAB_APPEARANCE));
    assert_eq!(control_tab(ID_KEYBOARD_TAKEOVER), Some(TAB_SHORTCUTS));
    assert_eq!(control_tab(ID_BINDING_LIST), Some(TAB_SHORTCUTS));
    assert_eq!(control_tab(ID_GAMING_RECHECK), Some(TAB_SHORTCUTS));
    assert_eq!(control_tab(ID_MOUSE_SNAP), Some(TAB_DESKTOP));
    assert_eq!(control_tab(ID_TASKBAR_PAGER), Some(TAB_DESKTOP));
    assert_eq!(control_tab(ID_APPLY), None);
    assert_eq!(control_tab(ID_TABS), None);
    assert_eq!(control_tab(9999), None);
    // Every automated control with a field or panel resolves a tab.
    for id in ALL_CONTROL_IDS {
        if control_field(*id).is_some() || control_panel(*id).is_some() {
            assert!(control_tab(*id).is_some(), "{id}");
        }
    }
}

#[test]
fn tabs_cycle_both_ways() {
    assert_eq!(next_tab(0, true), 1);
    assert_eq!(next_tab(3, true), 0);
    assert_eq!(next_tab(0, false), 3);
    assert_eq!(next_tab(2, false), 1);
}

#[test]
fn creation_sequence_covers_everything_once_in_order() {
    let seq = creation_sequence();
    assert_eq!(seq.first(), Some(&CreateStep::TabStrip));
    let footer = [
        CreateStep::Control(ID_TAB_BADGE),
        CreateStep::Control(ID_STATUS),
        CreateStep::Control(ID_APPLY),
        CreateStep::Control(ID_REVERT),
        CreateStep::Control(ID_CLOSE),
    ];
    assert_eq!(&seq[seq.len() - footer.len()..], &footer);
    // Every automated control exactly once, every group and label once.
    let mut controls = BTreeSet::new();
    let mut groups = BTreeSet::new();
    let mut labels = BTreeSet::new();
    for step in &seq {
        match step {
            CreateStep::TabStrip => {}
            CreateStep::Control(id) => assert!(controls.insert(id), "duplicate {id}"),
            CreateStep::Group(index) => assert!(groups.insert(index), "duplicate group"),
            CreateStep::Label(index) => assert!(labels.insert(index), "duplicate label"),
        }
    }
    assert_eq!(controls.len() + 1, ALL_CONTROL_IDS.len());
    for id in ALL_CONTROL_IDS {
        if *id == ID_TABS {
            continue;
        }
        assert!(controls.contains(id), "missing {id}");
    }
    assert_eq!(groups.len(), GROUPS.len());
    assert_eq!(labels.len(), LABEL_RECTS.len());
    // Buddy labels immediately precede their editors (accessible names).
    let positions: std::collections::HashMap<CreateStep, usize> = seq
        .iter()
        .enumerate()
        .map(|(index, step)| (*step, index))
        .collect();
    for (editor, label) in BUDDY_LABELS {
        let editor_pos = positions[&CreateStep::Control(*editor)];
        let label_pos = positions[&CreateStep::Label(*label)];
        assert_eq!(label_pos + 1, editor_pos, "buddy label for {editor}");
    }
    // Tab-stop flow never jumps tabs: each tab's block is contiguous.
    let mut seen_tabs = BTreeSet::new();
    let mut last_tab: Option<usize> = None;
    for step in &seq {
        let tab = match step {
            CreateStep::TabStrip => continue,
            CreateStep::Group(index) => group_tab(GROUPS[*index].0),
            CreateStep::Label(index) => Some(LABEL_RECTS[*index].0),
            CreateStep::Control(id) => control_tab(*id),
        };
        if tab != last_tab {
            if let Some(tab) = tab {
                assert!(seen_tabs.insert(tab), "tab {tab} block split");
            }
            last_tab = tab;
        }
    }
}

#[test]
fn groups_cover_every_page_rect_exactly_once() {
    // Every page control and label center sits inside exactly one
    // groupbox of its own tab: nothing floats, nothing straddles.
    for id in ALL_CONTROL_IDS {
        let Some(tab) = control_tab(*id) else {
            continue;
        };
        let center = control_center(*id).expect("rect");
        let owners: Vec<_> = GROUPS
            .iter()
            .filter(|entry| group_tab(entry.0) == Some(tab) && group_contains(entry, center))
            .collect();
        assert_eq!(owners.len(), 1, "{id}");
    }
    for (index, rect) in LABEL_RECTS.iter().enumerate() {
        let center = label_center(index).expect("label rect");
        let owners: Vec<_> = GROUPS
            .iter()
            .filter(|entry| group_tab(entry.0) == Some(rect.0) && group_contains(entry, center))
            .collect();
        assert_eq!(owners.len(), 1, "label {index}");
    }
}

#[test]
fn rects_sit_on_their_own_tab_and_inside_the_client() {
    for id in ALL_CONTROL_IDS {
        let (x, y, w, h) = control_rect(*id).expect("every control has a rect");
        assert!(w > 0 && h > 0, "{id}");
        assert!(x >= 0 && y >= 0, "{id}");
        assert!(x + w <= CLIENT_W, "{id}");
        assert!(y + h <= CLIENT_H, "{id}");
        match control_tab(*id) {
            None => assert!(y >= FOOTER_TOP || *id == ID_TABS, "{id}"),
            Some(_) => {
                assert!(y >= TAB_DISPLAY_TOP, "{id}");
                assert!(y + h <= TAB_DISPLAY_BOTTOM, "{id}");
            }
        }
        assert!(control_class(*id).is_some(), "{id}");
    }
    for entry in GROUPS {
        let (_, x, y, w, h) = *entry;
        assert!(x + w <= CLIENT_W, "{entry:?}");
        assert!(y >= TAB_DISPLAY_TOP, "{entry:?}");
        assert!(y + h <= TAB_DISPLAY_BOTTOM, "{entry:?}");
    }
    for (index, rect) in LABEL_RECTS.iter().enumerate() {
        let (_, x, y, w, h) = *rect;
        assert!(x + w <= CLIENT_W, "label {index}");
        assert!(y >= TAB_DISPLAY_TOP, "label {index}");
        assert!(y + h <= TAB_DISPLAY_BOTTOM, "label {index}");
    }
}

#[test]
fn client_fits_1080p_at_100_and_150_percent() {
    // 1080p work area with a 40px taskbar: 1920x1040 physical.
    for dpi in [96u32, 144u32] {
        assert!(px(CLIENT_W, dpi) + FRAME_W_ALLOW <= 1920, "width at {dpi}");
        assert!(px(CLIENT_H, dpi) + FRAME_H_ALLOW <= 1040, "height at {dpi}");
        // Tallest page bottom (Shortcuts gaming row) plus the footer
        // must stay inside the client at both DPIs.
        assert!(px(TAB_DISPLAY_BOTTOM, dpi) <= px(CLIENT_H, dpi), "{dpi}");
    }
}

#[test]
fn origin_plus_outer_fits_1080p_at_100_and_150_percent() {
    // Size fit alone allows a clipped bottom edge when the top is
    // fixed: origin plus worst-case outer must fit the work area.
    for dpi in [96u32, 144u32] {
        assert!(origin_fits_1080p(dpi), "origin fit at {dpi}");
        assert!(client_fits_1080p(dpi), "size fit at {dpi}");
    }
}

#[test]
fn radio_starters_cover_every_field_exactly_once() {
    // One WS_GROUP starter per radio field; every other radio of the
    // field follows without one. Groupbox reordering can never merge
    // two fields while this holds.
    use std::collections::BTreeMap;
    let mut by_field: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for id in ALL_CONTROL_IDS {
        if let Some(field) = control_field(*id) {
            let def = windows_field_def(field).expect("descriptor");
            if matches!(
                def.field_type,
                FieldType::Enum if field != "workspace.default-tiled"
            ) || field == "workspace.default-tiled"
            {
                by_field.entry(field).or_default().push(*id);
            }
        }
    }
    assert_eq!(by_field.len(), 8);
    for (field, ids) in &by_field {
        let starters = ids.iter().filter(|id| is_radio_first(**id)).count();
        assert_eq!(starters, 1, "{field} needs one WS_GROUP starter");
        // The starter leads its field's radios in creation order.
        let seq = creation_sequence();
        let positions: Vec<usize> = ids
            .iter()
            .map(|id| {
                seq.iter()
                    .position(|step| *step == CreateStep::Control(*id))
                    .expect("radio created")
            })
            .collect();
        let starter_id = ids.iter().find(|id| is_radio_first(**id)).expect("starter");
        let starter_pos = seq
            .iter()
            .position(|step| *step == CreateStep::Control(*starter_id))
            .expect("starter created");
        assert_eq!(
            starter_pos,
            *positions.iter().min().expect("radios"),
            "{field}"
        );
    }
}

#[test]
fn single_line_texts_fit_their_rects() {
    // Every checkbox, radio, and push button fits on one line at the
    // message font; statics may wrap within their rect height;
    // scrollable list/edits are exempt (they scroll, never clip).
    for id in ALL_CONTROL_IDS {
        if control_scrollable(*id) {
            continue;
        }
        let (_, _, w, h) = control_rect(*id).expect("rect");
        let text = control_text(*id);
        if text.is_empty() {
            continue;
        }
        // Edits carry values, not labels; their buddy labels are
        // checked below. Window-text names on list/tab have no width.
        if control_class(*id) == Some("EDIT")
            || control_class(*id) == Some("LISTBOX")
            || *id == ID_TABS
        {
            continue;
        }
        let fits = if control_class(*id) == Some("STATIC") {
            static_fits(&text, w, h)
        } else {
            single_line_fits(&text, w)
        };
        assert!(fits, "{id} {text:?} in {w}x{h}px");
    }
    for (key, _, _, w, _) in GROUPS {
        assert!(single_line_fits(&group_title(*key), *w), "{key:?}");
    }
    for title in tab_titles() {
        assert!(single_line_fits(title, 200), "{title:?}");
    }
    for (index, rect) in LABEL_RECTS.iter().enumerate() {
        if label_class(index) == "EDIT" {
            continue;
        }
        let (_, _, _, w, h) = *rect;
        let text = label_text(index);
        assert!(static_fits(&text, w, h), "label {index} {text:?}");
    }
}

#[test]
fn note_edits_scroll_instead_of_clipping() {
    // The converted long notes plus the binding info all scroll.
    for id in [ID_PRESET_EXPLAIN, ID_REBIND_NOTE, ID_BINDING_INFO] {
        assert!(control_scrollable(id), "{id}");
        assert_eq!(control_class(id), Some("EDIT"), "{id}");
    }
    assert_eq!(label_class(2), "EDIT");
    assert_eq!(label_class(3), "EDIT");
    // ... while short labels stay plain statics.
    for index in [0, 1, 4, 5, 6, 7, 8, 12, 15, 16, 17] {
        assert_eq!(label_class(index), "STATIC", "{index}");
    }
}

#[test]
fn focus_moves_only_off_hidden_pages() {
    // Focused page controls move to the strip when their tab hides.
    assert!(needs_focus_move(ID_INNER_GAP, TAB_APPEARANCE));
    assert!(needs_focus_move(ID_BIND_KEEP, TAB_TILING));
    assert!(!needs_focus_move(ID_INNER_GAP, TAB_TILING));
    // Chrome never moves: strip, badge, status, buttons.
    for id in [
        ID_TABS,
        ID_TAB_BADGE,
        ID_STATUS,
        ID_APPLY,
        ID_REVERT,
        ID_CLOSE,
    ] {
        for tab in 0..TAB_COUNT {
            assert!(!needs_focus_move(id, tab), "{id}");
        }
    }
}

#[test]
fn badge_reports_invalid_unsaved_conflicts_and_warnings() {
    assert!(badge_text(true, false, 0, 0).contains("invalid"));
    assert!(badge_text(true, true, 3, 2).contains("invalid"));
    // Proven saved with nothing to report stays quiet and truthful.
    assert!(badge_proven_saved(false, false, false));
    assert_eq!(
        badge_text(false, false, 0, 0),
        "All tabs match saved settings."
    );
    // Any unproven channel (dirty HWND, staged work, pending chord)
    // reports requested-not-saved and never claims an effect.
    assert!(!badge_proven_saved(true, false, false));
    assert!(!badge_proven_saved(false, true, false));
    assert!(!badge_proven_saved(false, false, true));
    let unsaved = badge_text(false, true, 0, 0);
    assert!(unsaved.contains("Unsaved edits"));
    assert!(unsaved.contains("never confirms effect"));
    // Counts aggregate off-tab rows; singulars read correctly.
    let one = badge_text(false, false, 1, 0);
    assert!(one.contains("1 conflict") && !one.contains("conflicts"));
    let both = badge_text(false, true, 2, 1);
    assert!(both.contains("Unsaved edits"));
    assert!(both.contains("2 conflicts, 1 warning"));
    // Every wording fits the footer static so nothing clips silently.
    for text in [
        badge_text(true, false, 0, 0),
        badge_text(false, false, 0, 0),
        badge_text(false, true, 0, 0),
        badge_text(false, true, 103, 99),
        badge_text(false, false, 1, 1),
    ] {
        assert!(text.chars().count() <= BADGE_MAX_CHARS, "{text:?}");
    }
}

#[test]
fn zorder_plan_sends_groups_reversed_then_strip_last() {
    let plan = zorder_plan();
    assert_eq!(plan.len(), GROUPS.len() + 1);
    for (step, op) in plan.iter().enumerate() {
        if step < GROUPS.len() {
            assert_eq!(*op, ZOrderOp::GroupToBottom(GROUPS.len() - 1 - step));
        } else {
            assert_eq!(*op, ZOrderOp::StripToBottom);
        }
    }
    // Creation order equals GROUPS order: group tabs never decrease, so
    // the shell's k-th created groupbox is GROUPS[k] and plan indices
    // address it directly.
    let mut last = 0;
    for entry in GROUPS {
        let tab = group_tab(entry.0).expect("group tab");
        assert!(tab >= last, "{entry:?}");
        last = tab;
    }
}

/// Bottom-to-top sibling order with true `SetWindowPos` semantics:
/// `HWND_BOTTOM` sends to the very bottom (index zero, visually back), and an
/// `hWndInsertAfter` anchor precedes the moved window, i.e. the moved
/// window lands BELOW/BEHIND its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZElem {
    Strip,
    Group(usize),
    Page(usize),
}

fn z_to_bottom(order: &mut Vec<ZElem>, window: ZElem) {
    let pos = order
        .iter()
        .position(|elem| *elem == window)
        .expect("present");
    order.remove(pos);
    order.insert(0, window);
}

fn z_after_anchor(order: &mut Vec<ZElem>, window: ZElem, anchor: ZElem) {
    let pos = order
        .iter()
        .position(|elem| *elem == window)
        .expect("present");
    order.remove(pos);
    let anchor_pos = order
        .iter()
        .position(|elem| *elem == anchor)
        .expect("anchor present");
    order.insert(anchor_pos, window);
}

fn z_initial(bottom_to_top: bool) -> Vec<ZElem> {
    // Representative creation orders with page controls after their
    // groupboxes; both directions, since the plan must converge
    // regardless of which end new windows join.
    let mut order = vec![ZElem::Strip];
    for group in 0..GROUPS.len() {
        order.push(ZElem::Group(group));
        order.push(ZElem::Page(group));
    }
    if bottom_to_top {
        order
    } else {
        order.into_iter().rev().collect()
    }
}

#[test]
fn zorder_plan_converges_to_strip_groups_pages_from_either_start() {
    for bottom_to_top in [true, false] {
        let mut order = z_initial(bottom_to_top);
        let pages: Vec<ZElem> = order
            .iter()
            .copied()
            .filter(|elem| matches!(elem, ZElem::Page(_)))
            .collect();
        for op in zorder_plan() {
            match op {
                ZOrderOp::GroupToBottom(group) => z_to_bottom(&mut order, ZElem::Group(group)),
                ZOrderOp::StripToBottom => z_to_bottom(&mut order, ZElem::Strip),
            }
        }
        let mut want = vec![ZElem::Strip];
        want.extend((0..GROUPS.len()).map(ZElem::Group));
        want.extend(pages);
        assert_eq!(order, want, "from bottom_to_top={bottom_to_top}");
    }
}

#[test]
fn anchor_chaining_leaves_the_strip_above_the_frames() {
    // The superseded sequence (strip to bottom, then each groupbox
    // "after" the strip/predecessor) under true insert-after semantics:
    // every placed groupbox lands BELOW its anchor, so the strip ends
    // above the frames instead of below them.
    let mut order = z_initial(true);
    z_to_bottom(&mut order, ZElem::Strip);
    let mut anchor = ZElem::Strip;
    for group in 0..GROUPS.len() {
        z_after_anchor(&mut order, ZElem::Group(group), anchor);
        anchor = ZElem::Group(group);
    }
    let strip_pos = order
        .iter()
        .position(|elem| *elem == ZElem::Strip)
        .expect("strip");
    for group in 0..GROUPS.len() {
        let group_pos = order
            .iter()
            .position(|elem| *elem == ZElem::Group(group))
            .expect("group");
        assert!(group_pos < strip_pos, "group {group} ends below the strip");
    }
}
