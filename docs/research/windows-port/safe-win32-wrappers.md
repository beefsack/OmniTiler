# Safe Win32 wrappers (research item 13g)

- Researched 2026-10-16, revised same day. User decision 2026-10-17: option A
  (one minimal safe FFI module on `windows-sys`; keep the module minimal).
  Route chosen; implementation queued, not started.
- Backlog: [13g](../../backlog.md). Principle: [Unsafe Code](../../principles.md#unsafe-code).
  Guardrail: [Rust unsafe guardrail](../../decisions.md#rust-unsafe-guardrail).
- Snapshot: local source at `aa76633` (2026-10-16); upstream data as served
  2026-10-16 (upstream timestamps recorded below).
- Bottom line: `winsafe` has substantial adoption but concentrated maintenance
  and leaves our hardest families unsafe or unmatched; the evidence does not
  establish it as battle-hardened for safety-critical hook/window-proc use, and
  no defects are claimed. Both Microsoft crates keep the sampled native calls
  `unsafe`. Recommended route is one dedicated boundary module atop the current
  `windows-sys`, pending user choice.

## 1. Current binding

- `crates/tiler-windows/Cargo.toml` declares `windows-sys = "0.61"` with 21
  `Win32_*` features; `Cargo.lock` pins `windows-sys 0.61.2`.

## 2. Unsafe inventory

Definitions over `crates/tiler-windows/src/**/*.rs`: A = lines matching
`unsafe\s*\{` (line-based, not AST blocks); B = `unsafe fn` definitions;
C = `unsafe extern "system" fn` definitions.

- Totals: A = 809 lines in 67 files. B = 2 (`snapkey/sys/mod.rs`
  `test_drive_llproc`, `win_mouse.rs` `test_drive_mouse`). C = 17 in 13 files
  (2 each in `active_border_sys.rs`, `pager_sys.rs`, `tray_sys/mod.rs`,
  `tiling_sys/inspection.rs`; 1 each in `winarrow.rs`, `win_mouse.rs`,
  `snapkey/sys/mod.rs`, `test_window.rs`, `tiling_sys/observe.rs`,
  `tiling_sys/monitors.rs`, `tiling_sys/observation.rs`,
  `tiling_sys/tests_restart.rs`, `settings_ui/mod.rs`).
- No `unsafe impl` / `unsafe trait`. 17 safe roots carry
  `#![forbid(unsafe_code)]`. Zero `allow(unsafe_code)` sites.
- Repro at `aa76633`:
  `rg -c 'unsafe\s*\{' crates/tiler-windows/src --glob '*.rs' --sort path`
  (A, sums to 809); same with `'unsafe\s+fn'` (B) and `'unsafe\s+extern'` (C).

| Module | A |
| --- | --- |
| `tray_sys/mod.rs` | 94 |
| `settings_ui/mod.rs` | 63 |
| `pager_sys.rs` | 57 |
| `product_hide.rs` | 49 |
| `active_border_sys.rs` | 39 |
| `tiling_sys/fullscreen.rs` | 37 |
| `winarrow.rs` | 35 |
| `test_window.rs` | 31 |
| `render_pump/native/mod.rs` | 30 |
| `native.rs` (tokens/session/process) | 29 |
| `win_mouse.rs` | 23 |
| `tiling_sys/observe.rs` | 22 |
| `tiling_sys/inspection.rs` | 21 |
| `pager_icons_sys.rs` | 18 |
| `tiling_sys/minmax.rs` | 17 |
| `tiling_sys/history.rs`, `tiling_sys/probes.rs` | 15 each |
| `snapkey/sys/mod.rs` | 14 |
| `tiling_sys/` other 37 files (largest: `tests_restart` 13, `settings` 12, `tiling_sys/group_underlay` 12, `focus_actuation` 11) | 145 |
| Other 12 files (`settings_ui/ctl` 11, `sys_tests` 11, `input_pump` 5+4+4, `tray_sys/gaming` 6, rest 1-4) | 55 |

Check: 609 + 145 + 55 = 809 across 18 + 37 + 12 = 67 files.

## 3. `windows` vs `windows-sys` signatures

All linked signatures fetched 2026-10-16. For these representatives of every
requested family, `windows-sys` declares
`pub unsafe extern "system" fn` (unsafety at each call-site block); `windows`
wraps as `pub unsafe fn` (unsafety at each call site via signature, plus
`Result`/typed conveniences). Neither removes `unsafe` from our code.

| Family (call) | `windows` 0.62.2 | `windows-sys` 0.61.2 |
| --- | --- | --- |
| LL hooks (`SetWindowsHookExW`) | [unsafe wrapper][W1], `Result<HHOOK>` | [unsafe extern][S1], `HHOOK` |
| Window proc (`DefWindowProcW`) | [unsafe wrapper][W2] | [unsafe extern][S2] |
| SPI (`SystemParametersInfoW`) | [unsafe wrapper][W3] | [unsafe extern][S3] |
| Tokens (`OpenProcessToken`) | [unsafe wrapper][W4] | [unsafe extern][S4] |
| Accessibility (`SetWinEventHook`) | [unsafe wrapper][W5] | [unsafe extern][S5] |
| DWM (`DwmGetWindowAttribute`) | [unsafe wrapper][W6] | [unsafe extern][S6] |
| Tray (`Shell_NotifyIconW`) | [unsafe wrapper][W7] | [unsafe extern][S7] |
| WTS module (`ProcessIdToSessionId`) | [unsafe wrapper][W8] | [unsafe extern][S8] |
| Layered windows (`UpdateLayeredWindow`) | [unsafe wrapper][W9] | [unsafe extern][S9] |
| GDI (`CreateDIBSection`) | [unsafe wrapper][W10] | [unsafe extern][S10] |
| ToolHelp (`CreateToolhelp32Snapshot`, unused today) | [unsafe wrapper][W11] | [unsafe extern][S11] |

- `CallNextHookEx`, `RegisterClassW` and `GetTokenInformation` were also
  checked in both crates: all unsafe. This is a family-level check, not an
  exhaustive signature audit of the crate. It rules out either binding as
  a wholly safe replacement for our required native surface.
- `windows` pages are Microsoft's rolling docs, headered 0.62.2 when read;
  `windows-sys` links are version-pinned. Constants/types do not themselves
  require unsafe; callback payload dereferences and ownership remain our
  responsibility. Avoiding all own unsafe while retaining these raw calls
  and callback state handling is not feasible.

## 4. `winsafe` assessment

Adoption and maintenance (fetched 2026-10-16; popularity is context, never
safety proof):

- crates.io: max `0.0.29` (2026-09-01); 51,304,878 all-time /
  10,652,728 trailing-90-day downloads (`recent_downloads` is the 90-day
  window). [Releases][crate-winsafe]: 0.0.24 (2025-06-01), 0.0.25
  (2025-07-01), 0.0.26 (2025-10-01), 0.0.27 (2026-01-01), 0.0.28
  (2026-07-01), 0.0.29 (2026-09-01): active, with gaps of 1-6 months.
- [Reverse dependencies][reverse]: 27 listed crates, including Bevy helpers
  and `temp_deno_which`. Downloads include automated/transitive fetches,
  not unique users or proof that hooks/overlays are exercised.
- Maintenance is concentrated: GitHub `rodrigocfd/winsafe` (696 stars,
  48 forks, pushed 2026-10-12;
  [repository metadata][repo-winsafe]) lists ~24 contributors
  but the owner holds 1,934 of ~1,968 commits; sole crates.io owner
  ([owners][owners]). MIT.
  Comparison: `microsoft/windows-rs` 12,829 stars / 681 forks
  ([repository metadata][repo-windows]); `windows-sys`
  1.7B / 458M downloads; `windows` 365M / 93M.
- Conclusion, narrowly drawn: the evidence does not establish `winsafe` as
  high-quality/battle-hardened for our hook/window-proc use (0.0.x, one
  maintainer, niche downstream). No defects are claimed.

Coverage checked against the [0.0.29 API index][winsafe-index] and linked
signatures. "Not found" is a search result, not proof no alternative exists.

| Family (our exact calls) | `winsafe` 0.0.29 | Status |
| --- | --- | --- |
| LL hooks (`SetWindowsHookExW`, `CallNextHookEx`; hook payload derefs) | [Safe setup/unhook][safe-hook]; callback aliases do not validate raw `lParam` payloads for us | Partial: own payload/state boundary still needed |
| DWM cloak/frame bounds (`DwmGetWindowAttribute`) | [Safe getter][safe-hwnd] with [typed `Cloak`, `Cloaked`, `ExtendedFrameBounds`][safe-dwm] | Covered |
| Tray (`Shell_NotifyIconW`) | [Safe `Shell_NotifyIcon`][safe-tray] | Covered |
| Tokens/SID (`OpenProcessToken`, `GetTokenInformation`, SID conversion/subauthorities) | [Safe token open][safe-process], [typed information][safe-token], [SID accessors][safe-sid] | Covered API shapes; ownership adaptation still required |
| ToolHelp (feature enabled, no current calls) | [Safe snapshot/iteration][safe-toolhelp] | Covered if needed |
| Layered GDI (`UpdateLayeredWindow`, `CreateDIBSection`, `SelectObject`, raw pixels) | [Safe upload][safe-hwnd]; no `CreateDIBSection` found in the searched index | Partial; no verified safe replacement for our DIB path |
| Window procs (`RegisterClassW`, `DefWindowProcW`, USERDATA state) | [`RegisterClassEx`][safe-class] and [`HWND::DefWindowProc`][safe-hwnd] are unsafe; GUI layer manages owned windows, not a drop-in foreign-window adapter | Gap; safe owned-state/callback abstraction needs design |
| SPI (`SystemParametersInfoW`) | [`SystemParametersInfo` is unsafe][safe-spi]: wrong payload type may overrun buffers | Gap |
| Accessibility (`SetWinEventHook`, `UnhookWinEvent`) | No wrapper found in the searched API index | Gap |
| WTS module (only `ProcessIdToSessionId` used) | No matching session-ID wrapper found; WTS notification constants/struct exist | Gap even for current session query; no current WTS enumeration requirement |

## 5. Minimal-boundary comparison

One dedicated boundary module per crate (a logical module; may file-subdivide
internally). No benchmarks run; no build/runtime numbers claimed.

- Conveniences: `windows` adds typed signatures, `Result` errors, string/COM
  helpers. `windows-sys` keeps today's raw shapes (matches all 67 files).
- Simplicity: `windows-sys` boundary moves code without re-spelling calls.
  `windows` re-spells most call sites for zero unsafety reduction.
- Compile cost: unmeasured; both Microsoft crates are feature-gated generated
  bindings. `windows` adds projection/core/error/string machinery; `windows-sys`
  has only `windows-link` as a normal dependency. Retaining it avoids new
  compile work; exact clean/incremental cost needs an isolated prototype.
- Migration churn: mostly design work, not file moves: every `unsafe` site
  needs a safe ownership/borrow/callback API (handle guards, USERDATA
  lifetime discipline, hook-proc payload validation). `windows` adds a
  dependency swap plus that same design work across more call shapes;
  `winsafe` adds a second FFI idiom (safe where covered, raw fallback for
  hooks/wndprocs/SPI/WinEvent/WTS).
- Runtime: unsafe is a compile-time contract, not a runtime check. Raw calls
  and thin inline projections have no inherent extra OS call, but allocations,
  conversions, error mapping and RAII cleanup can add work. Keep LL callbacks
  allocation-free and retain existing safe input/render handoffs; measure
  any changed hot path. No performance measurements were made.

## 6. Recommendation (selected User 2026-10-17: option A; implementation queued, not started)

- A (recommended): one boundary module atop `windows-sys`. Lowest churn,
  keeps the Microsoft binding; no verified complete safe replacement in
  `winsafe` for this surface.
- B: boundary atop `windows`. Ergonomics gain, full re-spell churn,
  unmeasured compile impact.
- C: `winsafe` where covered + raw fallback. Splits idioms, keeps
  concentrated-maintenance dependency for partial gain. Not recommended.

## 7. Follow-through (pending implementation)

- Root `[workspace.lints.rust] unsafe_code = "forbid"` (absent today).
  Normal crates use `[lints] workspace = true` (inherit forbid).
- Each approved FFI crate (`tiler-windows`, `tiler-kwin-effect-ffi`, and the
  separately scoped `tiler-macos`) does NOT
  inherit: Cargo rejects inherit-plus-override in one table. It carries an
  explicit exception table `[lints.rust] unsafe_code = "deny"`, inherits
  nothing. `deny` (not `forbid`) because `forbid` cannot be overridden by an
  inner `allow`.
- Inside an FFI crate: every non-boundary module carries
  `#![forbid(unsafe_code)]` (forbid-over-deny is allowed); exactly one
  dedicated boundary module carries `#![allow(unsafe_code)]`.
- Review the safe facade's ownership, buffer sizes/alignment, callback lifetime
  and no-unwind behavior, thread affinity and cleanup. Moving raw calls into
  safe functions that still trust arbitrary caller pointers is not sufficient.
- CI: allowlist the exact reviewed boundary file per exception crate; reject
  all other `allow(unsafe_code)` sites, including whitespace, multiline,
  combined-lint and `cfg_attr` variants. Also check crate lint inheritance
  and overrides so a new root cannot silently escape the default. Prove the
  gate with allowed-boundary and rejected-stray fixtures, then run existing
  fmt/build/test/strict-clippy gates on applicable platforms. This is a plan,
  not enforcement delivered by this research.

## Sources (fetched 2026-10-16)

- Local: `crates/tiler-windows/Cargo.toml`, `Cargo.toml`, `Cargo.lock`
  (`windows-sys 0.61.2`), `crates/tiler-windows/src/**` at `aa76633`.
- [winsafe metadata/releases][crate-winsafe], [owners][owners],
  [contributors][contributors], [reverse dependencies][reverse].
- [windows metadata](https://crates.io/api/v1/crates/windows) and
  [windows-sys metadata](https://crates.io/api/v1/crates/windows-sys): latest
  0.62.2 / 0.61.2, both released 2025-10-06 per the registry API.
- [winsafe documentation](https://docs.rs/winsafe/0.0.29/winsafe/) describes handwritten bindings since
  2019, UTF-8 conversion, RAII guards and its owned-window GUI layer.
- [Cargo lint inheritance](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-lints-table)
  and [Rust lint levels](https://doc.rust-lang.org/rustc/lints/levels.html).

## Verification limits

- Line-based grep, not AST; no build, migration prototype or live test.
- Signature checks cover the representatives in section 3, not every API
  imported anywhere in the crate. All table signatures were read directly.
- Coverage is API/source inspection, not a soundness audit of dependencies;
  absence findings mean "not found in the searched index".
- Popularity data are snapshots as served; used only for the principle's
  popularity bar, never as correctness proof.

[W1]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/WindowsAndMessaging/fn.SetWindowsHookExW.html
[S1]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/UI/WindowsAndMessaging/fn.SetWindowsHookExW.html
[W2]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/WindowsAndMessaging/fn.DefWindowProcW.html
[S2]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/UI/WindowsAndMessaging/fn.DefWindowProcW.html
[W3]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/WindowsAndMessaging/fn.SystemParametersInfoW.html
[S3]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/UI/WindowsAndMessaging/fn.SystemParametersInfoW.html
[W4]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Threading/fn.OpenProcessToken.html
[S4]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/System/Threading/fn.OpenProcessToken.html
[W5]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/Accessibility/fn.SetWinEventHook.html
[S5]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/UI/Accessibility/fn.SetWinEventHook.html
[W6]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Graphics/Dwm/fn.DwmGetWindowAttribute.html
[S6]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/Graphics/Dwm/fn.DwmGetWindowAttribute.html
[W7]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/Shell/fn.Shell_NotifyIconW.html
[S7]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/UI/Shell/fn.Shell_NotifyIconW.html
[W8]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/RemoteDesktop/fn.ProcessIdToSessionId.html
[S8]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/System/RemoteDesktop/fn.ProcessIdToSessionId.html
[W9]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/WindowsAndMessaging/fn.UpdateLayeredWindow.html
[S9]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/UI/WindowsAndMessaging/fn.UpdateLayeredWindow.html
[W10]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Graphics/Gdi/fn.CreateDIBSection.html
[S10]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/Graphics/Gdi/fn.CreateDIBSection.html
[W11]: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Diagnostics/ToolHelp/fn.CreateToolhelp32Snapshot.html
[S11]: https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/System/Diagnostics/ToolHelp/fn.CreateToolhelp32Snapshot.html
[crate-winsafe]: https://crates.io/api/v1/crates/winsafe
[owners]: https://crates.io/api/v1/crates/winsafe/owners
[reverse]: https://crates.io/api/v1/crates/winsafe/reverse_dependencies?per_page=100
[repo-winsafe]: https://api.github.com/repos/rodrigocfd/winsafe
[repo-windows]: https://api.github.com/repos/microsoft/windows-rs
[contributors]: https://api.github.com/repos/rodrigocfd/winsafe/contributors?per_page=100
[winsafe-index]: https://docs.rs/winsafe/0.0.29/winsafe/all.html
[safe-hook]: https://docs.rs/winsafe/0.0.29/winsafe/struct.HHOOK.html#method.SetWindowsHookEx
[safe-hwnd]: https://docs.rs/winsafe/0.0.29/winsafe/struct.HWND.html
[safe-dwm]: https://docs.rs/winsafe/0.0.29/winsafe/enum.DwmAttr.html
[safe-tray]: https://docs.rs/winsafe/0.0.29/winsafe/fn.Shell_NotifyIcon.html
[safe-process]: https://docs.rs/winsafe/0.0.29/winsafe/struct.HPROCESS.html#method.OpenProcessToken
[safe-token]: https://docs.rs/winsafe/0.0.29/winsafe/struct.HACCESSTOKEN.html#method.GetTokenInformation
[safe-sid]: https://docs.rs/winsafe/0.0.29/winsafe/struct.SID.html
[safe-toolhelp]: https://docs.rs/winsafe/0.0.29/winsafe/struct.HPROCESSLIST.html
[safe-class]: https://docs.rs/winsafe/0.0.29/winsafe/fn.RegisterClassEx.html
[safe-spi]: https://docs.rs/winsafe/0.0.29/winsafe/fn.SystemParametersInfo.html
