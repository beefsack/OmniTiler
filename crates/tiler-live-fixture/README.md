# tiler-live-fixture

Development-only controllable live test windows. One `tiler-live-fixture`
process owns many native windows (winit + softbuffer CPU text); callers drive
them by caller id over versioned JSON-lines on a private local endpoint
(Unix socket on Linux/macOS, owner-restricted named pipe on Windows).

Existing helpers stay: the Windows `tiler-test-window` binary and the macOS
probe target are untouched. This crate is not product code and is never part
of the shipping roots.

## Smoke (two terminals, from the repo root after an env restart)

Run everything with `cargo run --locked -p tiler-live-fixture --` from the
repo root. Never assume a `target/` directory: `CARGO_TARGET_DIR` may
redirect build output.

```sh
# Terminal 1: start the server in a freshly made private run dir.
RUN_DIR="$(mktemp -d "${XDG_RUNTIME_DIR:-/tmp}/tiler-live-fixture-XXXXXX")"
chmod 700 "$RUN_DIR"
cargo run --locked -p tiler-live-fixture -- serve --dir "$RUN_DIR"
# Prints exactly one JSON line (the only stdout line, the only nonce source):
# {"socket":"$RUN_DIR/fixture.sock","nonce":"<64 hex>","pid":1234}
# On Windows the same announcement carries a pipe name
# ({"socket":"\\.\pipe\tiler-live-fixture-...","nonce":"...","pid":...});
# `send --socket` accepts the announced value verbatim on both transports.
# On Windows `--dir` is lifecycle/naming compat only (created/removed with
# the run): it is never a filesystem security boundary. The pipe DACL
# (SYSTEM + owner only, no remote clients) plus the per-run nonce are the
# Windows security boundary.
```

```sh
# Terminal 2: paste the two announcement values when prompted (no polling,
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
  X11 when `DISPLAY` is set. (`WINIT_UNIX_BACKEND` was removed in winit
  0.29 and does nothing here.) For the X11 run on a KDE session that also
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

## Protocol

- One JSON object per line, max 1 MiB per frame (oversize frames close the
  connection). Every request carries `v: 1`, `request_id`, `nonce`, `op`.
- Ops: `spawn` (many windows), `set`/`get`/`close` (id lists), `list`,
  ordered non-atomic `batch`, `subscribe`/`unsubscribe`, `shutdown`.
  `batch` items accept only `spawn`/`set`/`get`/`close`/`list` with strict
  field checking; unknown item fields are rejected like the rest of the
  protocol.
- Replies echo `request_id`. ID-list ops return per-window entries with
  `ok`, `error`, `snapshot` and per-field `fields` outcomes.
- Snapshots carry `requested` (caller intent, may hold unsupported values)
  beside `observed` (unknown is null, never a request echo), `configured`
  (only values the host actually took where no trustworthy readback exists:
  explicitly a cache, never refused values), `unsupported`, and `settled`.
- Accepted means the host took the request, never that it settled on the
  value. Unsupported actions are reported per field, never faked.
- Subscriptions are connection-scoped: a repeat `subscribe` from the same
  connection replaces the filter (never duplicates events), `unsubscribe`
  removes that connection's entry, and a disconnect forgets it (plus lazy
  cleanup when a broadcast fails). An empty `events` filter is rejected;
  omit `events` for `["all"]`.
- Closing windows never exits the server, even when none remain. Only
  `shutdown` (or killing the process) ends the run.

## Settled (honest minimal semantics)

`settled` reports geometry feedback received, nothing more:

- `settled=false` after an accepted geometry set (`size_px`,
  `position_outer_px`). Cache-only (`decorations`, `resizable`, `min/max`),
  local-only (`description`), creation-only, mode/visible, and
  unsupported-only patches never make a window pending; they leave `settled`
  unchanged.
- `settled=true` after host geometry feedback (resize/move notifications,
  including scale changes).
- `settled=true` after a `get` whose fresh readback matches the requested
  geometry. Unknown or mismatched geometry never claims settled.
- Accepted never implies settled: an accepted outcome only proves the host
  took the request. Always compare `observed` against `requested` to detect
  compositor negotiation.

Immediate spawn/set replies can precede native feedback. Issue a later `get`
with a bounded wait and compare the relevant observed fields, not just `settled`:
mode changes do not make geometry pending, and clamped geometry can settle
without matching the request. For example, after the geometry smoke request:

```sh
run '{"v":1,"request_id":"geometry-readback","op":"get","ids":["a"]}'
```

If feedback is still pending, repeat within the test's deadline; a fixed sleep
alone is not evidence. `list` returns IDs only; use `get` for snapshots.

Prefer the event-driven `--wait-ms`/`--expect` form above for an exact observed
predicate. It does not use geometry-only `settled`: negotiated/clamped geometry
can be settled without matching, and focus/mode changes need their own readbacks.

## Additional window controls

All fields below work with spawn/set unless marked creation-only. These are
requests/hints, not proof of compositor effects. Values without trustworthy
readback live in `configured`, never in `observed`.

| Field | Values / purpose | Backend limits |
| --- | --- | --- |
| `window_level` | `always_on_top`, `normal`, `always_on_bottom` | X11/Windows/macOS hint; Wayland non-normal unsupported, normal accepted as default no-op |
| `focus` | `true` requests raise/focus; `false` does nothing | Wayland `focus_window` is a winit no-op, so true reports unsupported; use real `observed.focused` |
| `attention` | `informational`, `critical`, `clear` | X11 urgency collapses severity; clear clears the WM_HINTS urgency bit but KWin keeps `_NET_WM_STATE_DEMANDS_ATTENTION` on an inactive window until it is activated (accepted is not proof of withdrawal); Wayland needs activation protocol/context and cannot clear; macOS clear is a winit no-op |
| `title` | Suffix appended to the immutable sentinel/id/run-tag prefix; empty resets | No Linux title readback; `snapshot.title` is constructed, not observed |
| `cursor_visible` | Boolean, cursor over owned content | No readback; no capture, grab or global pointer move |
| `cursor_hittest` | Boolean; false makes content input-passthrough | Safe winit result reported per field; host refusal is unsupported |
| `enabled_buttons` | Array containing `close`, `minimize`, `maximize`; empty disables all | X11/Wayland winit setters unimplemented: unsupported |
| `resize_increments_px` | Positive physical `[width,height]` hint | Wayland unsupported; no portable enforcement/readback claim |
| `theme` | `system`, `light`, `dark` | Host/decoration hint; no cross-backend theme observation |
| `transparent` | Boolean; creation-only on X11 | Host hint only: software content remains opaque, not an alpha-rendering test |
| `blur` | Boolean hint | Wayland requires KDE blur protocol; macOS supported; X11/Windows true unsupported, false default no-op |
| `override_redirect` | Boolean, creation-only unmanaged X11 witness | True unsupported elsewhere; runtime changes never recreate a window |

`focus`/`attention` are one-shot actions: per-field outcomes report them, but
they are not persisted in `requested`/`configured` or replayed by unrelated
sets. Refused persistent intent stays in `requested`; its `unsupported` tag
survives unrelated changes until that field is accepted again.

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

### API sweep exclusions and platform follow-ups

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
  winit APIs; the fixture transport compiles on both native targets in CI.
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
- Each window renders a header plus its full snapshot as JSON (every
  exposed field, unknown is null) plus the independently settable
  `description` (wrapped; scroll with Up/Down, PageUp/PageDown, Home;
  scrolling is unbounded and saturating).
- Host chrome only: normal decorations, title and borders. No custom hooks.

## Platform example

`platform.rs` holds the extension pattern: creation-only X11 window type
`normal`/`dialog`/`utility` via safe winit extension APIs, gated per
backend. A runtime change reports `unsupported` instead of recreating the
window. Future Windows/macOS actions follow the same shape: one catalog
entry plus one capability flag, no plugin framework.

## Honest host limitations

- Wayland: no outer/global position, no restore-from-minimize, no
  show/hide (`visible` requests report unsupported), and exclusive
  fullscreen is reported unsupported. Sizes and hints are negotiated with
  the compositor; decorations/min/max may stay cached rather than observed.
- Readbacks never echo the request: on Wayland fullscreen reflects the
  compositor configure; on X11 the winit fullscreen getter is a setter
  cache (written on the request path, never from window-manager events),
  so a set fullscreen there stays null (unknown) in `observed` while
  `configured` keeps the sent value. Maximized comes from its getter;
  minimized/visible are optional readbacks with unknown staying null
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
  socket path (same as Linux) and compiles in CI, with live behavior
  covered by the same headless transport tests where practical.
- If the pixel context fails, spawns are refused with an explicit error
  instead of producing blank windows.
- Runtime display libraries must be installed for live use (see `devenv.nix`);
  the binary links nothing at build time and only `dlopen`s at runtime.
- Logs go to stderr and never contain the connection nonce or description
  text. A `SIGINT` kill may leave the socket file behind; remove only the
  exact owned socket file (and the run dir when this run created it).
