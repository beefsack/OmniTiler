// Development-only isolated fixture settings source:
// - exact OmniTilerTestWindow scope + __OMNITILER_TRACE__ === "1" guard
// - isolated defaults/snapshot without reading user kwinrc (readConfig)
// - genuine v4 settings wire ingress (focusFollowsCursor)
// - reload/re-handshake preservation across generation reset
// - fail-closed on malformed JSON, unknown keys, wrong types, wrong scope
// - production stripping on ordinary and installed builds

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";

import { startWorldAdapterEntry } from "../src/world-entry";
import { createWorldSettingsLive } from "../src/world-settings-live";

interface CapturedCall {
    service: string;
    path: string;
    iface: string;
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

function makeHarness(opts: {
    scopeClass?: string | readonly string[];
    devSettingsJson?: string;
    readConfigFn?: (key: string, fallback?: unknown) => unknown;
}) {
    const ws1 = { id: "ws1" };
    const winA = {
        internalId: "win-a",
        resourceClass: "OmniTilerTestWindow",
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        dialog: false,
        modal: false,
        wantsInput: true,
        normalWindow: true,
        output: { name: "out" },
        desktops: [ws1],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
    };
    const surface = {
        windowList: () => [winA],
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1],
        activeScreen: { name: "out" },
        activeWindow: winA,
        currentDesktopForScreen: () => ws1,
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1080 }),
        windowAdded: { connect: () => {}, disconnect: () => {} },
        windowRemoved: { connect: () => {}, disconnect: () => {} },
        windowActivated: { connect: () => {}, disconnect: () => {} },
        currentDesktopChanged: { connect: () => {}, disconnect: () => {} },
        screensChanged: { connect: () => {}, disconnect: () => {} },
        virtualDesktopsChanged: { connect: () => {}, disconnect: () => {} },
    };

    const calls: CapturedCall[] = [];
    let latestWaitWorld: CapturedCall | null = null;
    const logs: string[] = [];
    let currentGen = "gen-1";

    const origReadConfig = (globalThis as { readConfig?: unknown }).readConfig;
    if (opts.readConfigFn !== undefined) {
        (globalThis as { readConfig?: unknown }).readConfig = opts.readConfigFn;
    }

    let handle: ReturnType<typeof startWorldAdapterEntry> = null;
    try {
        handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "test-owner",
            adapter: "test-adapter",
            ...(opts.scopeClass !== undefined ? { scopeClass: opts.scopeClass } : {}),
            ...(opts.devSettingsJson !== undefined ? { devSettingsJson: opts.devSettingsJson } : {}),
            callDbus: (service, path, iface, method, payload, callback) => {
                if (method === "ReadFloatIntent") {
                    const body = JSON.parse(payload) as { correlation_id?: string };
                    callback(JSON.stringify({ v: 2, correlation_id: body.correlation_id ?? "1", ok: true, entries: [] }));
                    return;
                }
                if (method === "ReadNativePreimage") {
                    const body = JSON.parse(payload) as { correlation_id?: string };
                    callback(JSON.stringify({ v: 1, correlation_id: body.correlation_id ?? "1", ok: true, entries: [] }));
                    return;
                }
                if (method === "DiscoverWorld") {
                    const msg = JSON.parse(payload) as { type: string; req?: string; gen?: string; rev?: string };
                    if (msg.type === "handshake") {
                        calls.push({ service, path, iface, method, payload, callback });
                        callback(JSON.stringify({
                            type: "handshake-reply",
                            v: 4,
                            adapter: "test-adapter",
                            req: msg.req ?? "1",
                            ready: false,
                            kind: "",
                            gen: currentGen,
                        }));
                        return;
                    }
                    if (msg.type === "settings") {
                        calls.push({ service, path, iface, method, payload, callback });
                        callback(JSON.stringify({
                            type: "settings-reply",
                            v: 4,
                            adapter: "test-adapter",
                            req: msg.req ?? "1",
                            gen: msg.gen ?? currentGen,
                            rev: msg.rev ?? "1",
                            status: "accepted",
                            diagnostics: [],
                        }));
                        return;
                    }
                }
                if (method === "WaitWorldWake") {
                    latestWaitWorld = { service, path, iface, method, payload, callback };
                    return;
                }
                calls.push({ service, path, iface, method, payload, callback });
            },
            registerShortcutFn: () => true,
            scheduleOnce: () => () => {},
            log: (line) => logs.push(line),
        });
    } finally {
        if (opts.readConfigFn !== undefined) {
            if (origReadConfig === undefined) {
                delete (globalThis as { readConfig?: unknown }).readConfig;
            } else {
                (globalThis as { readConfig?: unknown }).readConfig = origReadConfig;
            }
        }
    }

    return {
        handle,
        calls,
        logs,
        setGen: (gen: string) => {
            currentGen = gen;
        },
        triggerGenerationMismatch: () => {
            assert.ok(latestWaitWorld !== null, "WaitWorldWake must be armed");
            const cb = latestWaitWorld.callback;
            latestWaitWorld = null;
            cb(JSON.stringify({
                v: 4,
                outcome: "rejected",
                kind: "generation-mismatch",
                source: "planner",
                message: "stale generation",
                gen: currentGen,
                clock: "1",
                nativeInvalid: "0",
                gestureAssoc: null,
                gestureSettledAssoc: null,
            }));
        },
        settingsRequests: () =>
            calls
                .filter((c) => c.method === "DiscoverWorld")
                .map((c) => JSON.parse(c.payload) as { type: string; gen?: string; rev?: string; values?: Record<string, unknown> })
                .filter((m) => m.type === "settings"),
    };
}

describe("isolated dev settings source", () => {
    it("sends isolated hover snapshot over v4 settings wire without reading user kwinrc", () => {
        let readConfigCalled = 0;
        const h = makeHarness({
            scopeClass: "OmniTilerTestWindow",
            devSettingsJson: JSON.stringify({ focusFollowsCursor: true }),
            readConfigFn: () => {
                readConfigCalled += 1;
                throw new Error("must not read user kwinrc");
            },
        });
        assert.ok(h.handle !== null, "entry must start with valid isolated settings");
        assert.equal(readConfigCalled, 0, "user readConfig must never be called");
        const reqs = h.settingsRequests();
        assert.equal(reqs.length, 1);
        assert.deepEqual(reqs[0]?.values, {
            gap: 8,
            outer: 8,
            defaultTiled: true,
            mode: "per-output-local",
            sameAxis: "group-with-neighbor",
            bornMaximized: "tile-on-launch",
            predicate: "both-axes-fixed",
            refill: "last-remaining-workspace",
            focusFollowsCursor: true,
        });
        h.handle.stop();
    });

    it("preserves isolated settings across generation reset (Planner reload re-handshake)", () => {
        const h = makeHarness({
            scopeClass: "OmniTilerTestWindow",
            devSettingsJson: JSON.stringify({ focusFollowsCursor: true, gap: 12 }),
            readConfigFn: () => {
                throw new Error("must not read user kwinrc");
            },
        });
        assert.ok(h.handle !== null);
        assert.equal(h.settingsRequests().length, 1);
        assert.equal(h.settingsRequests()[0]?.gen, "gen-1");

        // Simulate Planner reload: new generation gen-2 triggers re-handshake.
        h.setGen("gen-2");
        h.triggerGenerationMismatch();
        const reqs = h.settingsRequests();
        assert.equal(reqs.length, 2);
        assert.equal(reqs[1]?.gen, "gen-2");
        assert.equal(reqs[1]?.values?.["focusFollowsCursor"], true);
        assert.equal(reqs[1]?.values?.["gap"], 12);
        h.handle.stop();
    });

    it("refuses isolated settings when scope is empty, foreign, or mixed", () => {
        for (const scope of [[], ["kwrite"], ["OmniTilerTestWindow", "kwrite"]]) {
            const h = makeHarness({
                scopeClass: scope,
                devSettingsJson: JSON.stringify({ focusFollowsCursor: true }),
            });
            assert.equal(h.handle, null, `scope ${JSON.stringify(scope)} must fail closed`);
            assert.ok(h.logs.some((l) => l.includes("omnitiler:world:dev-settings-invalid")));
        }
    });

    it("fails closed on malformed JSON, unknown keys, wrong types, or out-of-range values", () => {
        const badPayloads = [
            "{not-json",
            "[]",
            "null",
            JSON.stringify({ unknownKey: true }),
            JSON.stringify({ focusFollowsCursor: "true" }),
            JSON.stringify({ focusFollowsCursor: 1 }),
            JSON.stringify({ gap: -1 }),
            JSON.stringify({ gap: 65 }),
            JSON.stringify({ gap: 1.5 }),
            JSON.stringify({ mode: "invalid-mode" }),
            JSON.stringify({ defaultTiled: "yes" }),
        ];
        for (const payload of badPayloads) {
            let readConfigCalled = 0;
            const h = makeHarness({
                scopeClass: "OmniTilerTestWindow",
                devSettingsJson: payload,
                readConfigFn: () => {
                    readConfigCalled += 1;
                    return true;
                },
            });
            assert.equal(h.handle, null, `payload ${payload} must fail closed`);
            assert.equal(readConfigCalled, 0, "must never fall back to user readConfig");
            assert.ok(h.logs.some((l) => l.includes("omnitiler:world:dev-settings-invalid")));
        }
    });

    it("ordinary path without isolated settings omits focusFollowsCursor and uses readConfig", () => {
        const origReadConfig = (globalThis as { readConfig?: unknown }).readConfig;
        const readKeys: string[] = [];
        (globalThis as { readConfig?: unknown }).readConfig = (key: string, fallback: unknown) => {
            readKeys.push(key);
            return fallback;
        };
        try {
            const live = createWorldSettingsLive();
            const pairs = live.snapshotConnectionValues();
            assert.ok(!pairs.some(([k]) => k === "focusFollowsCursor"));
            assert.ok(readKeys.length > 0);
        } finally {
            if (origReadConfig === undefined) {
                delete (globalThis as { readConfig?: unknown }).readConfig;
            } else {
                (globalThis as { readConfig?: unknown }).readConfig = origReadConfig;
            }
        }
    });

    it("strips isolated dev settings code from ordinary and installed builds", () => {
        const manifest = JSON.parse(readFileSync("package.json", "utf8")) as { scripts: Record<string, string> };
        assert.match(manifest.scripts["build"] ?? "", /__OMNITILER_DEV_SETTINGS__/);
        assert.doesNotMatch(manifest.scripts["build"] ?? "", /OMNITILER_DEV_SETTINGS_LITERAL/);
        assert.match(manifest.scripts["build:installed"] ?? "", /__OMNITILER_DEV_SETTINGS__/);
        assert.doesNotMatch(manifest.scripts["build:installed"] ?? "", /OMNITILER_DEV_SETTINGS_LITERAL/);
        assert.match(manifest.scripts["build:trace"] ?? "", /OMNITILER_DEV_SETTINGS_LITERAL/);

        const outDir = mkdtempSync(join(tmpdir(), "pat-dev-settings-"));
        try {
            for (const [label, defines] of [
                ["build", ['--define:__OMNITILER_TRACE__="0"', '--define:__OMNITILER_SCOPE_CLASS__=""', '--define:__OMNITILER_DEV_SETTINGS__=""']],
                ["build:installed", ['--define:__OMNITILER_TRACE__="0"', '--define:__OMNITILER_SCOPE_CLASS__=""', '--define:__OMNITILER_DEV_SETTINGS__=""']],
                ["non-trace-with-scope", ['--define:__OMNITILER_TRACE__="0"', '--define:__OMNITILER_SCOPE_CLASS__="OmniTilerTestWindow"', '--define:__OMNITILER_DEV_SETTINGS__=""']],
            ] as const) {
                const outFile = join(outDir, `${label}.js`);
                execFileSync(
                    "./node_modules/.bin/esbuild",
                    ["src/entry.ts", "--bundle", "--format=iife", "--target=es2017", "--minify-syntax", ...defines, `--outfile=${outFile}`],
                    { encoding: "utf8" },
                );
                const bundle = readFileSync(outFile, "utf8");
                assert.ok(!bundle.includes("__OMNITILER_DEV_SETTINGS__"), `${label} must fold __OMNITILER_DEV_SETTINGS__`);
                assert.ok(!bundle.includes("dev-settings-invalid"), `${label} must strip dev-settings-invalid`);
                assert.ok(!bundle.includes("parseIsolatedSettingsSnapshot"), `${label} must strip parseIsolatedSettingsSnapshot`);
            }
        } finally {
            rmSync(outDir, { recursive: true, force: true });
        }
    });
});
