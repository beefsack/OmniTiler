# Vision

This is goal input for the project. It is not a governance document: active
decisions live in `docs/decisions.md`, process principles in
`docs/principles.md`.

## Purpose

Provide a faithful, jank-free auto-tiling experience for users of COSMIC, Hyprland and i3/sway when they need to use a different OS/DE/WM, such as KDE Plasma, GNOME, Windows or macOS.

## First-Class Requirements

Auto tiling, window and group highlights, keybinds, panels, and workspace
support are all first-class requirements. None is an optional extra.

## Configurability

Fully configurable. Users of tiling window managers generally run custom
configurations and expect to keep them.

## Faithfulness Versus Jank

Sometimes being faithful introduces jank. That trade-off is a subjective
decision owned by the project owner. Where it is inexpensive to do so, expose
the choice to the end user so they can configure towards faithfulness or
towards minimising jank.

## Reliability

Reliability is non-negotiable. The project must avoid crashing or silently
stopping working, including across:

- sleep and wake
- plugging and unplugging outputs
- resolution and scaling changes
- full-screen applications
- changes to underlying configuration that affect this project

When OmniTiler does fail, the desktop must stay usable. Prefer failing open: let go of the affected scope so windows stay visible, movable and focusable, and input and shortcuts return to the host. Fail closed only in rare cases where letting go would cause harm, break other system functions or leave a worse state, or where the platform forces it.

## Gaming And Full-Screen Applications

OmniTiler manages games and full-screen applications like any other window: they belong to workspaces and the user's explicit actions apply to them. It must never degrade them. A full-screen window, exclusive or borderless, is never tiled, resized, refocused, overlaid with borders or highlights, or has its input intercepted unless the user asks. Nothing OmniTiler does may look like cheating to anti-cheat systems. A windowed game is an ordinary window.

The target is aspirationally zero performance impact. Where a platform makes
zero impossible, the residual cost must be provably minimal and imperceivable,
and the project must be able to demonstrate what that cost is.

## Rust First

The project is Rust first, with minimal shims only where a platform makes them
unavoidable. Share and reuse code as much as possible across both the target
tiling window managers being emulated and the underlying host OS/DE/WM.

## One Core

Behaviour is decided once, in a shared core: it owns the desired world state and as much shared functional state as possible, so every platform behaves the same. Platform code stays thin. It moves state into the core rather than out of it. Narrow exceptions exist only where a round trip to the core would cause visible jank or a real performance cost, and even then the platform only applies or vetoes; it never decides.
