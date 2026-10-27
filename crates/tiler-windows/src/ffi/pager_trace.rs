//! Bounded in-product native event trace for taskbar pager overlays.
//!
//! Ordered event batch, not counters: every relevant OS-observed native
//! message retains its flags/insert-after/show/size/position args, and every
//! actual tiler-issued native effect retains its operation plus args. The
//! host drains once per pump and joins the batch to the drain-context
//! production triple (tick/request/observation/revision) for correlation
//! only: queued OS messages carry no causal request attribution, and the
//! sequence field records observed order.
//!
//! Privacy: per-window identity is an opaque monotonically assigned
//! lifecycle token (`pw{n}`), never a raw HWND, PID, device name, path, or
//! title. Native lifecycle before registration (the `WM_CREATE` inside
//! `CreateWindowExW`, or `WM_DESTROY` after unregister) has no token and
//! logs as `unknown`, never fabricated. Insert-after is a bounded word;
//! `HWND_TOP` (0) and null are indistinguishable, so both read
//! `top-or-null`, never distinct values.
//!
//! Bounds: 128 events between drains (diagnostic-only cap); overflow folds
//! into an explicit `omitted` count. Hide/show transitions are never
//! coalesced: each lands as its own event up to the cap.
//!
//! Threading: the pager documents loop-thread confinement (windows are
//! created, owned, and destroyed on the creating thread and the procedure
//! runs there), so this buffer is thread-local. Pushes never hold the
//! registry lock, a state borrow, or any mutex over a native call: callers
//! push immediately before (tiler effects, preserving causal order) or at
//! entry (OS observations), then release before the native work. Disabled
//! tracing skips all capture and allocation: pushes return before touching
//! the buffer, and no extra native query runs for trace purposes.

use std::cell::RefCell;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
};

/// Diagnostic-only cap: events retained between drains. Overflow counts in
/// `omitted` instead of growing.
pub const TRACE_CAP: usize = 128;

/// One traced native event. All args are bounded value words or geometry;
/// never raw handles, pointers, paths, titles, or error strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerTraceEvent {
    /// Observed order within this thread (monotonic, never reset).
    pub seq: u64,
    /// Bounded vocabulary kind (see push sites).
    pub kind: &'static str,
    /// Opaque lifecycle token; `None` is pre-registration/unknown, never
    /// fabricated.
    pub win: Option<u64>,
    /// `WINDOWPOS` flags / move-show flags word where relevant.
    pub flags: Option<u32>,
    /// Bounded insert-after word where relevant.
    pub after: Option<&'static str>,
    /// `WM_SHOWWINDOW` show bit where relevant.
    pub show: Option<bool>,
    /// `WM_SHOWWINDOW` status word where relevant.
    pub show_status: Option<u32>,
    /// Position/size words where relevant.
    pub x: Option<i32>,
    /// Position/size words where relevant.
    pub y: Option<i32>,
    /// Position/size words where relevant.
    pub w: Option<i32>,
    /// Position/size words where relevant.
    pub h: Option<i32>,
    /// `WM_SIZE` type word where relevant.
    pub size_type: Option<u32>,
    /// Window DPI at create where relevant.
    pub dpi: Option<u32>,
}

impl PagerTraceEvent {
    /// Bounded JSON row: only present args serialize, plus `seq`, `kind`,
    /// and `win` (`pw{n}` or `unknown`).
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("seq".to_owned(), serde_json::Value::from(self.seq));
        map.insert("kind".to_owned(), serde_json::Value::from(self.kind));
        map.insert(
            "win".to_owned(),
            match self.win {
                Some(token) => serde_json::Value::from(format!("pw{token}")),
                None => serde_json::Value::from("unknown"),
            },
        );
        if let Some(flags) = self.flags {
            map.insert("flags".to_owned(), serde_json::Value::from(flags));
        }
        if let Some(after) = self.after {
            map.insert("after".to_owned(), serde_json::Value::from(after));
        }
        if let Some(show) = self.show {
            map.insert("show".to_owned(), serde_json::Value::from(show));
        }
        if let Some(status) = self.show_status {
            map.insert("status".to_owned(), serde_json::Value::from(status));
        }
        if let Some(x) = self.x {
            map.insert("x".to_owned(), serde_json::Value::from(x));
        }
        if let Some(y) = self.y {
            map.insert("y".to_owned(), serde_json::Value::from(y));
        }
        if let Some(w) = self.w {
            map.insert("w".to_owned(), serde_json::Value::from(w));
        }
        if let Some(h) = self.h {
            map.insert("h".to_owned(), serde_json::Value::from(h));
        }
        if let Some(size_type) = self.size_type {
            map.insert("size_type".to_owned(), serde_json::Value::from(size_type));
        }
        if let Some(dpi) = self.dpi {
            map.insert("dpi".to_owned(), serde_json::Value::from(dpi));
        }
        serde_json::Value::Object(map)
    }
}

struct TraceState {
    enabled: bool,
    buf: Vec<PagerTraceEvent>,
    omitted: u64,
    seq: u64,
    next_token: u64,
    // HWND key -> opaque token. A small vector suffices (one entry per
    // overlay); it keeps the thread-local initializer const without a
    // hash map.
    map: Vec<(u64, u64)>,
}

impl TraceState {
    const fn fresh() -> Self {
        Self {
            enabled: false,
            buf: Vec::new(),
            omitted: 0,
            seq: 0,
            next_token: 0,
            map: Vec::new(),
        }
    }

    fn lookup(&self, key: u64) -> Option<u64> {
        self.map.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    fn remove(&mut self, key: u64) -> Option<u64> {
        let index = self.map.iter().position(|(k, _)| *k == key)?;
        Some(self.map.swap_remove(index).1)
    }
}

thread_local! {
    static TRACE: RefCell<TraceState> = const { RefCell::new(TraceState::fresh()) };
}

/// Enable or disable capture. The host calls this before the first pager
/// operation each pump; disabling clears the pending batch so nothing
/// lingers unread. A repeat enable preserves the pending batch so
/// between-pump OS paint/hide/show survives. Tokens and sequence stay
/// monotonic across toggles.
pub fn set_enabled(on: bool) {
    TRACE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.enabled == on {
            return;
        }
        state.enabled = on;
        if !on {
            state.buf.clear();
            state.omitted = 0;
        }
    });
}

/// Whether capture is active on this thread.
pub fn is_enabled() -> bool {
    TRACE.with(|cell| cell.borrow().enabled)
}

/// Bounded insert-after word. `HWND_TOP` is 0, which is also null, so the
/// two are indistinguishable: both read `top-or-null`, never distinct.
/// Compares raw `isize` values (`HWND_TOPMOST`=-1, `HWND_NOTOPMOST`=-2,
/// `HWND_BOTTOM`=1) so no extra native query runs.
#[must_use]
pub fn after_word(after: HWND) -> &'static str {
    let raw = after as isize;
    if raw == -1 {
        "topmost"
    } else if raw == -2 {
        "no-topmost"
    } else if raw == 1 {
        "bottom"
    } else if raw == 0 {
        "top-or-null"
    } else {
        "other"
    }
}

fn hwnd_key(hwnd: HWND) -> u64 {
    hwnd as usize as u64
}

/// Assign the next opaque lifecycle token for a freshly created window.
/// Skips allocation entirely while disabled. Replaces any stale entry for
/// the same key so re-created handles never alias.
pub fn assign_window(hwnd: HWND) -> Option<u64> {
    TRACE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state.enabled {
            return None;
        }
        state.next_token += 1;
        let token = state.next_token;
        let key = hwnd_key(hwnd);
        state.map.retain(|(k, _)| *k != key);
        state.map.push((key, token));
        Some(token)
    })
}

/// Resolve the opaque token for a live window, or `None` for unknown
/// lifecycle (pre-registration create/destroy, departed window) or while
/// disabled. No native query runs.
pub fn token_for(hwnd: HWND) -> Option<u64> {
    TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    })
}

/// Revoke the token for a window about to be destroyed. Returns the token
/// so the destroy effect itself still carries identity; later OS
/// `WM_DESTROY` traffic then reads `unknown` honestly.
pub fn revoke_window(hwnd: HWND) -> Option<u64> {
    TRACE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state.enabled {
            return None;
        }
        state.remove(hwnd_key(hwnd))
    })
}

fn push_built(event: impl FnOnce(u64) -> PagerTraceEvent) {
    TRACE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state.enabled {
            return;
        }
        state.seq += 1;
        let seq = state.seq;
        if state.buf.len() >= TRACE_CAP {
            state.omitted += 1;
            return;
        }
        state.buf.push(event(seq));
    });
}

/// OS-observed `WM_WINDOWPOSCHANGING`: flags, insert-after word, and
/// position/size words.
pub fn push_pos_changing(hwnd: HWND, flags: u32, after: HWND, x: i32, y: i32, w: i32, h: i32) {
    let after = after_word(after);
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    // Disabled check above borrowed once; push_built re-checks cheaply.
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "pos-changing",
        win,
        flags: Some(flags),
        after: Some(after),
        show: None,
        show_status: None,
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        size_type: None,
        dpi: None,
    });
}

/// OS-observed `WM_WINDOWPOSCHANGED`: same words as changing.
pub fn push_pos_changed(hwnd: HWND, flags: u32, after: HWND, x: i32, y: i32, w: i32, h: i32) {
    let after = after_word(after);
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "pos-changed",
        win,
        flags: Some(flags),
        after: Some(after),
        show: None,
        show_status: None,
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        size_type: None,
        dpi: None,
    });
}

/// OS-observed `WM_WINDOWPOSCHANGING` with no `WINDOWPOS` (`lparam == 0`):
/// records the occurrence with no fabricated flags or insert-after.
pub fn push_pos_changing_unknown(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "pos-changing",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// OS-observed `WM_WINDOWPOSCHANGED` with no `WINDOWPOS`: occurrence only.
pub fn push_pos_changed_unknown(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "pos-changed",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}
/// OS-observed `WM_SHOWWINDOW`: show bit plus status word. Every transition
/// lands as its own event up to the cap.
pub fn push_show(hwnd: HWND, show: bool, status: u32) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "show",
        win,
        flags: None,
        after: None,
        show: Some(show),
        show_status: Some(status),
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// OS-observed `WM_SIZE`: type word plus client size.
pub fn push_size(hwnd: HWND, size_type: u32, w: i32, h: i32) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "size",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: Some(w),
        h: Some(h),
        size_type: Some(size_type),
        dpi: None,
    });
}

/// OS-observed `WM_MOVE`: client origin.
pub fn push_move(hwnd: HWND, x: i32, y: i32) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "move",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: Some(x),
        y: Some(y),
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// OS-observed paint begin (`WM_PAINT` entry).
pub fn push_paint_begin(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "paint-begin",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// OS-observed paint end (after `EndPaint`).
pub fn push_paint_end(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "paint-end",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// OS-observed `WM_ERASEBKGND`. The HDC word is never logged.
pub fn push_erase(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "erase",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// OS-observed `WM_CREATE`. Runs before registration, so the window is
/// honestly `unknown`.
pub fn push_os_create(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "os-create",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// OS-observed `WM_DESTROY`. Runs after unregister, so the window is
/// honestly `unknown`.
pub fn push_os_destroy(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "os-destroy",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// Tiler-issued move/show at a strip: operation plus strip args.
pub fn push_tiler_move(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "tiler-move",
        win,
        flags: Some(SWP_NOACTIVATE | SWP_SHOWWINDOW),
        after: Some("topmost"),
        show: None,
        show_status: None,
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        size_type: None,
        dpi: None,
    });
}

/// Tiler-issued show that actually ran (`ShowWindow` after a hidden read).
pub fn push_tiler_ensure(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "tiler-ensure",
        win,
        flags: None,
        after: None,
        show: Some(true),
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// Tiler-issued hide (`ShowWindow(HIDE)`).
pub fn push_tiler_hide(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "tiler-hide",
        win,
        flags: None,
        after: None,
        show: Some(false),
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// Tiler-issued topmost reassert (existing 2s cadence, no move/size).
pub fn push_tiler_reassert(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "tiler-reassert",
        win,
        flags: Some(SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW),
        after: Some("topmost"),
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// Tiler-issued create with its strip args and DPI. The token was assigned
/// at creation.
pub fn push_tiler_create(token: u64, x: i32, y: i32, w: i32, h: i32, dpi: u32) {
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "tiler-create",
        win: Some(token),
        flags: None,
        after: Some("topmost"),
        show: None,
        show_status: None,
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        size_type: None,
        dpi: Some(dpi),
    });
}

/// Tiler-issued destroy (before `DestroyWindow`).
pub fn push_tiler_destroy(token: u64) {
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "tiler-destroy",
        win: Some(token),
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// Calibration hide from a display/DPI/settings broadcast (app-local
/// `ShowWindow(HIDE)` on the broadcast path).
pub fn push_calib_hide(hwnd: HWND) {
    let win = TRACE.with(|cell| {
        let state = cell.borrow();
        if !state.enabled {
            return None;
        }
        state.lookup(hwnd_key(hwnd))
    });
    if !is_enabled() {
        return;
    }
    push_built(|seq| PagerTraceEvent {
        seq,
        kind: "calib-hide",
        win,
        flags: None,
        after: None,
        show: None,
        show_status: None,
        x: None,
        y: None,
        w: None,
        h: None,
        size_type: None,
        dpi: None,
    });
}

/// Drain the ordered batch plus the overflow count. The buffer clears;
/// tokens and sequence stay monotonic so later batches keep observed order.
pub fn drain() -> (Vec<PagerTraceEvent>, u64) {
    TRACE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state.enabled {
            return (Vec::new(), 0);
        }
        let omitted = state.omitted;
        state.omitted = 0;
        let events = std::mem::take(&mut state.buf);
        (events, omitted)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        TRACE.with(|cell| {
            *cell.borrow_mut() = TraceState::fresh();
        });
        set_enabled(true);
    }

    fn fake_hwnd(raw: isize) -> HWND {
        raw as HWND
    }

    #[test]
    fn after_words_keep_top_or_null_honest() {
        assert_eq!(after_word(fake_hwnd(-1)), "topmost");
        assert_eq!(after_word(fake_hwnd(-2)), "no-topmost");
        assert_eq!(after_word(fake_hwnd(1)), "bottom");
        // HWND_TOP (0) and null are the same value: never distinct.
        assert_eq!(after_word(fake_hwnd(0)), "top-or-null");
        assert_eq!(after_word(fake_hwnd(0x1234)), "other");
    }

    #[test]
    fn ordered_hide_then_show_retains_both_args() {
        reset();
        let hwnd = fake_hwnd(0x1000);
        let token = assign_window(hwnd).expect("token");
        push_tiler_hide(hwnd);
        push_show(hwnd, true, 0);
        push_tiler_ensure(hwnd);
        let (events, omitted) = drain();
        assert_eq!(omitted, 0);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, "tiler-hide");
        assert_eq!(events[0].win, Some(token));
        assert_eq!(events[1].kind, "show");
        assert_eq!(events[1].show, Some(true));
        assert_eq!(events[2].kind, "tiler-ensure");
        assert_eq!(events[0].show, Some(false));
        assert_eq!(events[2].show, Some(true));
        // Sequence records observed order.
        assert!(events[0].seq < events[1].seq && events[1].seq < events[2].seq);
    }

    #[test]
    fn tiler_move_and_reassert_carry_actual_pos_flags() {
        // `move_show` runs SetWindowPos(TOPMOST, NOACTIVATE|SHOWWINDOW) and
        // the reassert runs with NOMOVE|NOSIZE added: both ride `flags`.
        reset();
        let hwnd = fake_hwnd(0x1200);
        assign_window(hwnd).expect("token");
        push_tiler_move(hwnd, 0, 40, 144, 40);
        push_tiler_reassert(hwnd);
        let (events, omitted) = drain();
        assert_eq!(omitted, 0);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "tiler-move");
        assert_eq!(events[0].flags, Some(0x0050));
        assert_eq!(events[1].kind, "tiler-reassert");
        assert_eq!(events[1].flags, Some(0x0053));
        assert!(events[0].seq < events[1].seq);
    }

    #[test]
    fn repeated_enable_preserves_between_pump_batch() {
        // Host calls set_enabled(true) each pump: a repeat enable must not
        // discard between-pump OS paint/hide/show. Native show args and
        // position flags on both sides of the repeat enable all survive.
        reset();
        let hwnd = fake_hwnd(0x1100);
        assign_window(hwnd).expect("token");
        push_show(hwnd, false, 7);
        push_pos_changed(hwnd, 0x0080, fake_hwnd(0), 0, 40, 144, 40);
        set_enabled(true);
        push_show(hwnd, true, 7);
        push_pos_changed(hwnd, 0x0040, fake_hwnd(0), 0, 40, 144, 40);
        let (events, omitted) = drain();
        assert_eq!(omitted, 0);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].kind, "show");
        assert_eq!(events[0].show, Some(false));
        assert_eq!(events[0].show_status, Some(7));
        assert_eq!(events[1].flags, Some(0x0080));
        assert_eq!(events[2].kind, "show");
        assert_eq!(events[2].show, Some(true));
        assert_eq!(events[2].show_status, Some(7));
        assert_eq!(events[3].flags, Some(0x0040));
        assert!(events[0].seq < events[1].seq && events[1].seq < events[2].seq);
        assert!(events[2].seq < events[3].seq);
    }

    #[test]
    fn per_window_alias_and_unknown_lifecycle() {
        reset();
        let first = fake_hwnd(0x2000);
        let second = fake_hwnd(0x3000);
        let unknown = fake_hwnd(0x9999);
        let first_token = assign_window(first).expect("first token");
        let second_token = assign_window(second).expect("second token");
        assert_ne!(first_token, second_token);
        push_pos_changing(first, 0x0040, fake_hwnd(-1), 1, 2, 3, 4);
        push_pos_changing(second, 0x0080, fake_hwnd(1), 5, 6, 7, 8);
        // Pre-registration native traffic has no token: explicit unknown.
        push_os_create(unknown);
        let (events, _) = drain();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].win, Some(first_token));
        assert_eq!(events[0].flags, Some(0x0040));
        assert_eq!(events[0].after, Some("topmost"));
        assert_eq!(events[1].win, Some(second_token));
        assert_eq!(events[1].after, Some("bottom"));
        assert_eq!(events[2].win, None);
        assert_eq!(events[2].kind, "os-create");
    }

    #[test]
    fn disabled_trace_skips_capture_and_allocation() {
        reset();
        set_enabled(false);
        let hwnd = fake_hwnd(0x4000);
        assert_eq!(assign_window(hwnd), None);
        push_tiler_hide(hwnd);
        push_show(hwnd, false, 1);
        push_erase(hwnd);
        let (events, omitted) = drain();
        assert!(events.is_empty());
        assert_eq!(omitted, 0);
        assert!(!is_enabled());
        // Re-enable starts fresh with no residue.
        set_enabled(true);
        let (events, omitted) = drain();
        assert!(events.is_empty());
        assert_eq!(omitted, 0);
    }

    #[test]
    fn overflow_folds_into_omitted_count() {
        reset();
        let hwnd = fake_hwnd(0x5000);
        assign_window(hwnd);
        for _ in 0..(TRACE_CAP + 7) {
            push_tiler_hide(hwnd);
        }
        let (events, omitted) = drain();
        assert_eq!(events.len(), TRACE_CAP);
        assert_eq!(omitted, 7);
        // Omitted events still advance the sequence: the next retained
        // event continues past the overflow gap.
        push_tiler_hide(hwnd);
        let (events, omitted) = drain();
        assert_eq!(omitted, 0);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, (TRACE_CAP + 7 + 1) as u64);
        // Next drain is quiet again.
        let (events, omitted) = drain();
        assert!(events.is_empty());
        assert_eq!(omitted, 0);
    }

    #[test]
    fn paint_begin_end_and_erase_all_capture() {
        reset();
        let hwnd = fake_hwnd(0x6000);
        assign_window(hwnd);
        push_paint_begin(hwnd);
        push_erase(hwnd);
        push_paint_end(hwnd);
        let (events, _) = drain();
        assert_eq!(
            events.iter().map(|event| event.kind).collect::<Vec<_>>(),
            vec!["paint-begin", "erase", "paint-end"]
        );
    }
}
