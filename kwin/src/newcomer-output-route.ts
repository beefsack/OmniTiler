// R-OUT-05 focused-output newcomer routing (User 2026-10-10, 0.1 triage D06).
//
// `observeNative` follows the active window's output after KWin activates a
// newcomer, so host-mapped pointer-output births would admit there. An
// eligible newcomer instead moves to the last managed output's shown
// workspace before tiling, with newcomer focus only after proof. No Rust
// core change: routing is host placement, and the existing transfer
// primitives already prove arrival.

export interface NewcomerRouteFlags {
    readonly fullscreen: boolean;
    readonly maximized: boolean;
    readonly floating: boolean;
    readonly sticky: boolean;
    // Adapter-asserted fixed-size provenance. These mark fixed-size-classified
    // windows (fixed games are a subset, not the whole set); all of them are
    // non-ordinary tiles, hence ineligible.
    readonly fixedAuto?: boolean | undefined;
    readonly fixedSuppress?: boolean | undefined;
}

export interface NewcomerRouteDomain {
    readonly output: string;
    readonly workspace: string;
}

export interface NewcomerRouteDecisionInput {
    readonly newcomerOutput: string;
    readonly flags: NewcomerRouteFlags;
    readonly unknownCount: number;
    readonly addedSinceRefresh: boolean;
    readonly lastManaged: NewcomerRouteDomain | null;
    readonly flightBusy: boolean;
}

export type NewcomerRouteRefusal =
    | "no-managed-domain"
    | "no-recent-add"
    | "flight-busy"
    | "same-output"
    | "multi-unknown"
    | "fullscreen"
    | "maximized"
    | "floating"
    | "sticky"
    | "fixed-size";

export type NewcomerRouteDecision =
    | { readonly ok: true; readonly target: NewcomerRouteDomain }
    | { readonly ok: false; readonly reason: NewcomerRouteRefusal };

export function decideNewcomerOutputRoute(input: NewcomerRouteDecisionInput): NewcomerRouteDecision {
    if (input.lastManaged === null) {
        return { ok: false, reason: "no-managed-domain" };
    }
    if (!input.addedSinceRefresh) {
        return { ok: false, reason: "no-recent-add" };
    }
    if (input.flightBusy) {
        return { ok: false, reason: "flight-busy" };
    }
    if (input.unknownCount !== 1) {
        return { ok: false, reason: "multi-unknown" };
    }
    if (input.newcomerOutput === input.lastManaged.output) {
        return { ok: false, reason: "same-output" };
    }
    const flags = input.flags;
    if (flags.fullscreen) {
        return { ok: false, reason: "fullscreen" };
    }
    if (flags.maximized) {
        return { ok: false, reason: "maximized" };
    }
    if (flags.floating) {
        return { ok: false, reason: "floating" };
    }
    if (flags.sticky) {
        return { ok: false, reason: "sticky" };
    }
    if (flags.fixedAuto === true || flags.fixedSuppress === true) {
        return { ok: false, reason: "fixed-size" };
    }
    return { ok: true, target: { output: input.lastManaged.output, workspace: input.lastManaged.workspace } };
}

export interface NewcomerRouteEnv {
    readonly resolveOutput?: ((name: string) => object | null) | undefined;
    readonly resolveDesktop?: ((workspace: string) => object | null) | undefined;
    readonly sendClientToScreen?: ((mover: object, output: object) => boolean) | undefined;
    readonly setDesktops?: ((mover: object, desktops: ReadonlyArray<object>) => boolean) | undefined;
    readonly readOutputName?: ((ref: object) => string | null) | undefined;
    readonly readDesktopIds?: ((ref: object) => ReadonlyArray<string> | null) | undefined;
    readonly setActive?: ((target: object) => boolean) | undefined;
    readonly reobserve?: ((ref: object) => { readonly output: string; readonly workspace: string; readonly fullscreen: boolean; readonly maximized: boolean } | null) | undefined;
}

export type NewcomerRouteOutcome =
    | "routed"
    | "routed-focus-failed"
    | "refused-no-output"
    | "refused-no-desktop"
    | "refused-race"
    | "refused-no-transfer"
    | "refused-send"
    | "refused-desktops"
    | "refused-proof";

export function applyNewcomerOutputRoute(
    env: NewcomerRouteEnv,
    newcomerRef: object,
    observedOutput: string,
    target: NewcomerRouteDomain,
): NewcomerRouteOutcome {
    let outputObj: object | null = null;
    try {
        outputObj = env.resolveOutput?.(target.output) ?? null;
    } catch (error) {
        void error;
        outputObj = null;
    }
    if (outputObj === null) {
        return "refused-no-output";
    }
    let desktopObj: object | null = null;
    try {
        desktopObj = env.resolveDesktop?.(target.workspace) ?? null;
    } catch (error) {
        void error;
        desktopObj = null;
    }
    if (desktopObj === null) {
        return "refused-no-desktop";
    }
    // Activation race: only a newcomer still on its observed output, with
    // readable membership, is ours to route.
    let stillThere: string | null = null;
    let membership: ReadonlyArray<string> | null = null;
    try {
        stillThere = env.readOutputName?.(newcomerRef) ?? null;
        membership = env.readDesktopIds?.(newcomerRef) ?? null;
    } catch (error) {
        void error;
        stillThere = null;
        membership = null;
    }
    if (stillThere === null || stillThere !== observedOutput || membership === null) {
        return "refused-race";
    }
    // `Window.output` is read-only; the transfer primitive is required. A
    // membership-only move cannot change outputs, so its absence refuses
    // before any write.
    if (env.sendClientToScreen === undefined) {
        return "refused-no-transfer";
    }
    let sent = false;
    try {
        sent = env.sendClientToScreen(newcomerRef, outputObj) === true;
    } catch (error) {
        void error;
        sent = false;
    }
    if (!sent) {
        return "refused-send";
    }
    let homed = false;
    try {
        homed = env.setDesktops?.(newcomerRef, [desktopObj]) === true;
    } catch (error) {
        void error;
        homed = false;
    }
    if (!homed) {
        return "refused-desktops";
    }
    // Proof before focus: target membership plus still a non-overlay window.
    let proof: { readonly output: string; readonly workspace: string; readonly fullscreen: boolean; readonly maximized: boolean } | null = null;
    try {
        proof = env.reobserve?.(newcomerRef) ?? null;
    } catch (error) {
        void error;
        proof = null;
    }
    if (
        proof === null ||
        proof.output !== target.output ||
        proof.workspace !== target.workspace ||
        proof.fullscreen ||
        proof.maximized
    ) {
        return "refused-proof";
    }
    let focused = false;
    try {
        focused = env.setActive?.(newcomerRef) === true;
    } catch (error) {
        void error;
        focused = false;
    }
    return focused ? "routed" : "routed-focus-failed";
}
