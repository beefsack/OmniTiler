# Native effect license audit

## Scope and method

At audit start, 17 `kwin/native-effect` source files carried
`SPDX-License-Identifier: GPL-2.0-or-later`. The 15 KWin-free files were
audited separately from `activewindowborder.cpp/.h`, which directly include
KWin effect/window headers.
The root `Cargo.toml`, crate manifests, `git ls-files`, the CMake target source
list, and source includes were inspected. The reference comparison used the
sibling KWin clone at tag `v6.7.5`, commit
`ab7df7ccb7c6af20f4b279cd6220f7cd3d2267d7`, matching the licensing research
header pin.

Commands in OmniTiler: `rg -l 'SPDX-License-Identifier: GPL-2.0-or-later'
kwin/native-effect` and `rg -n '^\s*#\s*include\s*[<"](effect/|kwin|workspace|window|.*kwin)'
kwin/native-effect --glob '*.{h,cpp}'`.

In the KWin reference clone, the exact searches were:

- `git grep -n -i -E 'inflate|deflate|outerRect|innerRect|outline.*thickness|thickness.*outline|alpha\(\)|isValid\(\).*alpha|alpha.*isValid' v6.7.5 -- src/scene src/effect`
- `git grep -n -i -E 'structural floor|covered counter|same instance|native invalidation|paint order|A->B->A|stale paint|reused counters|terminal FIFO|FIFO.*terminal' v6.7.5 -- src/scene src/effect`
- `git grep -n -E 'quint64.*(revision|generation|serial)|m_revision|m_generation|m_serial|sequence' v6.7.5 -- src/scene src/effect`
- `git grep -n -i -E 'interactiveMoveResizeStarted|interactiveMoveResizeFinished|Key_Escape|cancel.*move.*resize|moveResize.*escape|motion.*gesture|moveResizeWindow' v6.7.5 -- src/input src/window.cpp src/effect`
- `git show v6.7.5:src/scene/borderoutline.cpp` and `git show v6.7.5:src/scene/outlinedborderitem.cpp`

The source search found one behavior match: KWin's
`BorderOutline::inflate` expands all four sides by its thickness
(`src/scene/borderoutline.cpp:51-54`), and `OutlinedBorderItem` uses that
expanded geometry (`src/scene/outlinedborderitem.cpp:12-17`). OmniTiler's
`borderStrokeFromPublished` (`presentation_consumer.h:385-410`) inverts this
relationship: it derives a stroke from the four inset differences and accepts
only equal positive insets. Its source comment explicitly cites the KWin
outline behavior. It independently matches the observed outline behavior for
interoperability and does not reuse KWin code. The KWin source file headers
identify `LGPL-2.1-only OR LGPL-3.0-only OR
LicenseRef-KDE-Accepted-LGPL` (`src/scene/borderoutline.cpp:1-4`;
`src/scene/outlinedborderitem.cpp:1-4`).

Other comparisons were bounded: the KWin tree has its own interactive
move/resize Escape handling and lifecycle signals (`src/window.cpp:2501-2522`,
`2570-2588`; `src/effect/effectwindow.cpp:83-91`), but no matching identity-
fenced terminal FIFO was found. The searched KWin scene/effect code had no
matching presentation ordering / causal-coverage floor algorithm, nor the
queried color-validity/alpha gate. The phrase search returned one unrelated
repaint comment at `src/scene/item.cpp:475`, not a freshness/order algorithm.
Generic Qt/C++ patterns and unsearched source outside these directories are
not proof of absence.

The KWin include scan found only `activewindowborder.cpp/.h` including KWin
headers. The broader source/comment/algorithm searches found no KWin code reused
in the 15 KWin-free files. They found the independent outline behavior match
above; the other 14 files had no adapted KWin code identified by these searches.
Negative searches do not prove absence of conceptual similarity or copied
material outside the queried terms and source directories.

## Per-file findings

`KWin includes` reports direct KWin header includes in the file. `Adapted
KWin code` reports evidence identified in the bounded source comparison; `No`
does not claim an exhaustive proof of absence.

| File | KWin includes | Adapted KWin code | File evidence reviewed |
| --- | --- | --- | --- |
| `activeborderlogic.h` | No | No | Facts encoding, bounded lease, and border-color alpha gate (`:23-122`); KWin color search found no matching alpha/theme predicate. |
| `gesturefacts.cpp` | No | No | D-Bus facts publication/waiter implementation (`:38-223`); endpoint types/methods have no pinned-source match. |
| `gesturefacts.h` | No | No | Facts protocol types and endpoint declarations (`:17-120`); endpoint types/methods have no pinned-source match. |
| `gesturefacts_test.cpp` | No | No | Private-bus tests of the local endpoint (`:41-630`); test names/protocol are first-party. |
| `gesture_lifecycle.h` | No | No | Host-free gesture lifecycle state (`:65-457`); no matching pinned-source identifier. |
| `gesturelifecycle_test.cpp` | No | No | Tests of that local lifecycle model (`:33-349`); no KWin source/API exercised. |
| `presentation_consumer.h` | No | No | Order/freshness and coverage logic (`:122-383`) had no matching KWin algorithm/comments; geometry helper (`:385-410`) independently matches observed outline behavior for interoperability and does not reuse KWin code. |
| `presentation_router.h` | No | No | Source-instance counter, structural-floor, and invalidation transitions (`:39-256`); searches found no corresponding KWin ordering/floor algorithm. |
| `presentationfacts.cpp` | No | No | Qt D-Bus facts source and waiter logic (`:13-196`); `WaitPresentationFacts` has no pinned-source match. |
| `presentationfacts.h` | No | No | First-party facts wire contract and source declarations (`:17-88`); no matching pinned-source protocol identifier. |
| `presentationfacts_test.cpp` | No | No | Private-bus transport tests (`:46-540`); no KWin source/API exercised. |
| `presentation_stage5.cpp` | No | No | Counter publication and route orchestration (`:5-301`); `PresentationStage5` has no pinned-source match. |
| `presentation_stage5.h` | No | No | Injected host facts and pure routing/visibility API (`:5-176`); no matching KWin type/helper or algorithm identified. |
| `presentation_stage5_recovery_test.cpp` | No | No | Recorded-fixture recovery tests (`:88-940`); no KWin source/API exercised. |
| `presentation_stage5_test.cpp` | No | No | Offline stage/state transition tests (`:32-748`); no KWin source/API exercised. |

The approved outcome is MIT for all 15 audited KWin-free logic/test sources,
including `presentation_consumer.h`; its independently matching outline
behavior is not reused KWin code. `activewindowborder.cpp/.h`, which include
KWin headers, remain GPL-2.0-or-later, as do the effect plugin binary and
`kwin/native-effect/metadata.json`.

The 15 MIT files follow the existing project convention of no per-file
license or copyright header. Representative MIT files read during the audit
include `kwin/native-effect/activeborderconfig_module.cpp` (starts with an
include), `kwin/native-effect/shortcutreconciler.h` (starts with `#pragma
once`), `kwin/native-effect/activeborderlogic_test.cpp` (starts with an
ordinary test comment), and `crates/tiler-core/src/lib.rs` (starts with a Rust
crate attribute). The root MIT `LICENSE` and project copyright apply. GPL
source keeps explicit SPDX headers. See the
[Licensing decision](../../decisions/development.md#licensing).
