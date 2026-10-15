use super::*;

/// CLI options for the read-only `children` command: explicit top-level
/// targets whose child windows are reported with verified process identity.
/// Never reads titles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildrenOptions {
    pub hwnds: Vec<u64>,
}

/// Parse `children --hwnd HWND [--hwnd HWND ...]`. At least one `--hwnd`
/// is required; zero targets is a refusal, never an all-window enumeration.
pub fn parse_children_args(args: &[String]) -> Result<ChildrenOptions, String> {
    let usage = "usage: children --hwnd HWND [--hwnd HWND ...]";
    let mut hwnds: Vec<u64> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--hwnd" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let hwnd = crate::test_window::parse_hwnd(value).ok_or_else(|| usage.to_owned())?;
                if !hwnds.contains(&hwnd) {
                    hwnds.push(hwnd);
                }
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    if hwnds.is_empty() {
        return Err("refuse: children requires at least one --hwnd".to_owned());
    }
    Ok(ChildrenOptions { hwnds })
}

/// CLI options for the read-only `inspect` command: one frozen allowlist.
/// Reports fresh identity, rectangles, and eligibility per entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectOptions {
    pub allowlist: PathBuf,
}

/// Parse `inspect --allowlist PATH`.
pub fn parse_inspect_args(args: &[String]) -> Result<InspectOptions, String> {
    let usage = "usage: inspect --allowlist PATH";
    let mut allowlist: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err(usage.to_owned());
    };
    Ok(InspectOptions { allowlist })
}

/// Exact-owner `workspace` control action (normal `tile` only): `select`
/// focuses an existing/trailing same-output workspace, `send` moves the
/// focused managed window there and follows, `stay` moves it without
/// following, `previous` toggles the previous view, `fullscreen` toggles
/// fullscreen on the focused managed window with no workspace move, `float`
/// toggles float and `sticky` toggles sticky on the focused managed window
/// (both test-needed routes, tentative pending user review), and the three
/// `relative` forms step the item 1 scoped ring (`relative` selects,
/// `send-relative` follows, `stay-relative` stays). No synthetic input, no
/// keyboard acceptance, never a proof path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WorkspaceAction {
    #[serde(rename = "select")]
    Select,
    #[serde(rename = "send")]
    Send,
    #[serde(rename = "stay")]
    Stay,
    #[serde(rename = "previous")]
    Previous,
    #[serde(rename = "fullscreen")]
    Fullscreen,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "sticky")]
    Sticky,
    #[serde(rename = "relative")]
    RelativeHistory,
    #[serde(rename = "send-relative")]
    RelativeSend,
    #[serde(rename = "stay-relative")]
    RelativeStay,
}

impl WorkspaceAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Send => "send",
            Self::Stay => "stay",
            Self::Previous => "previous",
            Self::Fullscreen => "fullscreen",
            Self::Float => "float",
            Self::Sticky => "sticky",
            Self::RelativeHistory => "relative",
            Self::RelativeSend => "send-relative",
            Self::RelativeStay => "stay-relative",
        }
    }

    /// True for the three send-family actions (numbered/relative follow/stay):
    /// they move the focused managed window and require a managed origin.
    /// Select/history/fullscreen/float/sticky actions never move workspaces;
    /// `fullscreen`/`float`/`sticky` still require a managed origin (they
    /// toggle the focused managed window), while select/history work on empty
    /// workspaces and unmanaged foreground.
    #[must_use]
    pub const fn is_send(self) -> bool {
        match self {
            Self::Send | Self::RelativeSend | Self::RelativeStay | Self::Stay => true,
            Self::Select
            | Self::Previous
            | Self::Fullscreen
            | Self::Float
            | Self::Sticky
            | Self::RelativeHistory => false,
        }
    }

    /// Explicit follow intent for the send family (`send`/`send-relative`
    /// follow; `stay`/`stay-relative` stay). `None` for select/history/
    /// fullscreen/float/sticky.
    #[must_use]
    pub const fn follow(self) -> Option<bool> {
        match self {
            Self::Send | Self::RelativeSend => Some(true),
            Self::Stay | Self::RelativeStay => Some(false),
            Self::Select
            | Self::Previous
            | Self::Fullscreen
            | Self::Float
            | Self::Sticky
            | Self::RelativeHistory => None,
        }
    }
}

/// Relative direction for the `workspace --relative` / `--send-relative` /
/// `--stay-relative` forms: an ordinal step in the item 1 scoped ring, never
/// MRU, resolved once before transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WorkspaceDirection {
    #[serde(rename = "previous")]
    Previous,
    #[serde(rename = "next")]
    Next,
}

impl WorkspaceDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::Next => "next",
        }
    }

    /// Ring step: -1 previous, +1 next.
    #[must_use]
    pub const fn delta(self) -> i32 {
        match self {
            Self::Previous => -1,
            Self::Next => 1,
        }
    }
}

/// CLI options for the exact-owner `workspace` control (normal `tile` only):
/// `--select`/`--send`/`--stay` take a digit index 0..=9; `--previous`,
/// `--fullscreen`, `--float`, and `--sticky` take no value;
/// `--relative`/`--send-relative`/`--stay-relative` take `previous`|`next`.
/// The CLI only queues a bounded single-pending request file; the owner loop
/// validates the full owner binding and dispatches through the existing
/// `workspace_do_select` / `workspace_do_send` resolvers (plus the pure
/// history resolvers) or, for `--fullscreen`, through the existing
/// project-owned fullscreen toggle (`fullscreen_toggle_decision` +
/// `enter_fullscreen`/`exit_fullscreen_owned` with the `RefuseAppOwned`
/// R-MAX-05 refusal), or, for `--float`/`--sticky`, through the existing
/// hook float/sticky dispatch arms (`dispatch_float_intent` /
/// `dispatch_sticky_intent` with every production fence). No synthetic
/// input, no keyboard acceptance, never a proof path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOptions {
    pub action: WorkspaceAction,
    pub index: u8,
    pub direction: Option<WorkspaceDirection>,
}

/// Parse `workspace` CLI forms. Exactly one action flag; indexed forms take a
/// digit 0..=9, relative forms take `previous`|`next`, `--previous`,
/// `--fullscreen`, `--float`, and `--sticky` take no value. Anything else
/// (including a missing value or both flags) refuses.
pub fn parse_workspace_args(args: &[String]) -> Result<WorkspaceOptions, String> {
    let usage = "usage: workspace (--select|--send|--stay) INDEX | --previous | --fullscreen | --float | --sticky | (--relative|--send-relative|--stay-relative) (previous|next)";
    let mut action: Option<WorkspaceAction> = None;
    let mut index: u8 = 0;
    let mut direction: Option<WorkspaceDirection> = None;
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let next = match flag {
            "--select" => WorkspaceAction::Select,
            "--send" => WorkspaceAction::Send,
            "--stay" => WorkspaceAction::Stay,
            "--previous" => WorkspaceAction::Previous,
            "--fullscreen" => WorkspaceAction::Fullscreen,
            "--float" => WorkspaceAction::Float,
            "--sticky" => WorkspaceAction::Sticky,
            "--relative" => WorkspaceAction::RelativeHistory,
            "--send-relative" => WorkspaceAction::RelativeSend,
            "--stay-relative" => WorkspaceAction::RelativeStay,
            _ => return Err(usage.to_owned()),
        };
        if action.is_some() {
            return Err(usage.to_owned());
        }
        i += 1;
        match next {
            WorkspaceAction::Select | WorkspaceAction::Send | WorkspaceAction::Stay => {
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u8 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed > 9 {
                    return Err("refuse: workspace index must be 0..=9".to_owned());
                }
                index = parsed;
                i += 1;
            }
            WorkspaceAction::Previous
            | WorkspaceAction::Fullscreen
            | WorkspaceAction::Float
            | WorkspaceAction::Sticky => {}
            WorkspaceAction::RelativeHistory
            | WorkspaceAction::RelativeSend
            | WorkspaceAction::RelativeStay => {
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                direction = Some(match value.as_str() {
                    "previous" => WorkspaceDirection::Previous,
                    "next" => WorkspaceDirection::Next,
                    _ => return Err(usage.to_owned()),
                });
                i += 1;
            }
        }
        action = Some(next);
    }
    match action {
        Some(action) => Ok(WorkspaceOptions {
            action,
            index,
            direction,
        }),
        None => Err(usage.to_owned()),
    }
}

/// Verify the raw received argv against the parsed `workspace` options:
/// exactly one action flag plus its matching value (none for `--previous`,
/// `--fullscreen`, `--float`, and `--sticky`), no unknown flags.
pub fn verify_workspace_argv_consistency(
    raw: &[String],
    parsed: &WorkspaceOptions,
) -> Result<(), String> {
    let usage = "usage: workspace (--select|--send|--stay) INDEX | --previous | --fullscreen | --float | --sticky | (--relative|--send-relative|--stay-relative) (previous|next)";
    let want = match parsed.action {
        WorkspaceAction::Select => "--select",
        WorkspaceAction::Send => "--send",
        WorkspaceAction::Stay => "--stay",
        WorkspaceAction::Previous => "--previous",
        WorkspaceAction::Fullscreen => "--fullscreen",
        WorkspaceAction::Float => "--float",
        WorkspaceAction::Sticky => "--sticky",
        WorkspaceAction::RelativeHistory => "--relative",
        WorkspaceAction::RelativeSend => "--send-relative",
        WorkspaceAction::RelativeStay => "--stay-relative",
    };
    match parsed.action {
        WorkspaceAction::Previous
        | WorkspaceAction::Fullscreen
        | WorkspaceAction::Float
        | WorkspaceAction::Sticky => {
            if raw.len() != 1 {
                return Err(usage.to_owned());
            }
        }
        WorkspaceAction::Select | WorkspaceAction::Send | WorkspaceAction::Stay => {
            if raw.len() != 2 {
                return Err(usage.to_owned());
            }
            let got: u8 = raw[1].parse().map_err(|_| usage.to_owned())?;
            if got != parsed.index {
                return Err("error: argv/parsed workspace mismatch (impossible)".to_owned());
            }
        }
        WorkspaceAction::RelativeHistory
        | WorkspaceAction::RelativeSend
        | WorkspaceAction::RelativeStay => {
            if raw.len() != 2 {
                return Err(usage.to_owned());
            }
            let want_direction = parsed.direction.ok_or_else(|| usage.to_owned())?;
            if raw[1] != want_direction.as_str() {
                return Err("error: argv/parsed workspace mismatch (impossible)".to_owned());
            }
        }
    }
    if raw[0] != want {
        return Err(usage.to_owned());
    }
    Ok(())
}

/// Versioned exact-owner workspace request body. `creation`/`pid`/`exe_path`/
/// `user_sid`/`session_id` bind the exact ledger owner; `action` is the
/// transport op; `index` is the digit for indexed actions (ignored
/// otherwise, still bounded); `direction` rides the relative step for the
/// three relative actions (`None` elsewhere); `correlation` is a
/// client-generated opaque token the owner echoes in its dispatch log. No
/// titles, no geometry, no secrets.
pub const WORKSPACE_REQUEST_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceRequest {
    pub v: u32,
    pub creation: String,
    pub pid: u32,
    pub exe_path: String,
    pub user_sid: String,
    pub session_id: u32,
    pub action: WorkspaceAction,
    pub index: u8,
    #[serde(default)]
    pub direction: Option<WorkspaceDirection>,
    pub correlation: String,
}

/// Render one workspace request body. Bounds only; full owner equality stays
/// with the owner-side check.
pub fn render_workspace_request(request: &WorkspaceRequest) -> String {
    serde_json::to_string(request).unwrap_or_default()
}

/// Parse and bound one workspace request body. Refuses malformed JSON,
/// version drift, empty identity, out-of-range index, missing/misplaced
/// direction (required exactly for the three relative actions, refused
/// elsewhere), unknown actions, and invalid correlation tokens. Owner
/// equality (exact creation/pid/exe/sid/session) stays with the caller, which
/// holds the live owner identity.
pub fn parse_workspace_request(json: &str) -> Result<WorkspaceRequest, String> {
    let request: WorkspaceRequest =
        serde_json::from_str(json).map_err(|_| "refuse: malformed workspace request".to_owned())?;
    if request.v != WORKSPACE_REQUEST_VERSION {
        return Err("refuse: workspace request version".to_owned());
    }
    if request.creation.is_empty()
        || request.pid == 0
        || request.exe_path.is_empty()
        || request.user_sid.is_empty()
        || request.correlation.is_empty()
    {
        return Err("refuse: malformed workspace request".to_owned());
    }
    if request.index > 9 {
        return Err("refuse: workspace index must be 0..=9".to_owned());
    }
    match request.action {
        WorkspaceAction::RelativeHistory
        | WorkspaceAction::RelativeSend
        | WorkspaceAction::RelativeStay => {
            if request.direction.is_none() {
                return Err("refuse: malformed workspace request".to_owned());
            }
        }
        WorkspaceAction::Select
        | WorkspaceAction::Send
        | WorkspaceAction::Stay
        | WorkspaceAction::Previous
        | WorkspaceAction::Fullscreen
        | WorkspaceAction::Float
        | WorkspaceAction::Sticky => {
            if request.direction.is_some() {
                return Err("refuse: malformed workspace request".to_owned());
            }
        }
    }
    if tiler_core::ids::CorrelationId::parse(&request.correlation).is_none() {
        return Err("refuse: malformed workspace request".to_owned());
    }
    Ok(request)
}

/// Exact KDE `requestResize` repeat tracker (portable pure step): the press
/// index for one keyboard-resize dispatch plus the stored repeat state for
/// the next press. Continues only when the focused window, direction, and
/// mode all match the stored press; any focus/direction/mode change restarts
/// at 0. Key-up never resets: like KDE, only a focus/direction/mode mismatch
/// restarts the schedule. The pixel step itself comes from
/// `tiler_core::cosmic_v1::keyboard_step_px` (12/14/16/18/20px cap), so an
/// unbounded repeat run stays capped downstream. Wire tokens throughout
/// (`left`/`right`/`up`/`down`, `outwards`/`inwards`) so the native
/// dispatcher and offline tests share this exact rule with no duplication.
/// No fingerprint input: the Windows rows cannot replicate KDE's carried
/// snapshot stability across own verified writes, and a fingerprint reset on
/// every own write would restart the mandated 12/14/16/18/20 schedule after
/// the first press. External drift still fails closed at the Engine
/// (`partial-observation`) and the next press then restarts the schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeRepeat {
    pub focused: String,
    pub direction: String,
    pub mode: String,
    pub next: u32,
}

/// Next keyboard-resize press index plus the stored state for the following
/// press. See [`ResizeRepeat`] for the exact continuation rule.
#[must_use]
pub fn resize_repeat_next(
    prev: Option<ResizeRepeat>,
    focused: &str,
    direction: &str,
    mode: &str,
) -> (u32, ResizeRepeat) {
    if let Some(state) = prev
        && state.focused == focused
        && state.direction == direction
        && state.mode == mode
    {
        let index = state.next;
        return (
            index,
            ResizeRepeat {
                focused: state.focused,
                direction: state.direction,
                mode: state.mode,
                next: index.saturating_add(1),
            },
        );
    }
    (
        0,
        ResizeRepeat {
            focused: focused.to_owned(),
            direction: direction.to_owned(),
            mode: mode.to_owned(),
            next: 1,
        },
    )
}

/// Exact-owner test-needed keyboard-resize control (tentative, pending user
/// review): `resize --direction DIR --mode MODE` queues one bounded request
/// for the normal `tile` loop only. The owner consumes it once, resolves the
/// live foreground as the resize subject through the production origin/
/// member/lifetime/scope gates, and dispatches through the real
/// `keyboard_tick` resize arm with the same fences and repeat tracker as a
/// physical chord. No synthetic input, no classifier bypass, never a proof
/// path. The production origin is the real captured foreground; the CLI
/// carries no HWND. proof owners refuse; suspended/elevated sessions refuse.
pub const RESIZE_REQUEST_FILE: &str = "resize.request";
pub const RESIZE_REQUEST_VERSION: u32 = 1;

/// CLI options for the exact-owner test-needed `resize` control (normal
/// `tile` only): `--direction` takes `left`|`right`|`up`|`down`,
/// `--mode` takes `outwards`|`inwards`, in exactly this order. The CLI only
/// queues a bounded single-pending request file; the owner loop validates
/// the full owner binding and dispatches through the production resize arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeOptions {
    pub direction: String,
    pub mode: String,
}

/// Wire-valid resize direction token (Engine parser vocabulary).
#[must_use]
pub fn resize_direction_valid(direction: &str) -> bool {
    matches!(direction, "left" | "right" | "up" | "down")
}

/// Wire-valid resize mode token (Engine parser vocabulary).
#[must_use]
pub fn resize_mode_valid(mode: &str) -> bool {
    matches!(mode, "outwards" | "inwards")
}

/// Parse the test-needed `resize` CLI form. Exactly `--direction DIR`
/// `--mode MODE` in this order; anything else (including swapped order, a
/// missing value, or repeated flags) refuses.
pub fn parse_resize_args(args: &[String]) -> Result<ResizeOptions, String> {
    let usage = "usage: resize --direction (left|right|up|down) --mode (outwards|inwards)";
    if args.len() != 4 {
        return Err(usage.to_owned());
    }
    if args[0] != "--direction" || args[2] != "--mode" {
        return Err(usage.to_owned());
    }
    if !resize_direction_valid(&args[1]) {
        return Err("refuse: resize direction must be left|right|up|down".to_owned());
    }
    if !resize_mode_valid(&args[3]) {
        return Err("refuse: resize mode must be outwards|inwards".to_owned());
    }
    Ok(ResizeOptions {
        direction: args[1].clone(),
        mode: args[3].clone(),
    })
}

/// Verify the raw received argv against the parsed `resize` options:
/// exactly `--direction DIR --mode MODE` with matching values, no unknown
/// flags.
pub fn verify_resize_argv_consistency(
    raw: &[String],
    parsed: &ResizeOptions,
) -> Result<(), String> {
    let usage = "usage: resize --direction (left|right|up|down) --mode (outwards|inwards)";
    if raw.len() != 4 || raw[0] != "--direction" || raw[2] != "--mode" {
        return Err(usage.to_owned());
    }
    if raw[1] != parsed.direction || raw[3] != parsed.mode {
        return Err("error: argv/parsed resize mismatch (impossible)".to_owned());
    }
    Ok(())
}

/// Versioned exact-owner resize request body. `creation`/`pid`/`exe_path`/
/// `user_sid`/`session_id` bind the exact ledger owner; `direction`/`mode`
/// are the validated wire tokens; `correlation` is a client-generated opaque
/// token the owner echoes in its dispatch log. No titles, no geometry, no
/// HWNDs, no secrets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResizeRequest {
    pub v: u32,
    pub creation: String,
    pub pid: u32,
    pub exe_path: String,
    pub user_sid: String,
    pub session_id: u32,
    pub direction: String,
    pub mode: String,
    pub correlation: String,
}

/// Render one resize request body. Bounds only; full owner equality stays
/// with the owner-side check.
pub fn render_resize_request(request: &ResizeRequest) -> String {
    serde_json::to_string(request).unwrap_or_default()
}

/// Parse and bound one resize request body. Refuses malformed JSON, version
/// drift, empty identity, unknown direction/mode tokens, and invalid
/// correlation tokens. Owner equality (exact creation/pid/exe/sid/session)
/// stays with the caller, which holds the live owner identity.
pub fn parse_resize_request(json: &str) -> Result<ResizeRequest, String> {
    let request: ResizeRequest =
        serde_json::from_str(json).map_err(|_| "refuse: malformed resize request".to_owned())?;
    if request.v != RESIZE_REQUEST_VERSION {
        return Err("refuse: resize request version".to_owned());
    }
    if request.creation.is_empty()
        || request.pid == 0
        || request.exe_path.is_empty()
        || request.user_sid.is_empty()
        || request.correlation.is_empty()
    {
        return Err("refuse: malformed resize request".to_owned());
    }
    if !resize_direction_valid(&request.direction) {
        return Err("refuse: malformed resize request".to_owned());
    }
    if !resize_mode_valid(&request.mode) {
        return Err("refuse: malformed resize request".to_owned());
    }
    if tiler_core::ids::CorrelationId::parse(&request.correlation).is_none() {
        return Err("refuse: malformed resize request".to_owned());
    }
    Ok(request)
}

/// Central portable borderless-fullscreen predicate: a captionless window
/// whose visible frame completely contains any monitor's full bounds
/// (`rcMonitor`, not `rcWork`). Dimensions alone never classify: captioned
/// windows are never fullscreen, and small borderless windows stay eligible.
/// Negative coordinates are handled by exact containment, not size checks.
#[must_use]
pub fn is_borderless_fullscreen(captionless: bool, visible: Rect, monitor_fulls: &[Rect]) -> bool {
    if !captionless {
        return false;
    }
    monitor_fulls
        .iter()
        .any(|full| tiler_core::bounds::rect_contained(*full, visible))
}

/// Stable tick-summary signature for log dedupe. Carries no tick: an
/// identical observation (window count, applied writes, sorted skip reasons,
/// mismatches, op, Engine revision) stays quiet no matter how many ticks pass.
/// Bounded: the caller passes already-sorted skip pairs; no arbitrary caps.
#[must_use]
pub fn tick_summary_signature(
    windows: usize,
    applied: usize,
    sorted_skipped: &[(String, String)],
    mismatched: usize,
    op: &str,
    revision: u64,
) -> String {
    let skips = sorted_skipped
        .iter()
        .map(|(t, r)| format!("{t}={r}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{windows}|{applied}|{skips}|{revision}|{mismatched}|{op}")
}
