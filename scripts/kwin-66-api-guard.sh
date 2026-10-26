#!/usr/bin/env bash
# KWin 6.6 API guard: fail when kwin/src uses a scripting-API symbol that is
# unavailable on KWin 6.6 without the workspace-native capability seam, or
# any new runtime API symbol that has not been audited against 6.6 headers.
#
# Audited baseline (2026-10-11, KWin v6.6.4 headers): every runtime
# workspace/window/output/options symbol in kwin/src exists on 6.6 EXCEPT
#   - workspace.currentDesktopForScreen / setCurrentDesktopForScreen
#   - options.perOutputVirtualDesktops (migration-only)
#   - workspace.virtualDesktopNavigationWrapsAround (marked \since 6.7)
# Those live only in kwin/src/workspace-native.ts (feature detection plus
# shared fallbacks) and kwin/src/kwin-globals.d.ts (type declarations).
#
# Addition 2026-10-26: clientGeometry, Workspace.stackingOrder, and
# Workspace.windowAt audited against sibling kwin sources at v6.6.0
# (commit e53bb3be7ecf1735c7739b797f94d66de422ab4d; identical lines at
# v6.6.4): Window clientGeometry is src/window.h:123
# (Q_PROPERTY KWin::RectF READ/NOTIFY); Workspace.stackingOrder is
# src/scripting/workspace_wrapper.h:66 (QList<Window *>, bottommost
# first); Workspace.windowAt is workspace_wrapper.h:309
# (Q_INVOKABLE QList<Window *> windowAt(const QPointF &, int count = 1)).
# All three call sites in kwin/src/world-observe.ts read defensively
# (feature-detect, try/catch, null on unreadable; windowAt advisory only).
#
# Usage: scripts/kwin-66-api-guard.sh [SRC_DIR]
# Exit 0 when clean, 1 with violations on stdout.
set -uo pipefail

SRC_DIR="${1:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../kwin/src" && pwd)}"

FAIL=0

# ---- Part 1: 6.7-only symbols stay confined to the capability seam ----
CONFINED_OWNER="workspace-native.ts"
BANNED_RE='currentDesktopForScreen|setCurrentDesktopForScreen|perOutputVirtualDesktops|virtualDesktopNavigationWrapsAround'

while IFS= read -r file; do
    base="$(basename -- "$file")"
    if [[ "$base" == "$CONFINED_OWNER" || "$base" == "kwin-globals.d.ts" ]]; then
        continue
    fi
    # Strip // line comments; block comments are not used in kwin/src.
    hits="$(sed 's://.*$::' "$file" | grep -nE "$BANNED_RE" || true)"
    if [[ -n "$hits" ]]; then
        echo "kwin-66-api-guard: 6.7-only API outside $CONFINED_OWNER: $file"
        echo "$hits" | sed 's/^/  /'
        FAIL=1
    fi
done < <(find "$SRC_DIR" -maxdepth 1 -name '*.ts' | sort)

# ---- Part 2: every runtime API name must be audited (allowlist) ----
# Receiver patterns exclude parens (not commas) so TypeScript casts with
# generic commas such as `Record<string, unknown>` do not hide a name.
ALLOW=(
    activeScreen activeWindow clientArea clientGeometry configChanged createDesktop
    currentDesktop currentDesktopChanged currentDesktopForScreen cursorPos desktops
    desktopsChanged frameGeometry frameGeometryChanged fullScreen
    fullScreenChanged geometry id interactiveMoveResizeFinished
    interactiveMoveResizeStarted interactiveMoveResizeStepped internalId
    keepAbove keepBelow length managed maximizedChanged maximizeMode maxSize
    minimized minSize move name normalWindow onAllDesktops output outputChanged
    perOutputVirtualDesktops removeDesktop resize resizeable resourceClass
    screens screensChanged sendClientToScreen setCurrentDesktopForScreen
    setMaximize slotSwitchToAboveScreen slotSwitchToBelowScreen
    slotSwitchToLeftScreen slotSwitchToRightScreen stackingOrder transient transientFor
    windowActivated windowAdded windowAt windowList windowRemoved x11DesktopNumber
)

allow_hit() {
    local needle="$1"
    local entry
    for entry in "${ALLOW[@]}"; do
        if [[ "$entry" == "$needle" ]]; then
            return 0
        fi
    done
    return 1
}

used="$(
    {
        grep -hoE 'readProp\([^()]+, "[^"]+"\)' "$SRC_DIR"/*.ts || true
        grep -hoE 'Reflect\.(get|set)\([^()]+, "[^"]+"\)' "$SRC_DIR"/*.ts || true
        grep -hoE 'readSignal\([^()]+, "[^"]+"\)' "$SRC_DIR"/*.ts || true
        grep -hoE '(sub|subWindow[A-Za-z]*)\("[^"]+"' "$SRC_DIR"/*.ts || true
        grep -hoE '"slot[A-Za-z]+"' "$SRC_DIR"/*.ts || true
    } | grep -oE '"[^"]+"' | tr -d '"' | sort -u
)"

while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    if ! allow_hit "$name"; then
        echo "kwin-66-api-guard: unaudited API symbol: $name"
        echo "  Audit it against KWin 6.6.4 headers, then extend ALLOW in $0."
        FAIL=1
    fi
done <<< "$used"

# ---- Part 3: direct workspace bracket reads stay within the allowlist ----
bracket="$(
    grep -hoE '(surface|liveWorkspace)\["[A-Za-z]+"\]' "$SRC_DIR"/*.ts || true
)"

if [[ -n "$bracket" ]]; then
    while IFS= read -r name; do
        [[ -z "$name" ]] && continue
        if ! allow_hit "$name"; then
            echo "kwin-66-api-guard: unaudited workspace bracket symbol: $name"
            FAIL=1
        fi
    done < <(echo "$bracket" | grep -oE '"[A-Za-z]+"' | tr -d '"' | sort -u)
fi

if [[ "$FAIL" -eq 0 ]]; then
    echo "kwin-66-api-guard: clean"
fi
exit "$FAIL"
