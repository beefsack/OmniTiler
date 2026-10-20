# tiler-live-fixture

Development-only controllable live test windows. One `tiler-live-fixture`
process owns many native windows (winit + softbuffer CPU text); callers drive
them by caller id over versioned JSON-lines on a private local endpoint
(Unix socket on Linux/macOS, owner-restricted named pipe on Windows).

This crate is not product code and is never part of the shipping roots.
Existing helpers stay for the proofs explicitly bound to them until ported:
the Windows `tiler-test-window` binary (non-hooking lifecycle `run --hide`
proof and the documented private cloak proof) and the
macOS probe target (bound live legs). New agent live tests use this fixture;
see the fixture-first rules in the
[KDE](../../docs/live-kwin-testing.md#fixture-first-live-tests),
[Windows](../../docs/live-windows-testing.md#fixture-first-live-tests) and
[macOS](../../docs/live-macos-testing.md#fixture-first-live-tests) guides.

## Smoke (two terminals, from the repo root after an env restart)

Run everything with `cargo run --locked -p tiler-live-fixture --` from the
repo root. Never assume a `target/` directory: `CARGO_TARGET_DIR` may
redirect build output.
Build without starting windows with `cargo build --locked -p tiler-live-fixture`.
On macOS/Windows prefix Cargo commands with `mise exec --`; Linux uses devenv.

```sh
# Terminal 1: start the server in a freshly made private run dir.
RUN_DIR="$(mktemp -d "${XDG_RUNTIME_DIR:-/tmp}/tiler-live-fixture-XXXXXX")"
chmod 700 "$RUN_DIR"
cargo run --locked -p tiler-live-fixture -- serve --dir "$RUN_DIR"
# Prints exactly one JSON line (the only stdout line, the only nonce source):
# {"socket":"$RUN_DIR/fixture.sock","pid":1234,"nonce":"<64 hex>"}
# On Windows the same announcement carries a pipe name
# The JSON-escaped path starts with "\\\\.\\pipe\\".
# `send --socket` accepts the announced value verbatim on both transports.
# On Windows `--dir` is lifecycle/naming compat only (created/removed with
# the run): it is never a filesystem security boundary. The pipe DACL
# (current-user SID plus `SYSTEM`, no remote clients) plus the per-run nonce
# are the Windows security boundary.
```

```sh
# Terminal 2: paste the two announced values when prompted (no polling,
# no shared file race), then drive the windows. All commands run from the
# repo root.
printf 'socket: '; read -r SOCK
printf 'nonce: '; read -r NONCE
run() { cargo run --locked -p tiler-live-fixture -- send --socket "$SOCK" --nonce "$NONCE" "$@"; }
run '{"v":1,"request_id":"spawn1","op":"spawn","windows":[{"id":"a","size_px":[640,480],"description":"smoke"},{"id":"b","resizable":false,"x11_window_type":"dialog"}]}'
run '{"v":1,"request_id":"get1","op":"get","ids":["a","b"]}'
run '{"v":1,"request_id":"geometry","op":"set","ids":["a"],"patch":{"size_px":[800,600],"min_size_px":[320,240],"max_size_px":[1200,900],"position_outer_px":[100,100],"description":"updated geometry and hints"}}'
run '{"v":1,"request_id":"maximize","op":"set","ids":["a"],"patch":{"mode":"maximized"}}'
run '{"v":1,"request_id":"windowed","op":"set","ids":["a"],"patch":{"mode":"windowed"}}'
run '{"v":1,"request_id":"set1","op":"set","ids":["a"],"patch":{"decorations":false}}'
run '{"v":1,"request_id":"set2","op":"set","ids":["a"],"patch":{"decorations":true,"mode":"borderless_fullscreen"}}'
run '{"v":1,"request_id":"set3","op":"set","ids":["a"],"patch":{"mode":"windowed"}}'
run '{"v":1,"request_id":"list1","op":"list"}'
run '{"v":1,"request_id":"batch1","op":"batch","items":[{"op":"spawn","windows":[{"id":"c"}]},{"op":"set","ids":["c"],"patch":{"size_px":[800,600]}},{"op":"get","ids":["a","c"]}]}'
# Subscription plus the mutations it observes in ONE connection: subscribe,
# spawn/close window "d" (one created + one closed event), unsubscribe and a
# final list on the same connection, expecting exactly those 2 events.
# A separate invocation would be a separate connection, where unsubscribe
# tests nothing.
run --events 2 \
  '{"v":1,"request_id":"sub1","op":"subscribe","events":["created","closed"]}' \
  '{"v":1,"request_id":"spawn2","op":"spawn","windows":[{"id":"d"}]}' \
  '{"v":1,"request_id":"close2","op":"close","ids":["d"]}' \
  '{"v":1,"request_id":"unsub1","op":"unsubscribe"}' \
  '{"v":1,"request_id":"list2","op":"list"}'
run '{"v":1,"request_id":"minimize","op":"set","ids":["b"],"patch":{"mode":"minimized"}}'
run '{"v":1,"request_id":"restore","op":"set","ids":["b"],"patch":{"mode":"windowed"}}'
run '{"v":1,"request_id":"close3","op":"close","ids":["b","c"]}'
run '{"v":1,"request_id":"bye","op":"shutdown"}'
# Terminal 1 exits 0 after closing its windows and removing its own socket.
# Terminal 1: remove the now-empty directory created by mktemp.
# rmdir "$RUN_DIR"
```

### Windows PowerShell start

```powershell
# Terminal 1: start the server (default run dir; no --dir needed).
mise exec -- cargo run --locked -p tiler-live-fixture -- serve
# Terminal 2: paste the exact announcement from Terminal 1, then drive windows.
$ann = (Read-Host 'Paste announcement JSON') | ConvertFrom-Json
# Single-quote each JSON payload:
function Send-Fixture([string]$Json) { mise exec -- cargo run --locked -p tiler-live-fixture -- send --socket $ann.socket --nonce $ann.nonce $Json }
Send-Fixture '{"v":1,"request_id":"spawn1","op":"spawn","windows":[{"id":"a","size_px":[640,480]}]}'
Send-Fixture '{"v":1,"request_id":"get1","op":"get","ids":["a"]}'
Send-Fixture '{"v":1,"request_id":"bye","op":"shutdown"}'
# The pipe endpoint lives in the kernel namespace: no socket file exists.
# With the default run dir there is nothing to delete; with an explicit
# --dir, remove only that run dir afterwards.
```

### Windows live evidence (2026-10-20)

Standalone and fixture-only integrated checks passed on native Windows 11
across two outputs with different scale factors. Fresh native HWND/PID joins,
PMv2 rectangles, DWM frames, visibility and zoom/minimize state corroborated
fixture replies. Runtime changes retained the same HWNDs.

| Operation / field | Observed result |
| --- | --- |
| `spawn`, `get`, `list`, ordered `batch` | Passed with multiple windows on both outputs |
| `subscribe`, `unsubscribe` | One connection received exactly one created and one closed event, then completed its final list |
| Decorations off/on, resizable/fixed, size | Runtime changes passed on both outputs without recreation |
| Min/max size | Accepted on both outputs; native `WM_GETMINMAXINFO` confirmed track limits on one output. Programmatic sizes outside the hints were still honored; these are not resize clamps |
| Maximize/windowed, minimize/restore | Passed on both outputs, independently confirmed by native state and frames |
| Borderless fullscreen/windowed | Native frame covered each monitor and restored; fullscreen `observed.mode` remains unknown (`null`) under the current non-Wayland readback policy |
| Hide/show, focus | Fixture observations agreed with native visibility and foreground HWND |
| Level, cursor, buttons, increments, theme, transparency, attention | Accepted hints/actions; no visual, input-passthrough or user-resize enforcement claim |
| X11-only type/override-redirect, `blur:true` | Unsupported, not Windows fixture defects |
| Compact output / observation deadlines | Resize matched; intentional mismatch timed out with nonzero exit |
| `close`, `shutdown`, exact-process forced exit | Windows and server cleaned; graceful shutdown exited 0 and forced exit removed the named pipe endpoint |

Geometry rules for Windows checks:

- Compare physical coordinates from a PMv2-aware observer. DPI-unaware monitor
  metrics can be virtualized while another rectangle is physical; this can
  falsely suggest oversized fullscreen windows. Distinguish client size, outer
  bounds and DWM visible frames (including invisible resize borders).
- A cross-DPI move can preserve logical size and change physical client size.
  Place the window on the target output, then set `size_px` and wait for the
  observed physical size when exact staging matters.
- Runtime decoration toggles changed client/DWM frame geometry reversibly
  without changing `GWL_STYLE` bits. Style bits alone do not prove decorations.
- Native enumeration includes auxiliary winit/IME windows in the fixture
  process. Join caller IDs and the run-tagged sentinel titles to HWNDs rather
  than treating every window of the PID as a fixture test window.

Integrated checks used `--scope-exe tiler-live-fixture.exe`. Four fixture
windows tiled across both outputs; exact-owner directional focus and move
worked on each output, cross-output send/follow returned the subject, and
workspace send/select roundtrips hid and revealed members on each output.
Serial CLI dispatches were joined to production outcomes and independent
frames/visibility/foreground, not treated as completion on queue acceptance.
The managed-token roster joined to fixture HWNDs; protected window geometry
stayed unchanged. Stop/restore released the hook, preserved settings and left
no owner, fixture process, recovery ledger or pending control requests.

Native fixture tests (119), strict all-target clippy and workspace fmt passed.
No fixture code defect was found. Exclusive fullscreen/display-mode changes
and physical input acceptance were not exercised. See the
[first-use record](../../docs/changes/archive/windows-live-fixture-first-use.md).

### macOS bundle launch and scope

Standalone Cargo/direct-executable launch works, but has no LaunchServices
bundle identity and cannot match a bundle-scoped tiler run. For scoped tests,
build the development-only, regular-policy fixture bundle instead:

```sh
mise exec -- just fixture-app 'OmniTiler Mac Dev'
mise exec -- just fixture-verify
```

Use the existing persistent development signing identity (replace the example
name if needed); the builder refuses ad-hoc fallback, builds locked/offline,
honors `CARGO_TARGET_DIR` for the source binary, and assembles the fixed output
`target/macos/OmniTilerLiveFixture.app`. It targets macOS 15 on Apple Silicon.
No permission grants or settings changes are performed.

Launch a fresh instance through LaunchServices, not its inner executable.
The following is a template: replace each `/ABSOLUTE/PRIVATE/RUN` with the
same newly created, owned `0700` directory and `/ABSOLUTE/REPO` with the repo
root. The socket path must fit the platform's Unix-socket path limit.

```sh
open -n --stdout /ABSOLUTE/PRIVATE/RUN/announcement.json --stderr /ABSOLUTE/PRIVATE/RUN/stderr.log /ABSOLUTE/REPO/target/macos/OmniTilerLiveFixture.app --args serve --dir /ABSOLUTE/PRIVATE/RUN
```

Read the single announcement JSON line for `socket`, `pid` and `nonce`, then
use the same `send` protocol above (prefix Cargo with `mise exec --` and add
`--offline`). Bind ownership to that announcement, native executable and
launch lifetime, and a fresh CG/AX window join before effects. After
`shutdown`, verify process/window/socket absence; remove only the owned log
files and now-empty private directory. `open` does not report the fixture's
exit status: verify teardown rather than treating its success as server success.

The bundle ID is **`io.github.omnitiler.live-fixture`**. Keep regular activation
policy: `LSUIElement=true` makes the current classifier exclude fixture windows
as `AccessoryPolicy`. Read-only `scope-inspect --scope
io.github.omnitiler.live-fixture` verified a real bundle/launch date and
`AXWindow`/`AXStandardWindow` windows classified `Tile`, including a
borderless/non-resizable window. Future integrated tests must explicitly
scope this bundle and follow the [macOS live guide](../../docs/live-macos-testing.md#fixture-first-live-tests).
Integrated coverage is listed below.

### macOS standalone evidence (2026-10-20)

Native desktop checks used CG owner-PID/window joins, fixture-only AX window
IDs/roles/frames and LaunchServices identity, not socket replies alone.

| Operation / field | Observed result |
| --- | --- |
| `spawn`, `get`, `list`, ordered `batch` | Passed; multiple native windows and independent AX/CG joins |
| `set` geometry, min/max, title/description | Accepted; resize wait matched, native frames corroborated geometry; min/max enforcement and rendered text not independently proved |
| `subscribe`, `unsubscribe` | Same connection received exactly one created and one closed event; final list reply completed |
| Decorations, non-resizable windows | Creation and runtime decoration toggles plus subsequent socket dispatch passed after the readback fix below |
| Focus | `focus:true` accepted, fixture focused readback true and native application active; physical focus/shortcut behavior not tested |
| Hide/show, minimize/restore, maximize/windowed | Passed; native visibility/minimized/frame changes observed |
| Level, cursor, buttons, increments, theme, transparency, blur | Accepted hints; no visual, input-passthrough or enforcement claim (CG topmost layer observed) |
| X11-only type/override-redirect | Unsupported on macOS, without fake native effects |
| `close` | Removed owned windows without exiting server; empty list then respawn worked |
| `shutdown` | Direct child exited 0; bundled process exited; owned windows/socket cleaned |
| Compact output / observation deadlines | Resize matched; deliberate mismatch timed out with nonzero exit |

Fixture sizes/positions are physical pixels; CG/AX frames are logical points
and include outer chrome. Convert by `scale_factor`, distinguish inner size
from outer bounds, and allow for native decoration/transition differences.

macOS winit 0.30.13's `is_maximized()` temporarily changes style masks on
borderless or non-resizable windows. Calling it from geometry feedback caused
an event storm and socket starvation. The fixture now skips that getter for
those windows: `observed.mode` is unknown (`null`) unless minimized, never a
cached request presented as windowed/maximized. Normal decorated/resizable
windows retain zoom readback. Fullscreen modes were not exercised: native
fullscreen can create a Space and exclusive mode can change the display mode.

Offline crate gates passed: 121 tests, strict all-target clippy and fmt.
The two formerly failing oversized-frame tests now keep their writer alive
until the reader finishes; macOS timeout changes after peer closure returned
`EINVAL` and masked `TooLong`. The reader's size-cap assertions are unchanged.
Protected-window geometry stayed unchanged and owned live resources were
cleaned. Minimum-OS runtime coverage remains a separate check.

### macOS with OmniTiler active (2026-10-20)

After the live guide's preflight and ownership gates, launch the signed fixture
bundle above, then use the bounded, exact-scope tiler route:

```sh
mise exec -- just tile --user-start --scope io.github.omnitiler.live-fixture --seconds 90 --input tap
```

Native desktop integration passed with the tap running. Source revision,
artifact hashes/signatures, process lifetimes and scoped raw readbacks were
retained outside the repository. Fixture caller IDs were joined through their
native sentinel titles to CG/AX window IDs before comparing geometry.

| Check | Observed result |
| --- | --- |
| Admission and layout | One full tile, two side-by-side tiles, then three tiles (one full-height plus two stacked); all standard windows classified `Tile` |
| Socket versus native geometry | Physical outer position divided by scale matched CG/AX position; content width matched outer width, and the measured content-to-outer height difference was 32 logical points for these decorated windows, not a universal constant |
| Socket focus on two windows | `observed.focused:true`, exact native `AXFocusedWindow` ID and active fixture PID agreed |
| Socket resize while tiled | Readbacks reconverged to the prior tiled layout; the transient client size was not captured, so no claim about its visible duration |
| Close/reflow | Three to two to one windows re-tiled; full socket, AX and CG readbacks agreed after each close |
| Tap and shutdown | Completed with `shutdown_clean:true`, clean joins and no retained workers, input refusals or disable events; zero chord deliveries, so lifecycle only, not hotkey-effect proof |
| Teardown | Owned fixture/tiler processes, windows, sockets and awake helpers removed; protected-window records, display topology and native Space row/current matched their preimages |

One transient backend observation failed, then a later snapshot published
successfully; no topology failure or shutdown shortfall was reported.

AppKit can retain auxiliary offscreen CG entries that are not caller windows.
Count and compare the fixture's joined AX windows, not every process-owned CG
entry. Tiler stop leaves fixture geometry in place; shut down owned fixture
windows afterwards rather than assuming stop restores them.

Remaining input/workspace gap: `owned-post`/`owned-focus` authenticate legacy
probe/helper spawn records, not fixture socket announcements or this bundle.
The stay/trailing diagnostic injections accept only a TextEdit scope. No such
guard was bypassed and no native Space/settings mutation was exercised here.
Product focus/move chords and managed select/send/trailing therefore remain
unverified with this fixture; use physical keys on identified fixture windows,
or port the guarded diagnostic harness with an explicit fixture ownership
contract before automating them. Socket focus/resize is not evidence for those
product actions. See the [integration record](../../docs/changes/archive/macos-fixture-tiler-integration.md).

Expected backend differences (not failures):

- On X11, `"x11_window_type":"dialog"` creates a dialog-typed window
  (useful for tiler float/classification checks). On Wayland the same spawn
  reports `"x11_window_type"` unsupported and creates a normal window.
- `mode:borderless_fullscreen` is the portable fullscreen smoke (works on
  both backends where the compositor honors it). `mode:exclusive_fullscreen`
  additionally needs a real display video mode and is reported `unsupported`
  on Wayland and headless runs instead of faked.
- Wayland reports `position_outer_px`, restore-from-minimize, and `visible`
  as `unsupported`, and sizes are negotiated with the compositor (observed
  may differ from requested; compare both fields, do not assume equality).
  A set X11 fullscreen stays null in `observed` (setter cache, not a
  readback) while `configured` keeps the sent value.
- Backend selection is the standard winit rule, not an env override:
  Wayland wins when `WAYLAND_DISPLAY`/`WAYLAND_SOCKET` is set, otherwise
  X11 when `DISPLAY` is set. For the X11 run on a KDE session that also
  exports Wayland variables: `env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET
  cargo run --locked -p tiler-live-fixture -- serve --dir "$RUN_DIR"`.
- Host chrome only: normal decorations, title and borders. No custom hooks.

`send` also reads JSON lines from stdin when no payload arguments are given,
and injects `--nonce` into requests that omit it.

Client waiting rules (`send --events N`, `send --follow`):

- Replies are matched by `request_id`; events interleave freely, so a
  single connection can subscribe and then mutate (as the smoke does).
- `--events N` counts real events only, never the `subscribed` handshake,
  even when they arrive before all replies. `--events 0` returns as soon
  as the replies are in and never hangs.
- Without `--follow`, reads have a 30 s request/response deadline so a
  wedged server cannot hang scripts forever. `--follow` is the deliberate
  indefinite subscription: no deadline, ends with Ctrl-C or server EOF.

### Compact output and observation waits

`send --compact` omits null-valued optional snapshot fields in printed JSON
only. Missing fields in this view mean unknown/unset, not false. Errors,
per-field outcomes, `unsupported`, `settled`, and the requested/configured/
observed distinction remain. Wire snapshots and in-window text stay full.

```sh
run --compact '{"v":1,"request_id":"brief","op":"get","ids":["a"]}'
run --compact --wait-ms 3000 --expect '{"a":{"size_px":[800,600]}}' \
  '{"v":1,"request_id":"resize-wait","op":"set","ids":["a"],"patch":{"size_px":[800,600]}}'
```

- `--expect` maps caller IDs to nonempty **observed** field subsets:
  `size_px`, `position_outer_px`, `mode`, `visible`, `focused`,
  `scale_factor`, `logical_size_px`. Unknown/null never matches. Unsupported
  readbacks can therefore reach the deadline even after an accepted setter.
- `--wait-ms` and `--expect` are required together. A positive caller-chosen
  deadline bounds the whole socket exchange: hosts can clamp/ignore requests
  or withhold state, so scripts must not hang. No fixed sleep or read polling:
  the client subscribes before mutations, gets one baseline afterward, then
  waits for state events using the remaining absolute deadline.
- Completion requires all request replies and the latest observations to
  match. This is a point-in-time observation, not a promise of future stability.
  Final JSON reports `wait:matched`, `wait:timeout` or `wait:failed`, elapsed
  time and latest snapshots. Timeout/failure exits nonzero. Closed/missing
  windows, failed replies, errors and premature EOF fail promptly.
- Payload request IDs must be unique and explicit. Internal subscription/get
  replies are hidden. Waiting cannot combine with `--events`, `--follow`,
  or payload `subscribe`/`unsubscribe`/`shutdown` (the waiter owns its connection's
  subscription). Ordered non-atomic window batches remain available.

If `serve` is killed with a signal instead of `shutdown`, only remove the
exact owned resources (never a broad directory remove):

```sh
rm -f "$RUN_DIR/fixture.sock"
rmdir "$RUN_DIR"  # only when you created this run dir; never touch preexisting dirs
```

## Command reference

### `serve [--dir PATH] [--socket-name NAME]`

Starts the fixture server on the calling thread's winit event loop and
prints exactly one stdout line: `{"socket":...,"nonce":...,"pid":...}`.
All other output goes to stderr and never carries the nonce.

| Flag | Type | Default | Notes |
| --- | --- | --- | --- |
| `--dir` | path | `$XDG_RUNTIME_DIR/tiler-live-fixture-<pid>-<rand>`, else `$TMP/tiler-live-fixture-<pid>-<rand>` | Created non-recursively (parents must exist). On Unix must be an owned private real dir (mode `0700`, symlinks refused, never chmodded); on Windows lifecycle/naming only. |
| `--socket-name` | plain file name | `fixture.sock` | No separators or NUL. On Windows becomes the trailing component of the pipe name. |

Exit status is `2` on CLI misuse, `1` on bind/entropy/event-loop failure,
`0` after a clean `shutdown`.

### `send --socket PATH --nonce NONCE [mode flags] [JSON...]`

One client connection: forwards JSON lines from arguments (or stdin when no
arguments are given) and prints reply/event lines.

| Flag | Type | Default | Notes |
| --- | --- | --- | --- |
| `--socket` | path (required) | - | Unix socket path, or verbatim Windows pipe name from the announcement. |
| `--nonce` | string (required) | - | Per-run secret from the announcement. Injected into payloads that omit `nonce`; payloads carrying their own `nonce` keep it. |
| `--events N` | u64 | off | Complete after all replies plus N real events. `0` returns with the replies. |
| `--follow` | switch | off | Indefinite subscription, no read deadline. Ends on Ctrl-C or server EOF. |
| `--compact` | switch | off | Strip null-valued optional snapshot fields from printed JSON only. |
| `--wait-ms N` | u64, positive | off | Absolute-deadline observation wait; requires `--expect`, forbids `--events`/`--follow` and lifecycle payloads. |
| `--expect JSON` | object | off | `{id: {observed-field: value, ...}}`; every id needs a nonempty subset of the observed fields listed above. |

Exit status is `2` on CLI misuse, `1` on transport/wait failure (`wait:timeout`
and `wait:failed` both exit nonzero), `0` when the completion rule is met.

## Protocol

- One JSON object per line, max 1 MiB per frame (oversize frames close the
  connection). Every request carries `v: 1`, `request_id`, `nonce`, `op`.
- Caller ids match `[A-Za-z0-9_.-]+`, 1..=64 bytes. `request_id` is 1..=128
  bytes. Unknown op/field names are rejected, including inside `batch` items.
- Replies echo `request_id` (`ok` plus `result`, or `ok:false` plus `error`).
  ID-list ops return per-window entries with `ok`, `error`, `snapshot` and
  per-field `fields` outcomes (`accepted` or `unsupported`, optional `detail`).
- Snapshots carry `requested` (caller intent, may hold unsupported values)
  beside `observed` (unknown is null, never a request echo), `configured`
  (only values the host actually took where no trustworthy readback exists:
  explicitly a cache, never refused values), `unsupported`, and `settled`.
- `settled` is geometry feedback only: accepted size/position changes make it
  false; move/resize/scale feedback, or a fresh matching geometry `get`, makes
  it true. Mode/hint-only changes do not make it pending. A settled window
  can still differ from requested geometry; prefer observed predicates.
- Accepted means the host took the request, never that it settled on the
  value. Unsupported actions are reported per field, never faked.
- Subscriptions are connection-scoped: a repeat `subscribe` from the same
  connection replaces the filter (never duplicates events), `unsubscribe`
  removes that connection's entry, and a disconnect forgets it (plus lazy
  cleanup when a broadcast fails). Filter names are `created`, `state`,
  `closed`, `error`, `all`. An empty `events` filter is rejected;
  omit `events` for `["all"]`. The `subscribed` handshake marker is not an event.
- Event envelopes are `{"event":"created"|"state","id":...,"snapshot":...}`,
  `{"event":"closed","id":...}` or
  `{"event":"error","id":null|"id","message":...}`. State events may repeat
  unchanged observations and carry no exclusive request-causality claim.
- Closing windows never exits the server, even when none remain. Only
  `shutdown` (or killing the process) ends the run.

### Ops and required fields

| Op | Required fields | Notes |
| --- | --- | --- |
| `spawn` | `windows`: nonempty list of `{id, ...fields}` | Many windows in one request. Defaults fill `mode:windowed`, `visible:true`, `x11_window_type:normal`. |
| `set` | `ids`: nonempty id list; `patch`: field subset | `ids` may name unknown windows; those entries fail per-window. A merged min/max contradiction is rejected before reaching the host. |
| `get` | `ids`: nonempty id list | Returns snapshots. Unknown ids fail per-window. |
| `close` | `ids`: nonempty id list | Unknown ids fail per-window. |
| `list` | - | Returns caller ids only; use `get` for snapshots. |
| `batch` | `items`: nonempty list | Ordered, non-atomic. Items accept only `spawn`/`set`/`get`/`close`/`list` with the same per-item required fields; `subscribe`/`unsubscribe`/`shutdown`/nested `batch` are rejected per item. |
| `subscribe` | `events`: optional filter list | See filter names above. |
| `unsubscribe` | - | Removes this connection's subscription. |
| `shutdown` | - | Closes windows, removes the owned socket, exits 0. |

## Settable fields, types, defaults and backend limits

Every field below works with `spawn`/`set` unless marked creation-only.
These are requests/hints, not proof of compositor effects. Values without
trustworthy readback live in `configured`, never in `observed`. `focus` and
`attention` are one-shot actions: reported per field, never persisted in
`requested`/`configured`, never replayed by unrelated sets. A `set` patch
touches only the fields it names: omitting a field leaves the stored value
unchanged (merging overlays `Some` fields only; unrelated sets never replay
one-shot actions). JSON `null` also means unset/no change, not reset: min/max
constraints cannot be cleared through this protocol. Refused persistent
intent stays in `requested`; its `unsupported` tag survives unrelated changes
until that field is accepted again.

Spawn-request defaults are injected into `requested` for three fields only:
`mode:windowed`, `visible:true`, `x11_window_type:normal`. Every other unset
field stays absent in `requested`, and creation applies only the fields that
are present on top of winit `WindowAttributes::default()` (`resizable:true`,
`decorations:true`, `visible:true`, `window_level:Normal`,
`enabled_buttons:all`, `transparent:false`, `blur:false`, no size/min/max/
position/increments/theme). The "Unset" column gives `requested` then
effective creation behavior.

| Field | Wire type / bounds | Unset: requested -> effective |
| --- | --- | --- |
| `size_px` | `[u32,u32]`, each 1..=u32::MAX | absent -> no `with_inner_size`; winit `inner_size:None` takes the backend default size. (The 640x480 fallback exists only in the `FakeHost` test double.) Observed reports the real size. Wayland negotiates; X11 enforces min/max on programmatic resize. |
| `min_size_px` / `max_size_px` | `[u32,u32]`, each 1..=u32::MAX; merged min<=max per axis enforced, including merged with stored values on `set` | absent -> no constraints. X11 enforces; Wayland accepts out-of-hint requests, so hints are not a portable clamp. |
| `resizable` | bool | absent -> winit default `true`. Governs user resizing, not every programmatic change. |
| `decorations` | bool | absent -> winit default `true`. Runtime-toggleable where supported; no cross-backend readback (cache only). |
| `mode` | `windowed` (requested default) / `maximized` / `minimized` / `borderless_fullscreen` / `exclusive_fullscreen` | Default creates windowed; `minimized` requests minimize immediately after creation. `maximized` sets the maximized attr; borderless sets borderless fullscreen; exclusive takes the first enumerated video mode or reports `unsupported`. Wayland: no restore-from-minimize; exclusive unsupported. X11: set fullscreen stays null in `observed` (setter cache), `configured` keeps the sent value. Exclusive can change the display mode; not exercised in desktop-preserving checks. |
| `position_outer_px` | `[i32,i32]`, any 32-bit signed pair | absent -> backend places the window. Wayland unsupported; X11 readback can differ by a decoration offset. |
| `visible` | bool (requested default `true`) | `true` applies `with_visible(true)` where supported; Wayland reports `show`/`hide` unsupported. Spawn `visible:false` creates hidden where supported. |
| `description` | string, max 65536 bytes | absent -> none. Local-only: stays in `requested`, never `configured`; rendered wrapped in-window. |
| `title` | string suffix, no `\n`/`\r`; length bounded by the 1 MiB frame | absent or empty -> no suffix. Appended to the server-controlled `sentinel/id/run-tag` prefix. No Linux title readback; `snapshot.title` is constructed, not observed. |
| `x11_window_type` | `normal` (requested default) / `dialog` / `utility`, creation-only | X11 creation attr; Wayland accepts `normal` as a no-op and reports other values unsupported. Runtime changes report unsupported, never recreate. |
| `override_redirect` | bool, creation-only | absent -> winit default `false` (managed window). `true` off X11 reports unsupported; runtime changes never recreate. |
| `window_level` | `always_on_top` / `normal` / `always_on_bottom` | absent -> winit default `Normal`. X11/Windows/macOS hint; Wayland non-normal unsupported, normal accepted as default no-op. |
| `focus` | bool, one-shot, never stored | absent -> no request. `false` does nothing; `true` requests raise/focus where supported. Wayland `focus_window` is a winit no-op, so true reports unsupported; use real `observed.focused`. |
| `attention` | `informational` / `critical` / `clear`, one-shot, never stored | absent -> no request. X11 urgency collapses severity; clear clears the WM_HINTS urgency bit but KWin keeps `_NET_WM_STATE_DEMANDS_ATTENTION` on an inactive window until it is activated (accepted is not proof of withdrawal); Wayland needs activation protocol/context and cannot clear; macOS clear is a winit no-op. |
| `cursor_visible` | bool | absent -> winit default cursor (visible). No readback; no capture, grab or global pointer move. |
| `cursor_hittest` | bool; `false` makes content input-passthrough | absent -> default hit-testing. Safe winit result reported per field; host refusal is unsupported. |
| `enabled_buttons` | array of `close` / `minimize` / `maximize`; empty disables all | absent -> winit `WindowButtons::all()`. X11/Wayland winit setters unimplemented: unsupported. |
| `resize_increments_px` | `[u32,u32]`, each 1..=u32::MAX | absent -> no increments. Wayland unsupported; no portable enforcement/readback claim. |
| `theme` | `system` / `light` / `dark` (`system` resets to the host default) | absent -> winit `None` (host default). Host/decoration hint; no cross-backend theme observation. |
| `transparent` | bool | absent -> winit default `false`. Creation-only on X11; host hint only: software content remains opaque, not an alpha-rendering test. |
| `blur` | bool | absent -> winit default `false`. Wayland requires KDE blur protocol; macOS supported; X11/Windows `true` unsupported, `false` default no-op. |

```sh
run '{"v":1,"request_id":"level","op":"set","ids":["a"],"patch":{"window_level":"always_on_top","title":"topmost case"}}'
run '{"v":1,"request_id":"attention","op":"set","ids":["a"],"patch":{"attention":"informational"}}'
run --wait-ms 3000 --expect '{"a":{"focused":true}}' \
  '{"v":1,"request_id":"activate","op":"set","ids":["a"],"patch":{"focus":true}}'
run '{"v":1,"request_id":"normal","op":"set","ids":["a"],"patch":{"window_level":"normal","attention":"clear"}}'
```

Wayland has no client stacking query; winit exposes no native level/stacking
getter on any backend. Z-order query is **unsupported**, including X11 here:
an additional connection/property adapter is not needed for the set-only
fixture. No requested level is presented as observed stacking. Wayland
`request_user_attention` may use activation but cannot promise focus without
compositor support and valid activation-token/input context.

### API sweep exclusions and platform follow-ups (unchanged)

- X11 visual/screen selection and embedding: native-handle/renderer plumbing,
  not needed for owned tiler scenarios. Base-size hint adds little beyond the
  supplied min/max and increments.
- Wayland layer-shell is a different role, not a cheap winit toplevel option.
  Activation-token plumbing requires a real input context, not a manufactured
  background focus request. CSD theme remains only a preference.
- Icons/custom cursors, raw handles/graphics surfaces, IME, touch/gesture input,
  device capture/grab/warp, drag-resize/window menus: rendering/input-toolkit
  concerns or require physical input, not background tiler-test controls.
- Content protection: Linux winit no-op, not a useful Linux test witness.
- Windows follow-ups: owner/menu/taskbar identity, skip-taskbar, enable/disable,
  shadow/backdrop/corner/border/title styles. They need native compile/live
  coverage rather than a speculative Linux-only extension surface.
- macOS follow-ups: titlebar/full-size-content variants, shadow, first-mouse,
  tabbing/document-edited, simple fullscreen/Space behavior and borderless-game
  policy. They need native live coverage. Common controls use safe
  winit APIs; CI transport checks remain headless. Native macOS standalone
  coverage is listed above.
- Change history, elevated Windows instances and exclusive-mode selection stay
  deferred. Existing exclusive fullscreen still risks display-mode changes.

### KDE isolation evidence

Native Wayland and forced X11/XWayland were exercised without OmniTiler.
Raw replies and owned-window X11 properties were retained outside the repo;
accepted setters alone do not establish visible effects.

| Feature | Wayland | X11/XWayland |
| --- | --- | --- |
| Level | Non-normal unsupported | ABOVE/BELOW set; normal cleared both native hints |
| Focus | Request unsupported; focused state readable | Owned-window focus wait matched real readback |
| Attention | Info/critical forwarded, host-limited; clear unsupported | Urgency bit set and cleared natively; KWin `_NET_WM_STATE_DEMANDS_ATTENTION` persists on the inactive window until activation |
| Title suffix | Constructed title only, native readback unavailable | Sentinel title/class verified natively |
| Cursor visibility/passthrough | Accepted, no independent effect/readback proof | Same |
| Buttons / increments | Both unsupported | Buttons unsupported; native resize-increment hint verified |
| Theme / transparency / blur | Accepted hints, no visual/readback proof | Native dark-theme hint; transparency creation accepted/runtime refused; blur true unsupported |
| Override-redirect | True unsupported | Creation accepted, identity and close verified; redirect attribute unverified |
| Compact / resize wait / mismatch deadline | Passed; matched resize, timeout exited 1 | Same |

No visual alpha/blur/passthrough, resize-increment enforcement or foreign-window
restoration claim is made. Owned servers/windows/sockets/run directories were
cleaned; controller status remained down and compositor config hash/mtime
unchanged. See the [expansion record](../../docs/changes/archive/live-fixture-api-expansion.md).

### KDE with OmniTiler active

Use `just dev-on --scope-class OmniTilerTestWindow` (and `just dev-off` for
teardown), following the [live guide](../../docs/live-kwin-testing.md) readiness
and restoration gates. Unscoped runs manage unrelated normal windows too.
Native Wayland and X11/XWayland integration exercised tiled admission/reflow,
fixed-size floats, native modes, decorations and backend-specific controls;
protected non-fixture geometry/state remained unchanged.

Fixture `request_id` and per-step journal cursors correlate intent with tiler
dispatches; `applied` is not native settlement. Use observed predicates for
completion. A client resize can match its predicate while leaving the tiler's
retained layout stale until the next subscribed host event. No request-correlated
history is implemented. See the [integration results and limits](../../docs/changes/archive/kde-dev-scope-fixture-integration.md).

## Window identity and rendering

- Titles retain `OmniTilerTestWindow:<id>:<run-tag>`; optional `title` text
  is appended after a space, never substituted for identity.
- X11 `WM_CLASS` and Wayland `app_id` are both `OmniTilerTestWindow`.
  `APP_ID` is Linux-only; the macOS LaunchServices bundle above supplies
  `io.github.omnitiler.live-fixture` separately (direct executable launch does not).
- Snapshots identify each window with `id` (caller id), `pid` (server pid),
  `backend` (`x11`, `wayland`, `windows` or `macos`), `native_handle`
  (debug handle description), `title` (constructed full string) and
  `scale_factor` where known.
- Each window renders a header plus its full snapshot as JSON (every
  exposed field, unknown is null) plus the independently settable
  `description` (wrapped; scroll with Up/Down, PageUp/PageDown, Home;
  scrolling is unbounded and saturating).
- Host chrome only: normal decorations, title and borders. No custom hooks.

## Platform-specific feature how-to

`platform.rs` holds the extension pattern: creation-only X11 window type
`normal`/`dialog`/`utility` via safe winit extension APIs, gated per
backend. A runtime change reports `unsupported` instead of recreating the
window. Follow this shape for every future action:

1. Add one catalog entry to `EXTENSION_ACTIONS` plus one `action_supported`
   gate for the backend it applies to; report unsupported everywhere else.
2. Keep host calls on the main (winit) thread behind the existing
   `Host`/`Capabilities` boundary in `win.rs`; never store raw handles beyond
   the owning window's lifetime.
3. New modules follow the repository unsafe rule (workspace forbids unsafe;
   only reviewed FFI roots allow it): prefer safe winit APIs; any raw OS
   call needs its own reviewed safe wrapper module, no plugin framework.
4. Security review per addition: validate inputs like `validate_id`, keep
   secrets out of logs/titles, keep the 1 MiB frame as the memory bound,
   and document observed-vs-configured-vs-unsupported exactly as this README
   does. Accepted is never proof of effect.

Worked example (shipped): `x11_window_type` maps the portable
`normal`/`dialog`/`utility` to `winit::platform::x11::WindowType` at creation
(`platform::winit_x11_types`), gated by `action_supported(_, "x11")`; the
same creation-only shape covers `override_redirect`.

Raw-window-handle recipe (not implemented): for host features winit lacks
on the fixture's own already-created windows, resolve the owned handle on
the main thread with `window.window_handle()` (`HasWindowHandle`, raw-window-
handle 0.6 via winit's `rwh_06`) and match `RawWindowHandle`: `Win32` carries
the `HWND` (`hwnd`), `AppKit` carries an `NSView` pointer (`ns_view`, so the
`NSWindow` must be derived on the main thread), `Xlib`/`Xcb` carry the X11
window id (`window`, plus `visual_id`). Wayland yields a `wl_surface` pointer
that cannot drive window-manager behavior: there is no protocol bypass, so a
Wayland raw-handle action reports unsupported. Never accept opaque foreign
handle strings over the wire: protocol ids name owned windows only, handles
are resolved from the live owned `Window` for the duration of one host call
and never persisted. A new action needs: a protocol field, a
`capabilities_for_backend` gate (unsupported elsewhere), dispatch inside the
`win.rs` host boundary, a per-field outcome, and an honest readback (fresh
native `observed`, or a `configured` cache entry when no trustworthy readback
exists) proven by a unit test plus a live readback leg. Raw host calls live
in one minimal reviewed safe-wrapped module under the repository unsafe rule
(workspace forbids unsafe; only reviewed FFI roots allow it); depending on
raw-window-handle directly (beyond winit's re-export) is a new dependency and
needs the same review.

Windows cloak recipe (not implemented): cloaking/uncloaking owned fixture
windows for hidden-membership tests would use the public DWM API:
`DwmSetWindowAttribute(HWND, DWMWA_CLOAK, ...)` to write and
`DwmGetWindowAttribute(HWND, DWMWA_CLOAKED, ...)` to read back (the same
public read the repo already uses in `tiler-windows` `ffi::window::cloaked`),
with per-field outcomes and mask-readback honesty: a cleared mask is not
proof of visibility restoration. No cloak action exists in this crate. The
legacy `tiler-test-window cloak HWND --tag TAG --seconds N --stop-file PATH
--log PATH` / `uncloak` / `cloak-observe` spike uses the private
`IApplicationView::SetCloak` pair, not the public DWM attribute; do not cite
it as fixture cloak coverage or as game-behavior evidence.

## Platform status

- Linux (KDE): fixture-driven live evidence as listed above; live use stays
  under the [KDE guide](../../docs/live-kwin-testing.md).
- Windows: native standalone and fixture-only integrated live checks passed
  on both outputs, with 119 fixture tests, strict clippy and fmt. See the
  Windows evidence above; keep `--scope-exe tiler-live-fixture.exe` per the
  [Windows guide](../../docs/live-windows-testing.md#fixture-first-live-tests).
  CI run 39419800653 passed fixture build/tests/clippy; its Windows and Linux
  Rust jobs still failed on preexisting `tiler-macos` compilation errors.
- macOS: native standalone socket/window smoke passed, with 121 offline
  tests and strict clippy/fmt. The signed regular-policy fixture bundle now
  supplies bundle identity and launch date; read-only scope inspection
  classified its standard windows `Tile`. Fixture-scoped live tiling with
  the tap passed one-to-three admission, socket/native geometry, exact native
  focus, close/reflow and clean teardown. Chord-driven and managed-workspace
  coverage remains open. Direct launch still lacks bundle identity.
  See the macOS evidence above and the
  [macOS guide](../../docs/live-macos-testing.md#fixture-first-live-tests).
- Linux Rust `tiler-macos` failures (`AX_ATTRIBUTE_UNSUPPORTED`,
  `take_trailing_proof`) predate these changes; no action here.

## Honest host limitations

- Wayland: no outer/global position, no restore-from-minimize, no
  show/hide (`visible` requests report unsupported), and exclusive
  fullscreen is reported unsupported. Sizes and hints are negotiated with the
  compositor; decorations/min/max may stay cached rather than observed.
- Readbacks never echo the request: on Wayland fullscreen reflects the
  compositor configure; on X11 the winit fullscreen getter is a setter
  cache (written on the request path, never from window-manager events),
  so a set fullscreen there stays null (unknown) in `observed` while
  `configured` keeps the sent value. Windows fullscreen also remains unknown
  under the non-Wayland readback policy, even when native monitor-sized frames
  confirm its effect. Maximized comes from its getter; minimized/visible are
  optional readbacks with unknown staying null
  (Wayland minimize state is always unknown). Decorations, resizable and
  min/max hints have no cross-backend readback (cache only).
- Hints are physical pixels throughout (correct on high DPI); a `set`
  whose merged min/max hints contradict is rejected before reaching the
  host. There are no arbitrary geometry or batch-size caps: the 1 MiB
  frame already bounds request memory.
- Exclusive fullscreen needs a real display video mode, otherwise it is
  reported unsupported. `resizable=false` governs user resizing, not every
  programmatic change.
- KDE isolation checks exercised native Wayland and XWayland. X11 enforced
  min/max bounds on programmatic resize; Wayland accepted requests outside the
  hints, so hints are not a portable size clamp. X11 outer-position readback
  also differed from the request by a decoration offset. Keep both values.
- X11 exclusive fullscreen automatically selects the first enumerated video
  mode, not necessarily the current one. It can change the display mode; it
  was not exercised in desktop-preserving isolation checks. Borderless
  fullscreen was exercised on both backends.
- Description is local-only: it stays in `requested`, never `configured`.
- Transport is a private local endpoint with the same JSON-lines framing,
  nonce, request/event and absolute-deadline semantics on both transports:
  Unix socket files (mode `0600`) on Linux/macOS and owner-restricted
  named pipes on Windows (protected DACL granting access to the literal
  current-user SID plus `SYSTEM`, remote clients refused, first instance
  exclusive so a second server cannot hijack the name).
  An existing `--dir` is accepted only when it is already an owned private
  real directory on Unix (mode `0700`, rejected symlinks, never chmodded);
  on Windows `--dir` is only created/removed for run lifecycle and never
  enforces filesystem permissions. Fresh run dirs are created non-recursively
  (parents must exist). Preexisting socket paths are refused, never unlinked.
  The per-run nonce requires OS entropy (`getrandom`): startup fails clearly
  rather than falling back to a weak secret. Non-Windows/Unix socket
  platforms fail with an explicit unsupported error; macOS uses the Unix
   socket path (same as Linux). CI tests remain headless; native macOS
   standalone window coverage is listed above.
- If the pixel context fails, spawns are refused with an explicit error
  instead of producing blank windows.
- Runtime display libraries must be installed for live use (see `devenv.nix`);
  the binary links nothing at build time and only `dlopen`s at runtime.
- Logs go to stderr and never contain the connection nonce or description
  text. A `SIGINT` kill may leave the socket file behind; remove only the
  exact owned socket file (and the run dir when this run created it).
