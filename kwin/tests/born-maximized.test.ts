import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    BORN_MAXIMIZED_DEFAULT,
    BORN_MAXIMIZED_OVERLAY,
    normalizeBornMaximized,
    readBornMaximizedValue,
} from "../src/born-maximized";
import {
    PlanAdapter,
    type PlanAdapterEnv,
    type PlanObserved,
} from "../src/plan-adapter";
import type { MaximizeClearOutcome } from "../src/plan-adapter";
import { startPlanAdapterEntry } from "../src/plan-adapter-entry";

// R-MAX-06 KDE `bornMaximized` uses the `bornMaximized` script/kwinrc key,
// following the existing camelCase convention (`sameAxisMove` <->
// `core.same_axis_move`): unknown or missing values fall back to the
// `tile-on-launch` default, matching the Windows `core.born_maximized`
// tokens exactly.

function entryXml(path: string): string {
    return readFileSync(path, "utf8");
}

describe("born-maximized setting validation", () => {
    it("defaults to tile-on-launch", () => {
        assert.equal(BORN_MAXIMIZED_DEFAULT, "tile-on-launch");
        assert.equal(BORN_MAXIMIZED_OVERLAY, "keep-maximized-overlay");
        assert.equal(readBornMaximizedValue(), "tile-on-launch");
    });

    it("accepts exactly tile-on-launch and keep-maximized-overlay while rejecting the rest to tile-on-launch", () => {
        assert.equal(normalizeBornMaximized("tile-on-launch"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("keep-maximized-overlay"), "keep-maximized-overlay");
        assert.equal(normalizeBornMaximized(undefined), "tile-on-launch");
        assert.equal(normalizeBornMaximized(null), "tile-on-launch");
        assert.equal(normalizeBornMaximized(""), "tile-on-launch");
        assert.equal(normalizeBornMaximized("tile"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("COSMIC"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("keep_maximized_overlay"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("bornMaximized"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("tile-on-launch "), "tile-on-launch");
        assert.equal(normalizeBornMaximized(0), "tile-on-launch");
        assert.equal(readBornMaximizedValue(() => "keep-maximized-overlay"), "keep-maximized-overlay");
        assert.equal(readBornMaximizedValue(() => "bogus"), "tile-on-launch");
        assert.equal(
            readBornMaximizedValue(() => {
                throw new Error("boom");
            }),
            "tile-on-launch",
        );
    });
});

describe("born-maximized schema and native KCM persistence", () => {
    it("declares bornMaximized as an Enum setting defaulting to tile-on-launch", () => {
        const schema = entryXml("contents/config/main.xml");
        const match = schema.match(/<entry name="bornMaximized" type="Enum">([\s\S]*?)<\/entry>/);
        assert.ok(match !== null, "bornMaximized must be declared");
        const body = match[1] as string;
        assert.match(body, /<default>tile-on-launch<\/default>/);
        assert.match(body, /<choice name="tile-on-launch" value="tile-on-launch"\/>/);
        assert.match(body, /<choice name="keep-maximized-overlay" value="keep-maximized-overlay"\/>/);
        const ui = entryXml("contents/ui/config.ui");
        assert.match(ui, /name="kcfg_bornMaximized"/);
    });

    it("persists bornMaximized through the native script KCM with validation", () => {
        const module = entryXml("native-effect/unifiedsettings_module.cpp");
        const adapter = entryXml("native-effect/kdesettings_adapter.cpp");
        const descriptors = entryXml("native-effect/generated/kde_settings_descriptors.h");
        const ui = entryXml("native-effect/unifiedsettings.ui");
        assert.match(ui, /name="bornMaximizedCombo"/);
        assert.match(ui, /name="label_bornMaximized"[\s\S]*?bornMaximizedCombo/);
        assert.match(descriptors, /"bornMaximized", "bornMaximizedCombo"/);
        assert.match(descriptors, /kli18n\("Tile on launch"\)/);
        assert.match(descriptors, /kli18n\("Keep maximized overlay"\)/);
        assert.match(descriptors, /kli18n\("bspwm, i3, xmonad, sway, qtile"\)/);
        assert.match(descriptors, /kli18n\("COSMIC, Hyprland"\)/);
        assert.match(adapter, /readEnumToken/);
        assert.match(module, /KdeSettingsAdapter::configureNativeControls\(widget\(\)\)/);
        assert.match(module, /KdeSettingsAdapter::readEnumToken\(/);
        assert.match(module, /writeEntry\(QStringLiteral\("bornMaximized"\)/);
        assert.match(module, /KdeSettingsAdapter::selectEnumCombo\(/);
    });
});

function makeRefs(): { a: object; b: object } {
    return { a: {}, b: {} };
}

interface BornObservedOpts {
    maximizedB?: boolean;
    fullscreenB?: boolean;
    floatingB?: boolean;
    stickyB?: boolean;
    fingerprint?: string;
}

function makeObserved(refs: { a: object; b: object }, opts: BornObservedOpts = {}): PlanObserved {
    return {
        domainOutput: "out-1",
        domainWorkspace: "ws-1",
        domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
        domainGap: 0,
        domainOuterGap: 0,
        focusedId: "win-a",
        windows: Object.freeze([
            Object.freeze({
                id: "win-a",
                ref: refs.a,
                rect: Object.freeze({ x: 0, y: 0, w: 600, h: 800 }),
                output: "out-1",
                workspace: "ws-1",
                fullscreen: false,
                maximized: false,
                floating: false,
                sticky: false,
                resourceClass: "unknown",
            }),
            Object.freeze({
                id: "win-b",
                ref: refs.b,
                rect: Object.freeze({ x: 600, y: 0, w: 600, h: 800 }),
                output: "out-1",
                workspace: "ws-1",
                fullscreen: opts.fullscreenB === true,
                maximized: opts.maximizedB === true,
                floating: opts.floatingB === true,
                sticky: opts.stickyB === true,
                resourceClass: "unknown",
            }),
        ]) as unknown as PlanObserved["windows"],
        activeRef: refs.a,
        fingerprint: opts.fingerprint ?? "fp-born-1",
        revalidate: () => true,
    };
}

interface BornMocks {
    readonly dbusCalls: Array<{ payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    readonly logs: string[];
    readonly subscribes: Array<{ kind: string; handler: (target?: object) => void }>;
    readonly maximizeClears: Array<object>;
    observeImpl: () => PlanObserved | null;
    bornMode: unknown;
    floatingFirst: (id: string) => boolean;
    fixedHints: boolean;
    env: PlanAdapterEnv;
}

function mockBornEnv(refs: { a: object; b: object }): BornMocks {
    const state = {
        dbusCalls: [],
        callbacks: [],
        timers: [],
        logs: [],
        subscribes: [],
        maximizeClears: [],
        observeImpl: (): PlanObserved | null => makeObserved(refs, { maximizedB: true }),
        bornMode: undefined as unknown,
        floatingFirst: (_id: string): boolean => false,
        fixedHints: false,
        env: null as unknown as PlanAdapterEnv,
    } as BornMocks;
    const env: PlanAdapterEnv = {
        callDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method === "NameHasOwner") {
                callback(true);
                return;
            }
            if (method === "GetNameOwner") {
                callback(":1.7");
                return;
            }
            if (method === "StartServiceByName") {
                callback(1);
                return;
            }
            state.dbusCalls.push({ payload });
            state.callbacks.push(callback);
        },
        scheduleOnce: (delayMs, callback): (() => void) => {
            const entry = { delayMs, callback, cancelled: false };
            state.timers.push(entry);
            return (): void => {
                entry.cancelled = true;
            };
        },
        log: (message): void => {
            state.logs.push(message);
        },
        observe: (): PlanObserved | null => state.observeImpl(),
        observeHidden: (): ReadonlyArray<PlanObserved> => [],
        clearMaximize: (target): MaximizeClearOutcome => {
            state.maximizeClears.push(target);
            return "invoked";
        },
        setGeometry: (): boolean => true,
        setActive: (): boolean => true,
        active: (): object | null => refs.a,
        subscribe: (kind, handler): (() => void) => {
            state.subscribes.push({ kind, handler });
            return (): void => {};
        },
        readBornMaximized: (): unknown => state.bornMode,
        isFloatingFirst: (id: string): boolean => state.floatingFirst(id),
        readWindowConstraints: (): { resizeable: boolean | null; minSize: { w: number; h: number } | null; maxSize: { w: number; h: number } | null } | null => {
            if (!state.fixedHints) {
                return null;
            }
            return { resizeable: true, minSize: { w: 640, h: 480 }, maxSize: { w: 640, h: 480 } };
        },
    };
    (state as { env: PlanAdapterEnv }).env = env;
    return state;
}

function fire(mocks: BornMocks, kind: string, target?: object): void {
    for (const sub of mocks.subscribes) {
        if (sub.kind === kind) {
            sub.handler(target);
        }
    }
}

function runTimers(mocks: BornMocks): void {
    const pending = [...mocks.timers];
    mocks.timers.length = 0;
    for (const timer of pending) {
        if (!timer.cancelled) {
            timer.callback();
        }
    }
}

function plannerPayload(mocks: BornMocks, index: number): Record<string, unknown> {
    return JSON.parse(mocks.dbusCalls[index]?.payload as string) as Record<string, unknown>;
}

function wireWindow(payload: Record<string, unknown>, id: string): Record<string, unknown> | undefined {
    return (payload["windows"] as Array<Record<string, unknown>>).find((entry) => entry["window"] === id);
}

describe("born-maximized admission routing", () => {
    it("clears a born maximized window once under tile-on-launch, then admits it ordinarily", () => {
        const refs = makeRefs();
        const mocks = mockBornEnv(refs);
        let maximized = true;
        mocks.observeImpl = () => makeObserved(refs, { maximizedB: maximized });
        // readBornMaximized absent (undefined) resolves to the tile-on-launch default.
        mocks.bornMode = undefined;
        const adapter = new PlanAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.maximizeClears.length, 1);
        assert.equal(mocks.maximizeClears[0], refs.b);
        assert.ok(
            mocks.logs.some((line) =>
                line === "omnitiler:plan:maximize-admission-clear window=win-b resource_class=unknown outcome=issued"
            ),
        );
        // The refetch after the clear sees the restored window, so the
        // dispatch admits it as an ordinary tile (no floating flag).
        maximized = false;
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.maximizeClears.length, 1, "the one-shot clear never repeats");
        assert.ok(mocks.dbusCalls.length > 0);
        const admitted = wireWindow(plannerPayload(mocks, mocks.dbusCalls.length - 1), "win-b");
        assert.ok(admitted !== undefined);
        assert.ok(admitted["floating"] !== true, "cleared born window admits ordinarily");
    });

    it("keeps the maximized overlay with no clear under keep-maximized-overlay", () => {
        const refs = makeRefs();
        const mocks = mockBornEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { maximizedB: true });
        mocks.bornMode = "keep-maximized-overlay";
        const adapter = new PlanAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.maximizeClears.length, 0);
        assert.ok(mocks.dbusCalls.length > 0);
        const overlay = wireWindow(plannerPayload(mocks, mocks.dbusCalls.length - 1), "win-b");
        assert.ok(overlay !== undefined);
        assert.equal(overlay["fit_excluded"], true, "overlay window keeps its reserved slot");
    });

    it("resolves an invalid mode to tile-on-launch", () => {
        const refs = makeRefs();
        const mocks = mockBornEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { maximizedB: true });
        mocks.bornMode = "bogus";
        const adapter = new PlanAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.maximizeClears.length, 1);
    });

    it("leaves D4 fixed-size max births floating with no clear under tile-on-launch", () => {
        const refs = makeRefs();
        const mocks = mockBornEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { maximizedB: true });
        mocks.bornMode = "tile-on-launch";
        mocks.fixedHints = true;
        const adapter = new PlanAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.maximizeClears.length, 0);
        assert.ok(
            mocks.logs.some((line) =>
                line === "omnitiler:plan:maximize-admission-clear window=win-b resource_class=unknown outcome=skipped-fixed"
            ),
        );
    });

    it("leaves fullscreen, floating, and sticky maximized windows untouched under tile-on-launch", () => {
        for (const opts of [
            { fullscreenB: true, maximizedB: true },
            { floatingB: true, maximizedB: true },
            { stickyB: true, maximizedB: true },
        ]) {
            const refs = makeRefs();
            const mocks = mockBornEnv(refs);
            mocks.observeImpl = () => makeObserved(refs, opts);
            mocks.bornMode = "tile-on-launch";
            const adapter = new PlanAdapter(mocks.env);
            assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
            fire(mocks, "added");
            runTimers(mocks);
            assert.equal(mocks.maximizeClears.length, 0, `no clear for ${JSON.stringify(opts)}`);
        }
    });

    it("keeps R-MAX-03 floating-first ids overlaid under tile-on-launch", () => {
        const refs = makeRefs();
        const mocks = mockBornEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { maximizedB: true });
        mocks.bornMode = "tile-on-launch";
        mocks.floatingFirst = (id): boolean => id === "win-b";
        const adapter = new PlanAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.maximizeClears.length, 0);
    });

    it("leaves a later maximize of an already-tiled window untouched under tile-on-launch", () => {
        const refs = makeRefs();
        const mocks = mockBornEnv(refs);
        let maximized = false;
        mocks.observeImpl = () => makeObserved(refs, { maximizedB: maximized, fingerprint: "fp-born-tiled" });
        mocks.bornMode = "tile-on-launch";
        const adapter = new PlanAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        fire(mocks, "added");
        runTimers(mocks);
        assert.ok(mocks.dbusCalls.length > 0);
        // Settle the tiled admission so the window carries applied evidence.
        const correlation = plannerPayload(mocks, mocks.dbusCalls.length - 1)["correlation_id"] as string;
        mocks.callbacks[mocks.callbacks.length - 1]?.(
            JSON.stringify({
                v: 1,
                correlation_id: correlation,
                outcome: "planned",
                base_revision: 2,
                detail: { kind: "focus" },
                desired_geometry: [
                    { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-a-leaf" },
            }),
        );
        // A later maximize of the retained tile keeps its overlay: no clear.
        maximized = true;
        mocks.observeImpl = () => makeObserved(refs, { maximizedB: maximized, fingerprint: "fp-born-later-max" });
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.maximizeClears.length, 0);
    });
});

describe("born-maximized live reread on Options configChanged", () => {
    it("updates subsequent admissions without rebuild or reclassification work", () => {
        let mode: unknown = "tile-on-launch";
        const handlers: Array<() => void> = [];
        const options = {
            configChanged: {
                connect: (handler: () => void): void => { handlers.push(handler); },
                disconnect: (handler: () => void): void => {
                    const index = handlers.indexOf(handler);
                    if (index >= 0) { handlers.splice(index, 1); }
                },
            },
        };
        const logs: string[] = [];
        const output: Record<string, unknown> = { name: "out-1" };
        const desktop: Record<string, unknown> = { id: "ws-1" };
        const inert = (): unknown => ({ connect: (): void => {}, disconnect: (): void => {} });
        const winA: Record<string, unknown> = {
            normalWindow: true,
            internalId: "win-a",
            resourceClass: "test-app",
            output,
            desktops: [desktop],
            frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
            moveResizedChanged: inert(),
            fullScreenChanged: inert(),
            fullScreen: false,
            maximizedChanged: inert(),
            maximizeMode: 0,
            desktopsChanged: inert(),
            onAllDesktops: false,
        };
        const workspace = {
            activeWindow: winA,
            windowList: (): unknown[] => [winA],
            screens: [output],
            currentDesktopForScreen: (): unknown => desktop,
            clientArea: (): unknown => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowAdded: inert(),
            windowRemoved: inert(),
            windowActivated: inert(),
            screensChanged: inert(),
            currentDesktopChanged: inert(),
        };
        const dbusCalls: Array<{ method: string; payload: string }> = [];
        const handle = startPlanAdapterEntry({
            workspace,
            options,
            callDbus: (_s, _p, _i, method, _payload, callback): void => {
                if (method === "NameHasOwner") { callback(true); return; }
                if (method === "GetNameOwner") { callback(":1.7"); return; }
                if (method === "StartServiceByName") { callback(1); return; }
                dbusCalls.push({ method, payload: _payload });
                void callback;
            },
            scheduleOnce: () => () => {},
            log: (message): void => { logs.push(message); },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: () => true,
            readProfileFn: () => "cosmic",
            readBornMaximizedFn: (): unknown => mode,
        });
        assert.ok(handle !== null);
        mode = "keep-maximized-overlay";
        for (const fire of [...handlers]) {
            fire();
        }
        assert.ok(
            logs.some((line) => line === "omnitiler:plan:config-reloaded stage=born-maximized mode=keep-maximized-overlay"),
        );
        // Subsequent admissions only: no shortcut re-registration, no resync
        // dispatch, no restart or gap reload lines.
        assert.equal(dbusCalls.length, 0);
        assert.ok(!logs.some((line) => line.includes("restart-required")));
        assert.ok(!logs.some((line) => line.includes("re-read-queued")));
        assert.ok(!logs.some((line) => line.includes("default-tiled")));
        handle?.stop();
    });

    it("ignores an unchanged configChanged without reload lines", () => {
        let mode: unknown = "tile-on-launch";
        const handlers: Array<() => void> = [];
        const options = {
            configChanged: {
                connect: (handler: () => void): void => { handlers.push(handler); },
                disconnect: (handler: () => void): void => {
                    const index = handlers.indexOf(handler);
                    if (index >= 0) { handlers.splice(index, 1); }
                },
            },
        };
        const logs: string[] = [];
        const output: Record<string, unknown> = { name: "out-1" };
        const desktop: Record<string, unknown> = { id: "ws-1" };
        const inert = (): unknown => ({ connect: (): void => {}, disconnect: (): void => {} });
        const winA: Record<string, unknown> = {
            normalWindow: true,
            internalId: "win-a",
            resourceClass: "test-app",
            output,
            desktops: [desktop],
            frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
            moveResizedChanged: inert(),
            fullScreenChanged: inert(),
            fullScreen: false,
            maximizedChanged: inert(),
            maximizeMode: 0,
            desktopsChanged: inert(),
            onAllDesktops: false,
        };
        const workspace = {
            activeWindow: winA,
            windowList: (): unknown[] => [winA],
            screens: [output],
            currentDesktopForScreen: (): unknown => desktop,
            clientArea: (): unknown => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowAdded: inert(),
            windowRemoved: inert(),
            windowActivated: inert(),
            screensChanged: inert(),
            currentDesktopChanged: inert(),
        };
        const handle = startPlanAdapterEntry({
            workspace,
            options,
            callDbus: (_s, _p, _i, method, _payload, callback): void => {
                if (method === "NameHasOwner") { callback(true); return; }
                if (method === "GetNameOwner") { callback(":1.7"); return; }
                if (method === "StartServiceByName") { callback(1); return; }
                void callback;
            },
            scheduleOnce: () => () => {},
            log: (message): void => { logs.push(message); },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: () => true,
            readProfileFn: () => "cosmic",
            readBornMaximizedFn: (): unknown => mode,
        });
        assert.ok(handle !== null);
        void mode;
        for (const fire of [...handlers]) {
            fire();
        }
        assert.ok(!logs.some((line) => line.includes("omnitiler:plan:config-reloaded stage=born-maximized")));
        handle?.stop();
    });
});

interface BornFixtureCase {
    name: string;
    raw?: unknown;
    normalized: string;
    should_clear: boolean;
}

// Shared R-MAX-06 conformance table (also consumed by tiler-core
// `born_maximized` unit tests): each row pins the raw setting, the
// normalized token, and the clear-vs-keep outcome for an ordinary eligible
// tiled-first born-maximized admission. The adapter half exercises the real
// admission outcome (clearMaximize call count), not only the normalizer.
describe("born-maximized shared conformance fixture", () => {
    const fixture = JSON.parse(
        readFileSync("../test-fixtures/born-maximized.json", "utf8"),
    ) as { cases: BornFixtureCase[] };
    assert.ok(fixture.cases.length > 0, "fixture carries a case table");
    for (const kase of fixture.cases) {
        it(`${kase.name}: normalizes to ${kase.normalized}`, () => {
            const raw = "raw" in kase ? kase.raw : undefined;
            assert.equal(normalizeBornMaximized(raw), kase.normalized);
        });
        it(`${kase.name}: adapter ${kase.should_clear ? "clears once" : "keeps the overlay"}`, () => {
            const raw = "raw" in kase ? kase.raw : undefined;
            const refs = makeRefs();
            const mocks = mockBornEnv(refs);
            mocks.observeImpl = () => makeObserved(refs, { maximizedB: true });
            mocks.bornMode = raw;
            const adapter = new PlanAdapter(mocks.env);
            assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
            fire(mocks, "added");
            runTimers(mocks);
            assert.equal(mocks.maximizeClears.length, kase.should_clear ? 1 : 0, kase.name);
        });
    }
});
