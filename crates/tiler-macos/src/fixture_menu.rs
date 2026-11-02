#![forbid(unsafe_code)]

//! Development-only standalone Window-menu diagnostic for the signed live fixture.
//!
//! Exactly one native menu `AXPress` on one exact fixture window, with fresh
//! signed-fixture proof before the press and immediate before/after facts.
//! Fixed five-item vocabulary only: `zoom`, `fill`, `tile-left`,
//! `tile-right`, `return` (the standard Window-menu Zoom, Fill, Move and
//! Resize tile slots, and Return to Previous Size). No other menu item, no
//! chord vocabulary, no input synthesis.
//!
//! Proof shape (refuses before any press, first failure wins):
//! session gates (SLPS symbols, AX trust, Secure Input observed inactive -
//! unknown refuses, unlocked session), the serve announcement file (the only
//! nonce source), live bundle/executable/launch reads plus `codesign`
//! `--verify` and the designated-requirement read against the explicit
//! `--signer`, a nonce-gated read-only `get` over the announced Unix socket
//! (the announced socket path is used and shape-checked; the exchange proves
//! the nonce live), the CG owner join plus the sentinel-title caller/run-tag
//! join cross-checked against the live server snapshot, AND the selected
//! focus oracle: SLPS front pid plus workspace frontmost pid both equal the
//! fixture pid with the direct `AXFocusedWindow` selection (source exactly
//! `"focused"`, never the `AXMainWindow` fallback) equal to the exact wid
//! plus a fresh CG owner join (app menus target the key window, not the
//! selected AX element, so focus agreement is proven on the AX worker inside
//! the single press effect immediately before the press, not just at
//! pre-proof).
//!
//! Menu slots are located by fixed AppKit `AXIdentifier` selectors
//! (inspected from Loop at pin `573ace3`, never emitted): general slots
//! (`_performZoom:`, `_zoomFill:`, `_zoomUntile:`) directly under the Window
//! menu, halves (`_zoomLeft:`, `_zoomRight:`) one submenu deeper. No title
//! matching: titles are locale-fragile and the old title era predates the
//! current tiling slots. Inventory and reports carry metadata only
//! (IDs, counts, found/enabled bools, status names - never titles, content,
//! paths, nonces, executables, or requirement text).
//!
//! Fixture menu note (source-inspected, winit 0.30.13
//! `src/platform_impl/macos/menu.rs`): the default `menu::initialize` builds
//! the app menu only and never calls `setWindowsMenu`, so the fixture would
//! show no Window menu. The fixture therefore installs a minimal standard
//! Window menu at first window creation
//! (`tiler-live-fixture/src/menu_macos.rs`: one `Zoom` slot with
//! `performZoom:` on the responder chain, existing main menu preserved,
//! AppKit populating Fill/tile slots normally); no custom delegate, no custom
//! frame. Slot discovery follows Loop `573ace3`
//! (`SystemWindowManager.MoveAndResize::getItem`): fixed `AXIdentifier`
//! selectors, general slots Window-menu-direct, halves one submenu deeper.

use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use crate::fixture_post::SocketSnapshot;
use crate::fixture_post::{
    bundle_verdict, caller_id_valid, fixture_designated_tail, fixture_process_identity,
    parse_fixture_announcement, secure_input_verdict, socket_snapshot_for,
};

/// Bound for the single AX worker (join plus focus plus inventory plus
/// commit plus one press plus after-facts). Each AX read carries its own 2 s
/// messaging timeout; the worker bound caps the whole pass.
const AX_WORKER_BOUND: Duration = Duration::from_secs(15);
/// Hard lifetime for the whole diagnostic (proof plus worker plus report).
const HARD_LIMIT: Duration = Duration::from_secs(60);
/// Maximum transient title reads per inventory/press walk. Bounds the scan
/// of the Window submenu; the per-element messaging timeouts plus the worker
/// bound cap the cost the other way.
const MENU_SCAN_BUDGET: usize = 120;

/// CLI entry parameters for one standalone menu press: the serve
/// announcement file (socket/pid/nonce source), the selected fixture caller
/// id, the exact target window id, the fixed-vocabulary item key, and the
/// expected designated requirement (from `fixture-verify` output at
/// preflight; machine-specific, never hardcoded here).
pub struct FixtureMenuPressArgs {
    pub announcement_file: String,
    pub caller: String,
    pub wid: u32,
    pub item: String,
    pub signer: String,
}

/// Fixed menu vocabulary. The `key` is the CLI/report token; the match
/// string is the fixed AppKit `AXIdentifier` selector inspected from Loop
/// at pin `573ace3` (`SystemWindowManager.MoveAndResize`), never emitted
/// and never a title: titles are locale-fragile and the old title era was
/// wrong for current tiling slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Zoom,
    Fill,
    TileLeft,
    TileRight,
    Return,
}

impl MenuItem {
    pub fn key(self) -> &'static str {
        match self {
            MenuItem::Zoom => "zoom",
            MenuItem::Fill => "fill",
            MenuItem::TileLeft => "tile-left",
            MenuItem::TileRight => "tile-right",
            MenuItem::Return => "return",
        }
    }

    /// Fixed AppKit menu-item selector per key (Loop `573ace3`,
    /// `Loop/Window Management/SystemWindowManager.swift`, `MoveAndResize`
    /// raw values). General slots sit directly under the Window menu;
    /// halves sit one submenu deeper (the Move and Resize submenu).
    /// System slots carry these identifiers; the fixture's own hand-built
    /// `Zoom` slot carries none, so titles stay as fallback below.
    fn identifier(self) -> &'static str {
        match self {
            MenuItem::Zoom => "_performZoom:",
            MenuItem::Fill => "_zoomFill:",
            MenuItem::TileLeft => "_zoomLeft:",
            MenuItem::TileRight => "_zoomRight:",
            MenuItem::Return => "_zoomUntile:",
        }
    }

    /// Legacy title fallback per key: the fixture's own hand-built slots
    /// (created via `initWithTitle:`) carry no `AXIdentifier`, so the exact
    /// own-menu title is the only selector for them. Transient match
    /// strings only, never emitted. Kept narrow (proven own slots only);
    /// system slots are identifier-matched, never title-guessed.
    fn title_fallback(self) -> Option<&'static str> {
        match self {
            MenuItem::Zoom => Some("Zoom"),
            MenuItem::Fill => Some("Fill"),
            MenuItem::TileLeft | MenuItem::TileRight | MenuItem::Return => None,
        }
    }

    /// General (Window-menu-direct) slots per Loop's `generalActions`.
    fn is_general(self) -> bool {
        match self {
            MenuItem::Zoom | MenuItem::Fill | MenuItem::Return => true,
            MenuItem::TileLeft | MenuItem::TileRight => false,
        }
    }
}

/// Pure fixed-vocabulary parse: exactly the five keys, nothing else. No
/// title text, no chord/action vocabulary, no prefix guessing.
pub fn parse_menu_item(raw: &str) -> Result<MenuItem, String> {
    match raw {
        "zoom" => Ok(MenuItem::Zoom),
        "fill" => Ok(MenuItem::Fill),
        "tile-left" => Ok(MenuItem::TileLeft),
        "tile-right" => Ok(MenuItem::TileRight),
        "return" => Ok(MenuItem::Return),
        other => Err(format!(
            "fixture-menu-press: unknown --item '{other}' (want zoom|fill|tile-left|tile-right|return)"
        )),
    }
}

fn identifier_matches(item: MenuItem, identifier: &str) -> bool {
    identifier == item.identifier()
}

/// Combined slot match over one menu element: the fixed `AXIdentifier`
/// selector first (system slots), then the narrow legacy title fallback
/// (own hand-built slots without identifiers). Both reads are transient;
/// neither is retained or emitted.
#[cfg(target_os = "macos")]
fn slot_matches(item: MenuItem, element: &crate::ffi::AxElement) -> bool {
    if transient_identifier(element).is_some_and(|identifier| identifier_matches(item, &identifier))
    {
        return true;
    }
    match item.title_fallback() {
        Some(want) => transient_menu_title(element)
            .is_some_and(|title| title.trim().eq_ignore_ascii_case(want)),
        None => false,
    }
}

/// One menu-slot fact: metadata only. No title, no path, no content.
#[derive(Debug, Clone, Serialize)]
pub struct MenuItemFact {
    pub key: String,
    pub found: bool,
    pub enabled: Option<bool>,
    pub enabled_status: String,
}

fn missing_item(key: &str, status: &str) -> MenuItemFact {
    MenuItemFact {
        key: key.to_owned(),
        found: false,
        enabled: None,
        enabled_status: status.to_owned(),
    }
}

/// Read-only menu inventory: metadata only (counts, found/enabled bools,
/// status names). Carries no title, content, path, nonce, socket, or signer.
#[derive(Debug, Clone, Serialize)]
pub struct MenuInventory {
    pub menubar_found: bool,
    pub menubar_status: String,
    pub top_level_count: Option<usize>,
    pub window_menu_found: bool,
    pub window_menu_status: String,
    pub window_item_count: Option<usize>,
    pub items: Vec<MenuItemFact>,
}

fn failed_inventory(status: &str) -> MenuInventory {
    MenuInventory {
        menubar_found: false,
        menubar_status: status.to_owned(),
        top_level_count: None,
        window_menu_found: false,
        window_menu_status: "not-read".to_owned(),
        window_item_count: None,
        items: Vec::new(),
    }
}

/// Pure selected-focus agreement over plain data: the SLPS front pid and the
/// workspace frontmost pid must both equal the fixture pid, the direct
/// `AXFocusedWindow` selection must equal the exact wid with source exactly
/// `"focused"`, and the fresh CG owner join must hold. `AXMainWindow`-only
/// (`"main"`) or absent (`"none"`) selections refuse even when the wid
/// matches: app menus target the key window, so only the focused leg proves
/// the press target. Anything else refuses with the leg named. Wraps
/// [`crate::ffi::selected_focus_agrees`] with refusal detail plus the source
/// gate (`selected_focus_agrees` itself never sees the source).
pub fn check_menu_focus(
    want_pid: u32,
    want_wid: u32,
    slps_pid: Option<i32>,
    ws_pid: Option<i32>,
    selected_wid: Option<u32>,
    selected_source: &str,
    cg_owned: bool,
) -> Result<(), String> {
    if want_pid <= 1 {
        return Err(format!("refusing fixture menu: reserved pid {want_pid}"));
    }
    if want_wid == 0 {
        return Err("refusing fixture menu to window id 0".to_owned());
    }
    if slps_pid.is_none() || ws_pid.is_none() {
        return Err(
            "refusing fixture menu: frontmost pids unreadable (no SLPS/workspace agreement)"
                .to_owned(),
        );
    }
    if selected_wid.is_none() {
        return Err(
            "refusing fixture menu: direct focused/main window missing (no key-window agreement)"
                .to_owned(),
        );
    }
    if selected_source != "focused" {
        return Err(format!(
            "refusing fixture menu: direct selection source '{selected_source}' is not the focused leg (main/none never proves the key window even when wid matches)"
        ));
    }
    if !cg_owned {
        return Err(format!(
            "refusing fixture menu: wid {want_wid} not owned by pid {want_pid} in fresh CG join"
        ));
    }
    if !crate::ffi::selected_focus_agrees(
        want_pid,
        want_wid,
        slps_pid,
        ws_pid,
        selected_wid,
        cg_owned,
    ) {
        return Err(format!(
            "refusing fixture menu: focus agreement failed (SLPS {slps_pid:?} workspace {ws_pid:?} selected {selected_wid:?} want pid {want_pid} wid {want_wid}); app menu targets the key window, refusing without agreement"
        ));
    }
    Ok(())
}

/// Pure item pressability over the before-inventory: the requested slot must
/// be joined AND observed enabled. A missing, disabled, or
/// unreadable-enabled slot refuses before any focus proof or press. Pure;
/// unit-tested.
pub fn menu_item_pressable(inventory: &MenuInventory, key: &str) -> Result<(), String> {
    let Some(fact) = inventory.items.iter().find(|f| f.key == key) else {
        return Err(format!(
            "refusing fixture menu: slot '{key}' not inventoried (window menu absent or unreadable)"
        ));
    };
    if !fact.found {
        return Err(format!(
            "refusing fixture menu: slot '{key}' join missed ({})",
            fact.enabled_status
        ));
    }
    match fact.enabled {
        Some(true) => Ok(()),
        Some(false) => Err(format!(
            "refusing fixture menu: slot '{key}' observed disabled; refusing"
        )),
        None => Err(format!(
            "refusing fixture menu: slot '{key}' enabled unreadable ({}); refusing",
            fact.enabled_status
        )),
    }
}

/// Pure commit-to-press decision in press order: item pressable, then focus
/// agreement, then the commit re-proof verdict, then the latched-interrupt
/// check, then the main-bound cancellation fence. Any Err must prevent the
/// single press; only Ok authorizes it. Pure; unit-tested.
pub fn authorize_menu_press(
    inventory: &MenuInventory,
    key: &str,
    focus: &Result<(), String>,
    commit: &Result<(), String>,
    interrupted: bool,
    cancelled: bool,
) -> Result<(), String> {
    menu_item_pressable(inventory, key)?;
    if let Err(error) = focus {
        return Err(format!("menu worker focus: {error}"));
    }
    if let Err(error) = commit {
        return Err(format!("menu worker commit: {error}"));
    }
    if interrupted {
        return Err("menu worker: SIGINT/SIGTERM latched; refusing press".to_owned());
    }
    if cancelled {
        return Err(
            "menu worker: main bound lapsed before the press; refusing (press state stays provable only via facts)"
                .to_owned(),
        );
    }
    Ok(())
}

/// Drive exactly one press effect: the pure decision first, then a single
/// effect call with no retry. Uncertain outcomes propagate as Err and the
/// caller must not replay. Unit-tested with a fake effect counter standing
/// in for the single AXPress call site.
pub fn drive_single_menu_press(
    inventory: &MenuInventory,
    key: &str,
    focus: Result<(), String>,
    commit: Result<(), String>,
    interrupted: bool,
    cancelled: bool,
    effect: impl FnOnce() -> Result<u64, String>,
) -> Result<u64, String> {
    authorize_menu_press(inventory, key, &focus, &commit, interrupted, cancelled)?;
    effect()
}

/// Final pre-press fence recheck for inside the single effect, after any
/// blocking pre-effect reads (enabled reread, focus proof): the
/// `interrupted`/`cancelled` snapshot judged by the driver was captured
/// before those reads, so a latch or fence that lands during them must
/// refuse here, immediately before `AXPress`. Pure; unit-tested.
pub fn refuse_if_late_cancelled(
    fresh_interrupted: bool,
    fresh_cancelled: bool,
) -> Result<(), String> {
    if fresh_interrupted {
        return Err(
            "menu worker: SIGINT/SIGTERM latched immediately before press; refusing".to_owned(),
        );
    }
    if fresh_cancelled {
        return Err(
            "menu worker: main bound lapsed immediately before press; refusing (press state stays provable only via facts)"
                .to_owned(),
        );
    }
    Ok(())
}

/// Shared live session gates before proof and inside the commit re-proof (no
/// presses): SLPS symbols, AX trust, Secure Input (strict: unknown refuses),
/// unlocked session. No CG post-access gate: this route performs no CG post.
#[cfg(target_os = "macos")]
fn menu_session_gates() -> Result<(), String> {
    if !crate::ffi::slps_available() {
        return Err("slps-unavailable: SLPS focus symbols unavailable; stop".to_owned());
    }
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err("ax-trust: AX trust not observed true; stop".to_owned());
    }
    secure_input_verdict(crate::ffi::secure_input_known())?;
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => Ok(()),
        crate::ffi::SessionLock::Locked => {
            Err("session: session locked; refusing menu press".to_owned())
        }
        crate::ffi::SessionLock::Unknown => {
            Err("session: session unknown; refusing menu press".to_owned())
        }
    }
}

/// Join the exact `(pid, wid)` AX window on the calling thread: application
/// element, 2 s messaging timeout, scan with a per-window 2 s timeout set
/// BEFORE the id read (an unsettable window is skipped, never read
/// unbounded), then a 2 s timeout on the retained target (refuses when
/// unsettable: later reads on it must stay bounded).
#[cfg(target_os = "macos")]
fn join_ax_window(pid: u32, wid: u32) -> Result<crate::ffi::AxElement, String> {
    use crate::ffi::{AxElement, ax_error_name, ax_set_messaging_timeout, ax_window_id};
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("menu join: pid {pid} out of range"))?;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| format!("menu join: no AX element for pid {pid}"))?;
    if ax_set_messaging_timeout(&app, 2.0) != 0 {
        return Err("menu join: messaging timeout unset before window join".to_owned());
    }
    let windows = app.windows().map_err(|code| {
        format!(
            "menu join: AX window list unreadable ({})",
            ax_error_name(code)
        )
    })?;
    let mut target: Option<AxElement> = None;
    for window in windows {
        if ax_set_messaging_timeout(&window, 2.0) != 0 {
            continue;
        }
        if ax_window_id(&window) == Some(wid) {
            target = Some(window);
            break;
        }
    }
    let Some(window) = target else {
        return Err(format!("menu join: window wid {wid} rejoin missed"));
    };
    if ax_set_messaging_timeout(&window, 2.0) != 0 {
        return Err("menu join: messaging timeout unset on retained target; refusing".to_owned());
    }
    Ok(window)
}

/// One fresh title-leg CG entry for `wid`: `(owner pid, title)`. `None`
/// entry (absent wid) is returned for the pure check to judge, never
/// guessed.
#[cfg(target_os = "macos")]
fn cg_entry_for(wid: u32) -> Result<Option<(u32, Option<String>)>, String> {
    crate::ffi::cg_window_list_all()
        .map(|list| {
            list.into_iter()
                .find(|window| window.wid == wid)
                .map(|window| (window.pid, window.title))
        })
        .map_err(|e| format!("CG snapshot refused ({e})"))
}

/// Transient identifier read: used only to locate the Window menu and the
/// target slot during a walk, never stored or emitted. Empty reads as
/// missing (`ax_identifier` already normalizes those to `None`); hard read
/// failures skip the element.
#[cfg(target_os = "macos")]
fn transient_identifier(element: &crate::ffi::AxElement) -> Option<String> {
    let _ = crate::ffi::ax_set_messaging_timeout(element, 2.0);
    crate::ffi::ax_identifier(element).ok().flatten()
}

#[cfg(target_os = "macos")]
fn transient_menu_title(element: &crate::ffi::AxElement) -> Option<String> {
    let _ = crate::ffi::ax_set_messaging_timeout(element, 2.0);
    crate::ffi::ax_string_attribute(element, "AXTitle")
        .ok()
        .flatten()
        .filter(|title| !title.trim().is_empty())
}

/// Transient enabled read for one menu item handle.
#[cfg(target_os = "macos")]
fn transient_enabled(element: &crate::ffi::AxElement) -> (Option<bool>, String) {
    use crate::ffi::{ax_bool_attribute, ax_error_name};
    let _ = crate::ffi::ax_set_messaging_timeout(element, 2.0);
    match ax_bool_attribute(element, "AXEnabled") {
        Ok(value) => (Some(value), "ok".to_owned()),
        Err(code) => (None, ax_error_name(code)),
    }
}

/// Loop-shaped menu scan from the Window `AXMenuBarItem`: general slots
/// (`_performZoom:`, `_zoomFill:`, `_zoomUntile:`) sit directly under the
/// Window `AXMenu`; halves (`_zoomLeft:`, `_zoomRight:`) sit one submenu
/// deeper (the Move and Resize submenu), matching Loop `573ace3`
/// `MoveAndResize::getItem`. Identifiers are compared transiently and never
/// retained; only found/enabled metadata lands in `hits`. Bounded by the
/// shared `budget`; failed reads skip the element.
#[cfg(target_os = "macos")]
fn scan_window_item(
    window_item: &crate::ffi::AxElement,
    budget: &mut usize,
    hits: &mut Vec<(MenuItem, Option<bool>, String)>,
) {
    use crate::ffi::ax_children;
    const GENERAL: [MenuItem; 3] = [MenuItem::Zoom, MenuItem::Fill, MenuItem::Return];
    const HALVES: [MenuItem; 2] = [MenuItem::TileLeft, MenuItem::TileRight];
    let mut record = |key: MenuItem, element: &crate::ffi::AxElement| {
        if !hits.iter().any(|(hit, _, _)| *hit == key) {
            let (enabled, status) = transient_enabled(element);
            hits.push((key, enabled, status));
        }
    };
    let Ok(menus) = ax_children(window_item) else {
        return;
    };
    for menu in menus {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(items) = ax_children(&menu) else {
            continue;
        };
        for item in items {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            for key in GENERAL {
                if slot_matches(key, &item) {
                    record(key, &item);
                }
            }
            // One nested level only (Loop's submenu leg): the Move and
            // Resize submenu's own items. No blind recursion.
            let Ok(submenus) = ax_children(&item) else {
                continue;
            };
            for submenu in submenus {
                if *budget == 0 {
                    return;
                }
                *budget -= 1;
                let Ok(targets) = ax_children(&submenu) else {
                    continue;
                };
                for target in targets {
                    if *budget == 0 {
                        return;
                    }
                    *budget -= 1;
                    for key in HALVES {
                        if slot_matches(key, &target) {
                            record(key, &target);
                        }
                    }
                }
            }
        }
    }
}

/// Locate the Window menu under the app menu bar by transient title,
/// scanning its subtree for the fixed vocabulary. Returns the inventory
/// plus, on success, nothing else (handles are re-walked for the press so
/// no element crosses a proof boundary).
#[cfg(target_os = "macos")]
fn read_menu_inventory(pid: u32, wid: u32) -> MenuInventory {
    use crate::ffi::{
        AxElement, ax_children, ax_element_attribute, ax_error_name, ax_set_messaging_timeout,
    };
    if pid <= 1 || i32::try_from(pid).is_err() {
        return failed_inventory(&format!("refusing out-of-range pid {pid}"));
    }
    if wid == 0 {
        return failed_inventory("refusing window id 0");
    }
    // Join the exact window first so a stale wid fails before any menu read.
    if let Err(error) = join_ax_window(pid, wid) {
        return failed_inventory(&error);
    }
    let pid_i32 = pid as i32;
    let Some(app) = AxElement::application(pid_i32) else {
        return failed_inventory(&format!("no AX element for pid {pid}"));
    };
    if ax_set_messaging_timeout(&app, 2.0) != 0 {
        return failed_inventory("messaging timeout unset before menubar join");
    }
    let menubar = match ax_element_attribute(&app, "AXMenuBar") {
        Ok(bar) => bar,
        Err(status) => return failed_inventory(&format!("menubar join missed ({status})")),
    };
    if ax_set_messaging_timeout(&menubar, 2.0) != 0 {
        return failed_inventory("messaging timeout unset on retained menubar; refusing");
    }
    let top_level = match ax_children(&menubar) {
        Ok(children) => children,
        Err(code) => {
            return failed_inventory(&format!(
                "menubar children unreadable ({})",
                ax_error_name(code)
            ));
        }
    };
    let top_level_count = top_level.len();
    // Index-based Window-menu fetch: the scan only proves presence; the
    // handle is joined here from the retained top-level list. The menubar
    // item itself is located by its fixed "Window" label (own-fixture
    // menubar only, transient, never emitted); slots below it match by
    // `AXIdentifier`, never by title.
    let mut window_index: Option<usize> = None;
    for (index, item) in top_level.iter().enumerate() {
        let _ = crate::ffi::ax_set_messaging_timeout(item, 2.0);
        let is_window = crate::ffi::ax_string_attribute(item, "AXTitle")
            .ok()
            .flatten()
            .is_some_and(|title| title.trim().eq_ignore_ascii_case("window"));
        if is_window {
            window_index = Some(index);
            break;
        }
    }
    let Some(window_index) = window_index else {
        return MenuInventory {
            menubar_found: true,
            menubar_status: "ok".to_owned(),
            top_level_count: Some(top_level_count),
            window_menu_found: false,
            window_menu_status: "not-found (no Window menu under the app menu bar)".to_owned(),
            window_item_count: None,
            items: vec![],
        };
    };
    let window_item = &top_level[window_index];
    // Presence scan plus per-slot enabled state, Loop-shaped and
    // budget-bounded. Identifiers are compared transiently inside and
    // never retained.
    let mut budget = MENU_SCAN_BUDGET;
    let mut hits: Vec<(MenuItem, Option<bool>, String)> = Vec::new();
    scan_window_item(window_item, &mut budget, &mut hits);
    let mut items = Vec::new();
    for key in [
        MenuItem::Zoom,
        MenuItem::Fill,
        MenuItem::TileLeft,
        MenuItem::TileRight,
        MenuItem::Return,
    ] {
        match hits.iter().find(|(hit, _, _)| *hit == key) {
            Some((_, enabled, status)) => {
                items.push(MenuItemFact {
                    key: key.key().to_owned(),
                    found: true,
                    enabled: *enabled,
                    enabled_status: status.clone(),
                });
            }
            None => items.push(missing_item(key.key(), "not-found")),
        }
    }
    // Count direct children of the Window item for the metadata record.
    let window_item_count = ax_children(window_item).ok().map(|c| c.len());
    MenuInventory {
        menubar_found: true,
        menubar_status: "ok".to_owned(),
        top_level_count: Some(top_level_count),
        window_menu_found: true,
        window_menu_status: "ok".to_owned(),
        window_item_count,
        items,
    }
}

/// Re-walk to the requested slot and return its live handle for the single
/// press. Same Loop-shaped identifier walk as the inventory (general slots
/// directly under the Window menu, halves one submenu deeper); fails closed
/// when the Window menu or the slot is absent.
#[cfg(target_os = "macos")]
fn join_menu_slot(pid: u32, key: MenuItem) -> Result<crate::ffi::AxElement, String> {
    use crate::ffi::{
        AxElement, ax_children, ax_element_attribute, ax_error_name, ax_set_messaging_timeout,
    };
    let pid_i32 =
        i32::try_from(pid).map_err(|_| format!("menu slot join: pid {pid} out of range"))?;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| format!("menu slot join: no AX element for pid {pid}"))?;
    if ax_set_messaging_timeout(&app, 2.0) != 0 {
        return Err("menu slot join: messaging timeout unset".to_owned());
    }
    let menubar = ax_element_attribute(&app, "AXMenuBar")
        .map_err(|status| format!("menu slot join: menubar join missed ({status})"))?;
    let top_level = ax_children(&menubar).map_err(|code| {
        format!(
            "menu slot join: menubar children unreadable ({})",
            ax_error_name(code)
        )
    })?;
    let mut window_item: Option<AxElement> = None;
    for item in top_level {
        let _ = ax_set_messaging_timeout(&item, 2.0);
        let is_window = crate::ffi::ax_string_attribute(&item, "AXTitle")
            .ok()
            .flatten()
            .is_some_and(|title| title.trim().eq_ignore_ascii_case("window"));
        if is_window {
            window_item = Some(item);
            break;
        }
    }
    let window_item =
        window_item.ok_or_else(|| "menu slot join: Window menu absent; refusing".to_owned())?;
    let mut budget = MENU_SCAN_BUDGET;
    let mut join_if_wanted = |element: AxElement| -> Result<Option<AxElement>, String> {
        if budget == 0 {
            return Err(format!(
                "menu slot join: slot '{}' not found under Window menu; refusing",
                key.key()
            ));
        }
        budget -= 1;
        if slot_matches(key, &element) {
            if ax_set_messaging_timeout(&element, 2.0) != 0 {
                return Err(
                    "menu slot join: messaging timeout unset on retained slot; refusing".to_owned(),
                );
            }
            Ok(Some(element))
        } else {
            Ok(None)
        }
    };
    let menus = ax_children(&window_item).map_err(|code| {
        format!(
            "menu slot join: submenu unreadable ({})",
            ax_error_name(code)
        )
    })?;
    for menu in menus {
        let items = ax_children(&menu).map_err(|code| {
            format!(
                "menu slot join: submenu unreadable ({})",
                ax_error_name(code)
            )
        })?;
        for item in items {
            // Loop shape: general slots match Window-menu-direct; halves
            // match one submenu deeper. No cross-level matching.
            if key.is_general() {
                if let Some(joined) = join_if_wanted(item)? {
                    return Ok(joined);
                }
                continue;
            }
            // Halves live one submenu deeper; general slots never match
            // there, so the nested walk runs for every key without widening
            // the vocabulary.
            let submenus = ax_children(&item).map_err(|code| {
                format!(
                    "menu slot join: submenu unreadable ({})",
                    ax_error_name(code)
                )
            })?;
            for submenu in submenus {
                let targets = ax_children(&submenu).map_err(|code| {
                    format!(
                        "menu slot join: submenu unreadable ({})",
                        ax_error_name(code)
                    )
                })?;
                for target in targets {
                    if let Some(joined) = join_if_wanted(target)? {
                        return Ok(joined);
                    }
                }
            }
        }
    }
    Err(format!(
        "menu slot join: slot '{}' not found under Window menu; refusing",
        key.key()
    ))
}

/// Fresh selected-focus proof on the calling (AX worker) thread: SLPS front
/// pid plus workspace frontmost pid plus the direct focused selection
/// (exactly the `AXFocusedWindow` leg, source `"focused"`) on the fixture pid
/// plus a fresh CG owner join, judged through the pure choke point. No
/// titles, no bundles in the proof record. The `AXMainWindow` fallback leg
/// (`"main"`) never suffices here: menus target the key window.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Serialize)]
struct FocusProof {
    slps_pid: i32,
    ws_pid: i32,
    selected_wid: Option<u32>,
    selected_source: String,
    cg_owned: bool,
}

#[cfg(target_os = "macos")]
fn live_focus_proof(pid: u32, wid: u32) -> Result<FocusProof, String> {
    let want_i32 = i32::try_from(pid).map_err(|_| format!("menu focus: pid {pid} out of range"))?;
    let slps = crate::ffi::slps_front_pid()
        .map_err(|e| format!("menu focus: SLPS front pid unreadable ({e})"))?;
    let (ws, _) = crate::ffi::frontmost_app()
        .ok_or_else(|| "menu focus: workspace frontmost unreadable".to_owned())?;
    let read = crate::ffi::ax_direct_focused_read(want_i32);
    let cg_owned = crate::ffi::cg_window_list_basic()
        .map(|list| list.iter().any(|w| w.wid == wid && w.pid == pid))
        .map_err(|e| format!("menu focus: CG owner join unreadable ({e})"))?;
    check_menu_focus(
        pid,
        wid,
        Some(slps),
        Some(ws),
        read.selected_wid,
        &read.selected_source,
        cg_owned,
    )?;
    Ok(FocusProof {
        slps_pid: slps,
        ws_pid: ws,
        selected_wid: read.selected_wid,
        selected_source: read.selected_source,
        cg_owned,
    })
}

#[derive(Debug, Clone, Serialize)]
struct FixtureMenuReport {
    outcome: String,
    error: Option<String>,
    pid: u32,
    wid: u32,
    item: String,
    caller: String,
    secure_input: Option<bool>,
    ax_trusted: Option<bool>,
    proof_us: u64,
    /// Worker-relative press cost; `None` on the unknown path, where no
    /// honest single-figure cost exists (the dispatch/return stamps below
    /// carry the timing instead).
    press_latency_us: Option<u64>,
    /// Shared host monotonic stamps (`crate::ffi::host_now_ns`, `None`
    /// when the timebase is unreadable): the `AXPress` dispatch edge and
    /// the return after it. These join with the concurrent sampler's
    /// stamps on one origin; the `press_latency_us` worker-relative cost
    /// above is unchanged.
    dispatch_ns: Option<u64>,
    return_ns: Option<u64>,
    cg_owner_before: Option<u32>,
    cg_owner_after: Option<u32>,
    run_tag_agreement: bool,
    focus_before: Option<FocusProofLite>,
    focus_after: Option<FocusProofLite>,
    menu_before: MenuInventory,
    menu_after: MenuInventory,
    synthetic: bool,
    note: String,
}

/// Focus proof as emitted: IDs and status names only, no bundles.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct FocusProofLite {
    slps_pid: i32,
    ws_pid: i32,
    selected_wid: Option<u32>,
    selected_source: String,
    agreed: bool,
}

#[cfg(target_os = "macos")]
impl From<FocusProof> for FocusProofLite {
    fn from(proof: FocusProof) -> Self {
        FocusProofLite {
            slps_pid: proof.slps_pid,
            ws_pid: proof.ws_pid,
            selected_wid: proof.selected_wid,
            selected_source: proof.selected_source,
            agreed: true,
        }
    }
}

/// Standalone menu diagnostic: fresh proof, then exactly one native menu
/// `AXPress` on an AX worker with a commit-boundary re-proof, then
/// before/after facts. Exit codes: 2 usage/validation, 1 gate/proof/press
/// failure with evidence.
#[cfg(target_os = "macos")]
pub fn run_fixture_menu_press(args: &FixtureMenuPressArgs) -> Result<String, (i32, String)> {
    use crate::fixture_zoom::{ZoomAuthInputs, check_zoom_authorization};
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    let refuse = |stage: &str, error: String| -> (i32, String) {
        eprintln!("fixture-menu-press refused at {stage}: {error}");
        err_json(stage, error)
    };
    if args.announcement_file.is_empty() {
        return Err((
            2,
            "fixture-menu-press needs a nonempty --announcement-file".to_owned(),
        ));
    }
    if args.signer.trim().is_empty() {
        return Err((2, "fixture-menu-press needs a nonempty --signer".to_owned()));
    }
    let item = parse_menu_item(&args.item).map_err(|e| (2, e))?;
    if !caller_id_valid(&args.caller) {
        return Err((2, "refusing fixture menu: caller id malformed".to_owned()));
    }
    if args.wid == 0 {
        return Err((2, "refusing fixture menu to window id 0".to_owned()));
    }
    let mut _watchdog = crate::watchdog::Watchdog::arm(HARD_LIMIT).map_err(|e| (1, e))?;
    menu_session_gates().map_err(|e| refuse("session", e))?;
    let _interrupt =
        crate::space_drag_diag_live::InterruptArmed::arm().map_err(|e| refuse("interrupt", e))?;
    let proof_start = Instant::now();
    let raw = std::fs::read_to_string(&args.announcement_file).map_err(|e| {
        (
            2,
            format!("fixture-menu-press announcement file unreadable ({e})"),
        )
    })?;
    let announced = parse_fixture_announcement(raw.trim())
        .map_err(|e| (2, format!("announcement refused: {e}")))?;
    let pid = announced.pid;
    if pid <= 1 {
        return Err((2, format!("refusing fixture menu to reserved pid {pid}")));
    }
    if pid == std::process::id() {
        return Err((
            2,
            "refusing fixture menu to the controller itself".to_owned(),
        ));
    }
    if i32::try_from(pid).is_err() {
        return Err((2, format!("refusing fixture menu: pid {pid} out of range")));
    }
    let identity = fixture_process_identity(pid).map_err(|e| refuse("identity", e))?;
    bundle_verdict(identity.bundle.as_deref())
        .map_err(|e| refuse("identity", format!("identity: {e}")))?;
    let exe = identity.exe.filter(|exe| !exe.is_empty()).ok_or_else(|| {
        refuse(
            "identity",
            "identity: executable path unreadable; refusing".to_owned(),
        )
    })?;
    let launch = identity
        .launch
        .filter(|launch| !launch.is_empty())
        .ok_or_else(|| {
            refuse(
                "identity",
                "identity: launch date unreadable (no LaunchServices lifetime); refusing"
                    .to_owned(),
            )
        })?;
    let tail = fixture_designated_tail(&exe).map_err(|e| refuse("signature", e))?;
    if tail != args.signer.trim() {
        return Err(refuse(
            "signature",
            "signature: designated requirement mismatch (not the expected fixture signer); refusing"
                .to_owned(),
        ));
    }
    let snapshot =
        socket_snapshot_for(&announced.socket, &announced.nonce, &args.caller).map_err(|e| {
            refuse(
                "socket",
                format!("socket: {e} (nonce proves the live run; stale or foreign runs refuse)"),
            )
        })?;
    if snapshot.pid != pid {
        return Err(refuse(
            "socket",
            format!(
                "refusing fixture menu: live server pid {} != announced pid {pid}; refusing",
                snapshot.pid
            ),
        ));
    }
    let cg_entry = cg_entry_for(args.wid).map_err(|e| refuse("proof", e))?;
    let cg_owner_before = cg_entry.as_ref().map(|(owner, _)| *owner);
    // When CG withholds the title (this reader lacks Screen Recording) but
    // the owner join already names the fixture pid, take one bounded AXTitle
    // read off the exact joined fixture window as the title source. A
    // present CG title stays authoritative and an absent CG entry still
    // refuses: the fallback fills a withheld title, never an absent window,
    // and never a foreign pid's window.
    let ax_title: Option<String> = match &cg_entry {
        Some((owner, None)) if *owner == pid => join_ax_window(pid, args.wid)
            .ok()
            .and_then(|window| {
                crate::ffi::ax_string_attribute(&window, "AXTitle")
                    .ok()
                    .flatten()
            })
            .filter(|title| !title.is_empty()),
        _ => None,
    };
    let bound = ZoomAuthInputs {
        caller: args.caller.clone(),
        wid: args.wid,
        announced_pid: pid,
        snapshot: snapshot.clone(),
        bundle: identity.bundle.clone(),
        sig_tail: Some(tail.clone()),
        expected_signer: args.signer.clone(),
        live_exe: Some(exe.clone()),
        bound_exe: exe.clone(),
        live_launch: Some(launch.clone()),
        bound_launch: launch.clone(),
        cg_entry,
        ax_title,
        secure_input: crate::ffi::secure_input_known(),
    };
    check_zoom_authorization(&bound).map_err(|e| {
        refuse(
            "proof",
            e.replace("refusing fixture zoom", "refusing fixture menu"),
        )
    })?;
    if crate::space_drag_diag_live::diag_interrupted() {
        return Err(refuse(
            "interrupted",
            "interrupted: SIGINT/SIGTERM latched; refusing press".to_owned(),
        ));
    }
    let proof_us = proof_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    // The single AX worker owns every `AxElement` (construct, use, drop);
    // the main thread only waits bounded. The worker repeats the cheap proof
    // through the same pure choke point at commit (fresh identity plus fresh
    // CG, no socket, no subprocess). Inside the effect, exact focused-window
    // agreement and fresh session/cancellation gates precede the single press.
    let worker_caller = args.caller.clone();
    let worker_wid = args.wid;
    let worker_signer = args.signer.clone();
    // Best-effort cancellation fence: set when the main bound lapses so a
    // still-running worker refuses instead of pressing late. It narrows but
    // cannot close the post-timeout press window of a synchronous AXPress
    // (flag set after the worker's final check still races), so a timeout
    // return always means press state UNKNOWN, never no-press, and never a
    // replay without inspecting the before/after facts.
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<WorkerOutcome, MenuPressFailure>>();
    std::thread::Builder::new()
        .name(format!("fixture-menu-press-{pid}"))
        .spawn(move || {
            let outcome = menu_press_on_worker(
                pid,
                &worker_caller,
                worker_wid,
                item,
                &worker_signer,
                &snapshot,
                &exe,
                &launch,
                &worker_cancelled,
            );
            let _ = done_tx.send(outcome);
        })
        .map_err(|e| refuse("ax-worker", format!("AX worker spawn refused ({e})")))?;
    // Bounded wait: a hung worker costs this bound only. A timeout or a dead
    // worker sets the fence and reports press state unknown (a late press
    // remains possible past the timeout); the invocation never replays.
    let outcome = match done_rx.recv_timeout(AX_WORKER_BOUND) {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(MenuPressFailure::Refused(error))) => return Err(refuse("press", error)),
        Ok(Err(MenuPressFailure::Unknown { evidence, error })) => {
            eprintln!("fixture-menu-press unknown after press: {error}");
            let report = FixtureMenuReport {
                outcome: "unknown".to_owned(),
                error: Some(error),
                pid,
                wid: args.wid,
                item: item.key().to_owned(),
                caller: args.caller.clone(),
                secure_input: crate::ffi::secure_input_known(),
                ax_trusted: crate::ffi::ax_is_process_trusted(),
                proof_us,
                press_latency_us: None,
                dispatch_ns: evidence.dispatch_ns,
                return_ns: evidence.return_ns,
                cg_owner_before,
                cg_owner_after: evidence.cg_owner_after,
                run_tag_agreement: true,
                focus_before: Some(evidence.focus_before),
                focus_after: evidence.focus_after,
                menu_before: evidence.before,
                menu_after: evidence.after,
                synthetic: false,
                note: "development-only standalone menu diagnostic (press state UNKNOWN after dispatch: exactly one AXPress attempted with no retry, no replay, no restore; dispatch/return stamps plus before/after facts retained as evidence)"
                    .to_owned(),
            };
            let json = serde_json::to_string(&report).map_err(|e| (1, e.to_string()))?;
            _watchdog.disarm();
            return Err((1, json));
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            cancelled.store(true, Ordering::SeqCst);
            return Err(refuse(
                "ax-worker",
                "AX worker timed out (bounded 15s); press state unknown (a late press remains possible), no replay; inspect the before/after facts before any retry"
                    .to_owned(),
            ));
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            cancelled.store(true, Ordering::SeqCst);
            return Err(refuse(
                "ax-worker",
                "AX worker died before reporting; press state unknown, no replay".to_owned(),
            ));
        }
    };
    let report = FixtureMenuReport {
        outcome: "pressed".to_owned(),
        error: None,
        pid,
        wid: args.wid,
        item: item.key().to_owned(),
        caller: args.caller.clone(),
        secure_input: crate::ffi::secure_input_known(),
        ax_trusted: crate::ffi::ax_is_process_trusted(),
        proof_us,
        press_latency_us: Some(outcome.press_latency_us),
        dispatch_ns: outcome.dispatch_ns,
        return_ns: outcome.return_ns,
        cg_owner_before,
        cg_owner_after: outcome.cg_owner_after,
        run_tag_agreement: true,
        focus_before: Some(outcome.focus_before),
        focus_after: outcome.focus_after,
        menu_before: outcome.before,
        menu_after: outcome.after,
        synthetic: false,
        note: "development-only standalone menu diagnostic: fresh signed-fixture proof plus selected-focus agreement plus exactly one menu AXPress; effect unclassified, native readback required separately; no input synthesis, fixed five-item vocabulary only"
            .to_owned(),
    };
    let out = serde_json::to_string(&report).map_err(|e| (1, e.to_string()));
    _watchdog.disarm();
    out
}

/// Non-macOS fallback: the fixture transport does not exist here.
#[cfg(not(target_os = "macos"))]
pub fn run_fixture_menu_press(_args: &FixtureMenuPressArgs) -> Result<String, (i32, String)> {
    Err((1, "fixture-menu-press is macOS-only".to_owned()))
}

/// Post-press focus fact: `Some` only when a fresh post-press proof agrees;
/// `None` when the post-press proof refuses. Never falls back to the
/// pre-press proof: cloning `focus_before` here would fabricate agreement.
#[cfg(target_os = "macos")]
fn focus_after_lite(post: Result<FocusProof, String>) -> Option<FocusProofLite> {
    post.ok().map(Into::into)
}

/// Outcome of the single AX worker pass: before/after inventories plus focus
/// proofs plus the press cost and the after CG owner. `Ok` means exactly one
/// press happened between the fact reads. `Err` means press state is UNKNOWN,
/// never no-press: focus/commit/interrupt/cancel refusals precede any press,
/// but an AX failure or a post-press read failure can follow an actual
/// actuation. Never infer no-press from `Err` and never replay without
/// inspecting the before/after facts. `focus_after` is `None` when the
/// post-press proof refuses (no fallback to the pre-press proof).
#[cfg(target_os = "macos")]
struct WorkerOutcome {
    before: MenuInventory,
    after: MenuInventory,
    focus_before: FocusProofLite,
    focus_after: Option<FocusProofLite>,
    press_latency_us: u64,
    dispatch_ns: Option<u64>,
    return_ns: Option<u64>,
    cg_owner_after: Option<u32>,
}

/// Failure of the single AX press pass: either a pre-dispatch refusal (the
/// press never dispatched, so no after-facts exist and no report is
/// emitted) or a post-dispatch unknown (the press was attempted, so it may
/// have actuated even when the clock stamps are missing: the caller samples
/// read-only after-facts and reports outcome unknown with exit 1). Neither
/// path replays or restores; exactly one `AXPress` dispatch per invocation
/// either way.
#[cfg(target_os = "macos")]
pub(crate) enum MenuPressFailure {
    Refused(String),
    Unknown {
        evidence: Box<UnknownPressEvidence>,
        error: String,
    },
}

#[cfg(target_os = "macos")]
impl From<String> for MenuPressFailure {
    fn from(error: String) -> Self {
        MenuPressFailure::Refused(error)
    }
}

/// Post-dispatch failure evidence retained for the unknown report: the
/// dispatch/return stamps plus the read-only before/after facts sampled
/// without any second actuation.
#[cfg(target_os = "macos")]
pub(crate) struct UnknownPressEvidence {
    pub before: MenuInventory,
    pub after: MenuInventory,
    pub focus_before: FocusProofLite,
    pub focus_after: Option<FocusProofLite>,
    pub dispatch_ns: Option<u64>,
    pub return_ns: Option<u64>,
    pub cg_owner_after: Option<u32>,
}

/// Pure post-effect failure classification over plain data (no OS touch):
/// a failure after the press was attempted means the single press may have
/// actuated (UNKNOWN: retain stamps plus evidence, report, never replay),
/// even when the host clock stamps are missing (`host_now_ns` legitimately
/// returns `None`); a failure before any attempt means nothing was
/// dispatched (refusal, no report). The attempt flag is set immediately
/// before `AXPress`, independent of the nullable clock stamps. Pure;
/// unit-tested with fake attempts.
pub fn press_failure_is_unknown(dispatch_attempted: bool) -> bool {
    dispatch_attempted
}

/// The whole AX pass on the owning worker thread: join the exact window,
/// prove selected focus, read before-inventory, join the slot handle, repeat
/// the cheap proof (fresh identity plus fresh CG through the shared pure
/// choke point, no socket, no subprocess) at the commit boundary, then press
/// exactly once with the last exact focused/frontmost proof immediately
/// before `AXPress` inside the single effect, then read after-facts. Every
/// `AxElement` is constructed, used, and dropped here; nothing crosses to
/// the waiting main thread except plain facts.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn menu_press_on_worker(
    pid: u32,
    caller: &str,
    wid: u32,
    item: MenuItem,
    signer: &str,
    snapshot: &SocketSnapshot,
    bound_exe: &str,
    bound_launch: &str,
    cancelled: &AtomicBool,
) -> Result<WorkerOutcome, MenuPressFailure> {
    use crate::ffi::{
        ax_bool_attribute, ax_error_name, ax_perform_action, ax_set_messaging_timeout,
    };
    use crate::fixture_zoom::{ZoomAuthInputs, check_zoom_authorization};
    use crate::mc_probe::MC_ACTION_PRESS;
    // One shared bounded join: the retained `window` below is the exact
    // joined fixture window every later read uses.
    let window = join_ax_window(pid, wid)?;
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("menu worker: pid {pid} out of range"))?;
    let focus_before = live_focus_proof(pid, wid)?;
    let before = read_menu_inventory(pid, wid);
    let slot = join_menu_slot(pid, item)?;
    // Commit boundary: fresh identity plus fresh CG through the same pure
    // choke point as the pre-press proof (no socket, no subprocess). A
    // re-created window, a relaunched process, or an activated Secure Input
    // refuses here, before the window server is touched. The signature was
    // proven at bind and is never re-forked: the retained tail is compared
    // again here so the commit judges through the identical choke point.
    // Focus is NOT proven here: the single exact focused/frontmost proof
    // lives inside the press effect below as the last gate before `AXPress`
    // (after the fresh enabled/session/cancel gates), so no stale focus
    // proof is ever passed as fresh. When CG withholds the
    // title but the fresh owner join already names the fixture pid, one
    // bounded AXTitle read off the retained joined window fills the title
    // source; a present CG title stays authoritative and an absent CG entry
    // still refuses.
    let fresh_cg = cg_entry_for(wid)?;
    let ax_title: Option<String> = match &fresh_cg {
        Some((owner, None)) if *owner == pid => crate::ffi::ax_string_attribute(&window, "AXTitle")
            .ok()
            .flatten()
            .filter(|title| !title.is_empty()),
        _ => None,
    };
    let commit: Result<(), String> = (|| {
        menu_session_gates()?;
        let fresh = crate::ffi::ns_app_identity_for_pid(pid_i32);
        check_zoom_authorization(&ZoomAuthInputs {
            caller: caller.to_owned(),
            wid,
            announced_pid: pid,
            snapshot: snapshot.clone(),
            bundle: fresh.bundle,
            sig_tail: Some(signer.to_owned()),
            expected_signer: signer.to_owned(),
            live_exe: fresh.exe,
            bound_exe: bound_exe.to_owned(),
            live_launch: fresh.launch_date,
            bound_launch: bound_launch.to_owned(),
            cg_entry: fresh_cg,
            ax_title,
            secure_input: crate::ffi::secure_input_known(),
        })
        .map_err(|e| e.replace("refusing fixture zoom", "refusing fixture menu"))
    })();
    let interrupted = crate::space_drag_diag_live::diag_interrupted();
    let fenced = cancelled.load(Ordering::SeqCst);
    // The single press of this invocation: menu-only vocabulary (`AXPress`
    // on the joined slot, never anything else). The pure `focus` leg passed
    // to the driver is `Ok` by construction: the real focus proof is the
    // last live gate inside the effect (after the enabled reread), so a
    // focus move after the before-facts refuses there instead of pressing
    // on a stale agreement. The retained handle is re-proven enabled inside
    // the effect immediately before actuation, so a handle that went stale
    // after the commit refuses instead of pressing; the effect runs exactly
    // once with no retry. The `AXPress` dispatch edge and return stamp the
    // shared host monotonic clock (the sampler joins these stamps on the
    // same origin); the stamps travel through `&mut` out-vars so the
    // single-effect signature stays unchanged.
    let mut dispatch_attempted = false;
    let mut dispatch_ns: Option<u64> = None;
    let mut return_ns: Option<u64> = None;
    let press_latency_us = match drive_single_menu_press(
        &before,
        item.key(),
        Ok(()),
        commit,
        interrupted,
        fenced,
        || {
            match ax_bool_attribute(&slot, "AXEnabled") {
                Ok(true) => {}
                Ok(false) => {
                    return Err(
                        "menu worker: retained slot reads disabled; refusing stale target"
                            .to_owned(),
                    );
                }
                Err(code) => {
                    return Err(format!(
                        "menu worker: retained slot liveness unreadable ({}); refusing",
                        ax_error_name(code)
                    ));
                }
            }
            // Last gate before actuation: fresh exact focused/frontmost
            // proof (focused leg only) after the enabled/session/cancel
            // gates. A moved key window refuses here.
            live_focus_proof(pid, wid).map_err(|e| {
                format!(
                    "menu worker: focus agreement lost immediately before press; refusing ({e})"
                )
            })?;
            // Final fence: `interrupted`/`fenced` were captured before the
            // blocking enabled reread and focus proof above, so a latch or
            // main-bound lapse that landed during those reads refuses here.
            // Session gates are re-proven too (cheap local reads, no
            // socket/subprocess): a lock or Secure Input flip during the
            // reads refuses as well. Fresh interrupt/cancel is judged last
            // so the worker-deadline fence is the closest gate to `AXPress`.
            menu_session_gates().map_err(|e| {
                format!("menu worker: session gate lost immediately before press; refusing ({e})")
            })?;
            refuse_if_late_cancelled(
                crate::space_drag_diag_live::diag_interrupted(),
                cancelled.load(Ordering::SeqCst),
            )?;
            let press_start = Instant::now();
            // Attempt flag first: `host_now_ns` legitimately returns `None`,
            // so a missing stamp must never reclassify an attempted press
            // as unattempted. Stamps stay nullable evidence only.
            dispatch_attempted = true;
            dispatch_ns = crate::ffi::host_now_ns();
            let outcome = ax_perform_action(&slot, MC_ACTION_PRESS).map_err(|code| {
                format!(
                    "menu worker: AXPress failed ({}) with no retry",
                    ax_error_name(code)
                )
            });
            return_ns = crate::ffi::host_now_ns();
            outcome?;
            Ok(press_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
        },
    ) {
        Ok(latency) => latency,
        Err(error) => {
            // No replay, no restore, no second dispatch: exactly one
            // `AXPress` per invocation either way. A failure after the press
            // was attempted means it may have actuated even with missing
            // stamps, so sample read-only after-facts and report outcome
            // unknown; a failure before any attempt means nothing was
            // dispatched.
            if !press_failure_is_unknown(dispatch_attempted) {
                return Err(MenuPressFailure::Refused(error));
            }
            let _ = ax_set_messaging_timeout(&slot, 2.0);
            let after = read_menu_inventory(pid, wid);
            let focus_after = focus_after_lite(live_focus_proof(pid, wid));
            let cg_owner_after = cg_entry_for(wid).ok().flatten().map(|(owner, _)| owner);
            return Err(MenuPressFailure::Unknown {
                evidence: Box::new(UnknownPressEvidence {
                    before,
                    after,
                    focus_before: focus_before.into(),
                    focus_after,
                    dispatch_ns,
                    return_ns,
                    cg_owner_after,
                }),
                error,
            });
        }
    };
    let after = read_menu_inventory(pid, wid);
    // Post-press fact only: a refused post-proof records `None`, never a
    // clone of the pre-press proof (fabricated agreement).
    let focus_after = focus_after_lite(live_focus_proof(pid, wid));
    let cg_owner_after = cg_entry_for(wid).ok().flatten().map(|(owner, _)| owner);
    Ok(WorkerOutcome {
        before,
        after,
        focus_before: focus_before.into(),
        focus_after,
        press_latency_us,
        dispatch_ns,
        return_ns,
        cg_owner_after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventoried(key: &str, found: bool, enabled: Option<bool>) -> MenuInventory {
        MenuInventory {
            menubar_found: true,
            menubar_status: "ok".to_owned(),
            top_level_count: Some(4),
            window_menu_found: true,
            window_menu_status: "ok".to_owned(),
            window_item_count: Some(9),
            items: vec![MenuItemFact {
                key: key.to_owned(),
                found,
                enabled,
                enabled_status: if found {
                    match enabled {
                        Some(_) => "ok".to_owned(),
                        None => "kAXErrorNoValue".to_owned(),
                    }
                } else {
                    "not-found".to_owned()
                },
            }],
        }
    }

    fn focus_ok() -> Result<(), String> {
        check_menu_focus(4242, 7, Some(4242), Some(4242), Some(7), "focused", true)
    }

    #[test]
    fn vocabulary_parses_exactly_five_keys() {
        assert_eq!(parse_menu_item("zoom"), Ok(MenuItem::Zoom));
        assert_eq!(parse_menu_item("fill"), Ok(MenuItem::Fill));
        assert_eq!(parse_menu_item("tile-left"), Ok(MenuItem::TileLeft));
        assert_eq!(parse_menu_item("tile-right"), Ok(MenuItem::TileRight));
        assert_eq!(parse_menu_item("return"), Ok(MenuItem::Return));
        for raw in [
            "",
            "Zoom",
            "tile_left",
            "fill ",
            "minimize",
            "--chord",
            "AXPress",
        ] {
            assert!(parse_menu_item(raw).is_err(), "{raw} must refuse");
        }
    }

    #[test]
    fn identifiers_match_exactly_without_leaking_into_reports() {
        // Fixed Loop `573ace3` selectors: exact match only, no case-fold,
        // no prefix guessing, no title-era strings.
        assert_eq!(MenuItem::Zoom.identifier(), "_performZoom:");
        assert_eq!(MenuItem::Fill.identifier(), "_zoomFill:");
        assert_eq!(MenuItem::TileLeft.identifier(), "_zoomLeft:");
        assert_eq!(MenuItem::TileRight.identifier(), "_zoomRight:");
        assert_eq!(MenuItem::Return.identifier(), "_zoomUntile:");
        assert!(MenuItem::Zoom.is_general());
        assert!(MenuItem::Fill.is_general());
        assert!(MenuItem::Return.is_general());
        assert!(!MenuItem::TileLeft.is_general());
        assert!(!MenuItem::TileRight.is_general());
        assert!(identifier_matches(MenuItem::Fill, "_zoomFill:"));
        assert!(identifier_matches(MenuItem::Return, "_zoomUntile:"));
        assert!(identifier_matches(MenuItem::TileLeft, "_zoomLeft:"));
        assert!(!identifier_matches(MenuItem::Zoom, "_zoomFill:"));
        assert!(!identifier_matches(MenuItem::Fill, "Fill"));
        assert!(!identifier_matches(
            MenuItem::Return,
            "Return to Previous Size"
        ));
        assert!(!identifier_matches(
            MenuItem::TileLeft,
            "Tile Window to Left of Screen"
        ));
        // Narrow legacy title fallback: own hand-built slots only.
        assert_eq!(MenuItem::Zoom.title_fallback(), Some("Zoom"));
        assert_eq!(MenuItem::Fill.title_fallback(), Some("Fill"));
        assert_eq!(MenuItem::TileLeft.title_fallback(), None);
        assert_eq!(MenuItem::TileRight.title_fallback(), None);
        assert_eq!(MenuItem::Return.title_fallback(), None);
        // Reports carry keys only: serialization of a full report-shaped
        // inventory must not contain any selector or title text.
        let inventory = inventoried("tile-left", true, Some(true));
        let json = serde_json::to_string(&inventory).unwrap();
        assert!(json.contains("\"tile-left\""));
        assert!(!json.contains("_zoomLeft:"));
        assert!(!json.contains("Tile Window"));
    }

    #[test]
    fn focus_agreement_requires_every_leg() {
        assert!(focus_ok().is_ok());
        for (name, slps, ws, selected, source, owned, needle) in [
            (
                "slps-missing",
                None,
                Some(4242),
                Some(7),
                "focused",
                true,
                "frontmost pids unreadable",
            ),
            (
                "ws-missing",
                Some(4242),
                None,
                Some(7),
                "focused",
                true,
                "frontmost pids unreadable",
            ),
            (
                "slps-foreign",
                Some(31337),
                Some(4242),
                Some(7),
                "focused",
                true,
                "focus agreement failed",
            ),
            (
                "ws-foreign",
                Some(4242),
                Some(31337),
                Some(7),
                "focused",
                true,
                "focus agreement failed",
            ),
            (
                "selected-missing",
                Some(4242),
                Some(4242),
                None,
                "none",
                true,
                "direct focused/main window missing",
            ),
            (
                "selected-foreign",
                Some(4242),
                Some(4242),
                Some(9),
                "focused",
                true,
                "focus agreement failed",
            ),
            (
                "selected-zero",
                Some(4242),
                Some(4242),
                Some(0),
                "focused",
                true,
                "focus agreement failed",
            ),
            // Main-leg-only agreement refuses even when the wid matches:
            // app menus target the key (focused) window.
            (
                "selected-main-matching-wid",
                Some(4242),
                Some(4242),
                Some(7),
                "main",
                true,
                "not the focused leg",
            ),
            (
                "selected-none-matching-wid",
                Some(4242),
                Some(4242),
                Some(7),
                "none",
                true,
                "not the focused leg",
            ),
            (
                "selected-empty-source",
                Some(4242),
                Some(4242),
                Some(7),
                "",
                true,
                "not the focused leg",
            ),
            (
                "cg-lost",
                Some(4242),
                Some(4242),
                Some(7),
                "focused",
                false,
                "not owned by pid",
            ),
        ] {
            let err = check_menu_focus(4242, 7, slps, ws, selected, source, owned).unwrap_err();
            assert!(err.contains(needle), "{name}: unexpected: {err}");
        }
        // Reserved pid and zero wid refuse before any leg comparison.
        assert!(check_menu_focus(1, 7, Some(1), Some(1), Some(7), "focused", true).is_err());
        assert!(
            check_menu_focus(4242, 0, Some(4242), Some(4242), Some(0), "focused", true).is_err()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn focus_after_never_falls_back_to_before_proof() {
        let before = FocusProof {
            slps_pid: 4242,
            ws_pid: 4242,
            selected_wid: Some(7),
            selected_source: "focused".to_owned(),
            cg_owned: true,
        };
        // A refused post-proof records `None`, never a clone of `before`.
        let after: Option<FocusProofLite> =
            focus_after_lite(Err("menu focus: workspace frontmost unreadable".to_owned()));
        assert!(after.is_none(), "refused post-proof must be None");
        let before_lite = FocusProofLite::from(before);
        assert_ne!(
            after.map(|lite| lite.slps_pid),
            Some(before_lite.slps_pid),
            "None must not equal the pre-press proof"
        );
        // An agreeing post-proof passes through with agreement marked.
        let agreeing = FocusProof {
            slps_pid: 4242,
            ws_pid: 4242,
            selected_wid: Some(7),
            selected_source: "focused".to_owned(),
            cg_owned: true,
        };
        let after = focus_after_lite(Ok(agreeing)).expect("agreeing post-proof keeps Some");
        assert!(after.agreed);
        assert_eq!(after.selected_source, "focused");
    }

    #[test]
    fn item_pressable_requires_joined_and_enabled() {
        assert!(menu_item_pressable(&inventoried("fill", true, Some(true)), "fill").is_ok());
        let err = menu_item_pressable(&inventoried("fill", true, Some(false)), "fill").unwrap_err();
        assert!(err.contains("disabled"), "unexpected: {err}");
        let err = menu_item_pressable(&inventoried("fill", true, None), "fill").unwrap_err();
        assert!(err.contains("unreadable"), "unexpected: {err}");
        let err = menu_item_pressable(&inventoried("fill", false, None), "fill").unwrap_err();
        assert!(err.contains("join missed"), "unexpected: {err}");
        let err = menu_item_pressable(&inventoried("zoom", true, Some(true)), "fill").unwrap_err();
        assert!(err.contains("not inventoried"), "unexpected: {err}");
        // Absent Window menu: empty slot list refuses without guessing.
        let err =
            menu_item_pressable(&failed_inventory("menubar join missed (x)"), "zoom").unwrap_err();
        assert!(err.contains("not inventoried"), "unexpected: {err}");
    }

    #[test]
    fn authorize_menu_press_orders_gates_before_any_press() {
        let inventory = inventoried("return", true, Some(true));
        let ok: Result<(), String> = Ok(());
        assert!(authorize_menu_press(&inventory, "return", &ok, &ok, false, false).is_ok());
        // Ordering is pinned: item beats focus, focus beats commit, commit
        // beats interrupt, interrupt beats cancel.
        let mut disabled = inventoried("return", true, Some(false));
        let err = authorize_menu_press(
            &disabled,
            "return",
            &Err("focus bad".to_owned()),
            &Err("commit bad".to_owned()),
            true,
            true,
        )
        .unwrap_err();
        assert!(err.contains("disabled"), "item must win: {err}");
        disabled.items[0].enabled = Some(true);
        let err = authorize_menu_press(
            &disabled,
            "return",
            &Err("focus bad".to_owned()),
            &Err("commit bad".to_owned()),
            true,
            true,
        )
        .unwrap_err();
        assert!(err.contains("menu worker focus"), "focus must win: {err}");
        let err = authorize_menu_press(
            &disabled,
            "return",
            &ok,
            &Err("commit bad".to_owned()),
            true,
            true,
        )
        .unwrap_err();
        assert!(err.contains("menu worker commit"), "commit must win: {err}");
        let err = authorize_menu_press(&disabled, "return", &ok, &ok, true, true).unwrap_err();
        assert!(err.contains("latched"), "interrupt must win: {err}");
        let err = authorize_menu_press(&disabled, "return", &ok, &ok, false, true).unwrap_err();
        assert!(err.contains("bound lapsed"), "cancel must refuse: {err}");
    }

    #[test]
    fn drive_single_menu_press_presses_once_on_ok_and_never_on_refusal() {
        use std::cell::Cell;
        let inventory = inventoried("zoom", true, Some(true));
        let ok: Result<(), String> = Ok(());
        // Authorized: the fake effect runs exactly once and its value passes
        // through (it stands in for the single AXPress call site).
        let calls = Cell::new(0u32);
        let latency = drive_single_menu_press(&inventory, "zoom", ok, Ok(()), false, false, || {
            calls.set(calls.get() + 1);
            Ok(77u64)
        })
        .expect("authorized press drives");
        assert_eq!(latency, 77);
        assert_eq!(calls.get(), 1);
        // Every refusal drives zero effects.
        type RefusalVector<'a> = (
            &'a str,
            &'a MenuInventory,
            Result<(), String>,
            Result<(), String>,
            bool,
            bool,
        );
        let disabled = inventoried("zoom", true, Some(false));
        let cases: Vec<RefusalVector<'_>> = vec![
            ("item", &disabled, Ok(()), Ok(()), false, false),
            (
                "focus",
                &inventory,
                Err("focus bad".to_owned()),
                Ok(()),
                false,
                false,
            ),
            (
                "commit",
                &inventory,
                Ok(()),
                Err("commit bad".to_owned()),
                false,
                false,
            ),
            ("interrupt", &inventory, Ok(()), Ok(()), true, false),
            ("cancel", &inventory, Ok(()), Ok(()), false, true),
        ];
        for (name, facts, focus, commit, interrupted, cancelled) in cases {
            let calls = Cell::new(0u32);
            let outcome = drive_single_menu_press(
                facts,
                "zoom",
                focus,
                commit,
                interrupted,
                cancelled,
                || {
                    calls.set(calls.get() + 1);
                    Ok(0u64)
                },
            );
            assert!(outcome.is_err(), "{name} must refuse");
            assert_eq!(calls.get(), 0, "{name} must drive no effect");
        }
    }

    #[test]
    fn late_cancel_during_pre_effect_reads_refuses_without_press() {
        use std::cell::Cell;
        use std::sync::atomic::{AtomicBool, Ordering};
        // Pure fence: fresh values refuse, quiet values pass.
        assert!(refuse_if_late_cancelled(false, false).is_ok());
        assert!(refuse_if_late_cancelled(true, false).is_err());
        assert!(refuse_if_late_cancelled(false, true).is_err());
        // Driver injection: the pre-read snapshot passes the driver gates,
        // then the fence flips during the blocking reads (simulated by
        // setting the atomic first inside the effect, as the worker main
        // bound would). The effect's fresh recheck refuses before any
        // press, so the effect body after the recheck never runs.
        let inventory = inventoried("zoom", true, Some(true));
        let fence = AtomicBool::new(false);
        let calls = Cell::new(0u32);
        let outcome: Result<u64, String> =
            drive_single_menu_press(&inventory, "zoom", Ok(()), Ok(()), false, false, || {
                fence.store(true, Ordering::SeqCst);
                refuse_if_late_cancelled(false, fence.load(Ordering::SeqCst))?;
                calls.set(calls.get() + 1);
                Ok(0u64)
            });
        let err = outcome.unwrap_err();
        assert!(err.contains("lapsed"), "late fence must refuse: {err}");
        assert_eq!(calls.get(), 0, "late fence must drive no press");
    }

    #[test]
    fn inventory_and_report_carry_no_secret_or_content_keys() {
        let inventory = MenuInventory {
            menubar_found: true,
            menubar_status: "ok".to_owned(),
            top_level_count: Some(4),
            window_menu_found: true,
            window_menu_status: "ok".to_owned(),
            window_item_count: Some(9),
            items: vec![
                MenuItemFact {
                    key: "zoom".to_owned(),
                    found: true,
                    enabled: Some(true),
                    enabled_status: "ok".to_owned(),
                },
                missing_item("fill", "not-found"),
            ],
        };
        let json = serde_json::to_string(&inventory).unwrap();
        for key in [
            "\"title\"",
            "\"content\"",
            "\"path\"",
            "\"exe\"",
            "\"executable\"",
            "\"nonce\"",
            "\"socket\"",
            "\"signer\"",
            "\"bundle\"",
            "Tile Window",
        ] {
            assert!(!json.contains(key), "menu inventory must not carry {key}");
        }
        let report = FixtureMenuReport {
            outcome: "pressed".to_owned(),
            error: None,
            pid: 4242,
            wid: 7,
            item: "zoom".to_owned(),
            caller: "menu-leg-1".to_owned(),
            secure_input: Some(false),
            ax_trusted: Some(true),
            proof_us: 11,
            press_latency_us: Some(13),
            dispatch_ns: Some(1_700_000_000_000_000_000),
            return_ns: Some(1_700_000_000_001_000_000),
            cg_owner_before: Some(4242),
            cg_owner_after: Some(4242),
            run_tag_agreement: true,
            focus_before: Some(FocusProofLite {
                slps_pid: 4242,
                ws_pid: 4242,
                selected_wid: Some(7),
                selected_source: "focused".to_owned(),
                agreed: true,
            }),
            focus_after: None,
            menu_before: inventory.clone(),
            menu_after: inventory,
            synthetic: false,
            note: "test".to_owned(),
        };
        let json = serde_json::to_string(&report).unwrap();
        for key in [
            "\"title\"",
            "\"content\"",
            "\"path\"",
            "\"exe\"",
            "\"executable\"",
            "\"nonce\"",
            "\"socket\"",
            "\"signer\"",
            "\"bundle\"",
        ] {
            assert!(!json.contains(key), "menu report must not carry {key}");
        }
        assert!(json.contains("\"outcome\":\"pressed\""));
        assert!(json.contains("\"synthetic\":false"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn failed_effect_after_dispatch_edge_is_unknown_with_stamps_retained() {
        // Fake-effect regression: the single effect marks the press
        // attempted, then `AXPress` fails with the host clock unavailable
        // (`dispatch_ns`/`return_ns` stay `None`). Classification must be
        // UNKNOWN with the missing stamps retained (never a silent
        // stamp-driven refusal); a failure before any attempt stays a
        // refusal. The fake effect runs exactly once: no second action.
        use std::cell::Cell;
        let inventory = inventoried("zoom", true, Some(true));
        let mut dispatch_attempted = false;
        let dispatch_ns: Option<u64> = None;
        let return_ns: Option<u64> = None;
        let calls = Cell::new(0u32);
        let result =
            drive_single_menu_press(&inventory, "zoom", Ok(()), Ok(()), false, false, || {
                calls.set(calls.get() + 1);
                dispatch_attempted = true;
                // Host clock unavailable: stamps stay `None` even though the
                // press was attempted.
                Err("menu worker: AXPress failed (kAXErrorFailure) with no retry".to_owned())
            });
        assert!(result.is_err());
        assert_eq!(calls.get(), 1);
        assert!(press_failure_is_unknown(dispatch_attempted));
        let failure = MenuPressFailure::Unknown {
            evidence: Box::new(UnknownPressEvidence {
                before: inventory.clone(),
                after: inventory.clone(),
                focus_before: FocusProofLite {
                    slps_pid: 4242,
                    ws_pid: 4242,
                    selected_wid: Some(7),
                    selected_source: "focused".to_owned(),
                    agreed: true,
                },
                focus_after: None,
                dispatch_ns,
                return_ns,
                cg_owner_after: Some(4242),
            }),
            error: "menu worker: AXPress failed (kAXErrorFailure) with no retry".to_owned(),
        };
        match failure {
            MenuPressFailure::Unknown { evidence, error } => {
                assert_eq!(evidence.dispatch_ns, None);
                assert_eq!(evidence.return_ns, None);
                assert!(error.contains("kAXErrorFailure"));
            }
            MenuPressFailure::Refused(_) => {
                panic!("attempted failure must be unknown even with missing stamps")
            }
        }
        assert!(!press_failure_is_unknown(false));
    }
}
