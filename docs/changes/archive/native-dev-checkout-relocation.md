# Native development checkout relocation

## Goal and scope

- Make `just dev-native-setup` recover from a moved checkout without manual cache or environment-script cleanup.
- Recreate a regenerable native build directory only when its CMake cache records a different source or build directory; matching caches remain incremental.
- Replace only a canonical generated environment export whose previous checkout stage no longer exists. Preserve unfamiliar content and refuse another existing checkout's stage, symlinks, and non-regular files.
- Preserve native session delivery: rebuilt plugins still require logout/login. No development lifecycle or shortcut behavior changes.

## Acceptance and approach

- Validate cache identity before configuring and emit a concise notice when recreating relocated caches.
- Strictly recognize generated environment bytes without evaluating shell content, and explain relocation or competing-checkout refusal.
- Extend existing isolated shell fixtures with moved/matching caches and missing/existing/unfamiliar environment cases.
- Run relevant shell suites and inspect deletion boundaries and file-replacement behavior.

## Outcome and verification

- The host-native builder recreates caches recording relocated source/build paths with a one-line notice, preserving matching incremental builds and refusing unsafe cleanup paths.
- Native setup replaces only byte-exact generated exports for a missing previous `target/kwin-native-effect-stage`, names both stages, and refuses other existing stages with a checkout-specific removal hint.
- Whole-file byte comparison and snapshot revalidation preserve unfamiliar content; trailing-slash symlink build paths and dangling stage symlinks are refused. Paths with spaces and single quotes remain supported.
- Boundary review findings were corrected before acceptance: strict stage suffix, byte-exact parsing, snapshot-pinned approval, checkout-root hint, and trailing-slash symlink protection.
- Isolated shell fixtures cover relocation, incremental retention, competing checkouts, malformed content, escaped paths, symlink preservation, and a file-swap refusal.
- Verification: `bash scripts/nix-host-kwin-build.test.sh` 125 pass / 0 fail; `bash scripts/dev-native-effect.test.sh` 204 pass / 0 fail; `bash scripts/dogfood-install.test.sh` 572 pass / 0 fail.
- Development lifecycle and shortcut behavior are unchanged; native delivery still requires logout/login.
