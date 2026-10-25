// Dev/test admission scope (platform-only): exact `resourceClass` filter
// with unchanged empty behavior, plus launcher/build plumbing parity.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createContext, runInContext } from "node:vm";
import { normalizeScopeClassList, windowMatchesScopeClass } from "../src/scope-class";
import { captureWorldObservation } from "../src/world-observe";
import { startWorldAdapterEntry } from "../src/world-entry";

function readProp(value: object, property: string): unknown {
    return Reflect.get(value, property);
}

function fakeRef(resourceClass: unknown): object {
    return { resourceClass };
}

function observe(scope: string | readonly string[] | undefined, wins: Array<{ id: string; resourceClass?: unknown }>, activeId: string) {
    const desktops = [{ id: "ws-1" }];
    const screens = [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const refs = wins.map((w) => ({
        internalId: w.id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        output: { name: "out-1" },
        desktops: [{ id: "ws-1" }],
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
        ...(w.resourceClass !== undefined ? { resourceClass: w.resourceClass } : {}),
    }));
    const active = refs.find((r) => r.internalId === activeId) ?? null;
    const surface = {
        windowList: () => refs,
        screens,
        desktops,
        activeScreen: { name: "out-1" },
        activeWindow: active,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => desktops[0],
    };
    return captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-1", pin: false, disp: false, managed: true }),
        isFloating: () => false,
        ...(scope !== undefined ? { scopeClass: scope } : {}),
    });
}

describe("scope-class normalization", () => {
    it("treats missing and empty as the unchanged whole world", () => {
        assert.deepEqual(normalizeScopeClassList(undefined), []);
        assert.deepEqual(normalizeScopeClassList(null), []);
        assert.deepEqual(normalizeScopeClassList(""), []);
        assert.deepEqual(normalizeScopeClassList([]), []);
    });

    it("accepts single and comma-joined identities, collapsing duplicates", () => {
        assert.deepEqual(normalizeScopeClassList("OmniTilerTestWindow"), ["OmniTilerTestWindow"]);
        assert.deepEqual(normalizeScopeClassList("OmniTilerTestWindow,kwrite"), ["OmniTilerTestWindow", "kwrite"]);
        assert.deepEqual(normalizeScopeClassList(["org.kde.konsole", "a1-._b"]), ["org.kde.konsole", "a1-._b"]);
        assert.deepEqual(normalizeScopeClassList("kwrite,kwrite"), ["kwrite"]);
        assert.deepEqual(normalizeScopeClassList("x".repeat(128)), ["x".repeat(128)]);
    });

    it("rejects any malformed entry fail-closed", () => {
        assert.equal(normalizeScopeClassList("has space"), null);
        assert.equal(normalizeScopeClassList("OmniTilerTestWindow,has space"), null);
        assert.equal(normalizeScopeClassList("wild*card"), null);
        assert.equal(normalizeScopeClassList("ok,bad!"), null);
        assert.equal(normalizeScopeClassList("-leading-dash"), null);
        assert.equal(normalizeScopeClassList(".leading-dot"), null);
        assert.equal(normalizeScopeClassList("trailing/"), null);
        assert.equal(normalizeScopeClassList("ok,"), null);
        assert.equal(normalizeScopeClassList(",ok"), null);
        assert.equal(normalizeScopeClassList("ok,,ok"), null);
        assert.equal(normalizeScopeClassList(","), null);
        assert.equal(normalizeScopeClassList(42), null);
        assert.equal(normalizeScopeClassList(["ok", 42]), null);
        assert.equal(normalizeScopeClassList(["ok", ""]), null);
        assert.equal(normalizeScopeClassList("x".repeat(129)), null);
    });
});

describe("scope-class window matching", () => {
    it("matches everything when empty, regardless of identity", () => {
        assert.equal(windowMatchesScopeClass(readProp, fakeRef("firefox"), []), true);
        assert.equal(windowMatchesScopeClass(readProp, fakeRef(undefined), []), true);
        assert.equal(windowMatchesScopeClass(readProp, {}, []), true);
    });

    it("matches only the exact resourceClass identities", () => {
        const scope = ["OmniTilerTestWindow", "kwrite"] as const;
        assert.equal(windowMatchesScopeClass(readProp, fakeRef("OmniTilerTestWindow"), scope), true);
        assert.equal(windowMatchesScopeClass(readProp, fakeRef("kwrite"), scope), true);
        assert.equal(windowMatchesScopeClass(readProp, fakeRef("firefox"), scope), false);
        assert.equal(windowMatchesScopeClass(readProp, fakeRef("omnitilertestwindow"), scope), false);
        assert.equal(windowMatchesScopeClass(readProp, fakeRef("OmniTilerTestWindowExtra"), scope), false);
        assert.equal(windowMatchesScopeClass(readProp, fakeRef(undefined), scope), false);
        assert.equal(windowMatchesScopeClass(readProp, fakeRef(42), scope), false);
    });

    it("treats unreadable identity as foreign under a scope", () => {
        const throwing = (): unknown => {
            throw new Error("unreadable");
        };
        assert.equal(windowMatchesScopeClass(throwing, fakeRef("OmniTilerTestWindow"), ["OmniTilerTestWindow"]), false);
    });
});

describe("scope-class world observation", () => {
    const wins = [
        { id: "win-a", resourceClass: "OmniTilerTestWindow" },
        { id: "win-b", resourceClass: "firefox" },
        { id: "win-c", resourceClass: "kwrite" },
    ];

    it("excludes foreign windows from management without holds", () => {
        const obs = observe("OmniTilerTestWindow", wins, "win-a");
        assert.ok(obs !== null);
        const ids = obs.domains.flatMap((d) => d.windows.map((w) => w.win)).sort();
        assert.deepEqual(ids, ["win-a"]);
        assert.equal(obs.complete, true);
        assert.ok(obs.domains.every((d) => d.readable && d.hold === ""));
        assert.equal(obs.focusedGlobal, "win-a");
    });

    it("admits a repeated-flags allowlist while excluding a third identity", () => {
        // Repeated `--scope-class A --scope-class B` accumulates to the
        // comma-joined `"A,B"` form the launcher bakes.
        const obs = observe("OmniTilerTestWindow,kwrite", wins, "win-a");
        assert.ok(obs !== null);
        const ids = obs.domains.flatMap((d) => d.windows.map((w) => w.win)).sort();
        assert.deepEqual(ids, ["win-a", "win-c"]);
        assert.equal(obs.complete, true);
        assert.ok(obs.domains.every((d) => d.readable && d.hold === ""));
    });

    it("admits the same allowlist from array form", () => {
        const obs = observe(["OmniTilerTestWindow", "kwrite"], wins, "win-c");
        assert.ok(obs !== null);
        const ids = obs.domains.flatMap((d) => d.windows.map((w) => w.win)).sort();
        assert.deepEqual(ids, ["win-a", "win-c"]);
        assert.equal(obs.focusedGlobal, "win-c");
    });

    it("leaves empty scope behavior unchanged", () => {
        const obs = observe("", wins, "win-b");
        assert.ok(obs !== null);
        const ids = obs.domains.flatMap((d) => d.windows.map((w) => w.win)).sort();
        assert.deepEqual(ids, ["win-a", "win-b", "win-c"]);
        assert.equal(obs.complete, true);
        assert.equal(obs.focusedGlobal, "win-b");
    });

    it("leaves an omitted scope unchanged", () => {
        const obs = observe(undefined, wins, "win-b");
        assert.ok(obs !== null);
        const ids = obs.domains.flatMap((d) => d.windows.map((w) => w.win)).sort();
        assert.deepEqual(ids, ["win-a", "win-b", "win-c"]);
        assert.equal(obs.focusedGlobal, "win-b");
    });

    it("blanks foreign focus instead of naming an unmanaged window", () => {
        const obs = observe("OmniTilerTestWindow", wins, "win-b");
        assert.ok(obs !== null);
        const ids = obs.domains.flatMap((d) => d.windows.map((w) => w.win));
        assert.deepEqual(ids, ["win-a"]);
        assert.equal(obs.focusedGlobal, "");
        assert.ok(obs.domains.every((d) => d.focused === ""));
    });

    it("excludes identity-less windows under a scope", () => {
        const obs = observe("OmniTilerTestWindow", [{ id: "win-x" }], "win-x");
        assert.ok(obs !== null);
        const ids = obs.domains.flatMap((d) => d.windows.map((w) => w.win));
        assert.deepEqual(ids, []);
        assert.equal(obs.focusedGlobal, "");
        assert.equal(obs.complete, true);
    });

    it("fails the whole observation on a malformed allowlist", () => {
        assert.equal(observe("OmniTilerTestWindow,bad scope!", wins, "win-a"), null);
        assert.equal(observe(["OmniTilerTestWindow", 42 as unknown as string], wins, "win-a"), null);
    });
});

describe("scope-class entry startup gate", () => {
    function minimalSurface() {
        const ws = { id: "ws-1", name: "ws-1" };
        return {
            windowList: () => [],
            screens: [],
            desktops: [ws],
            activeScreen: null,
            activeWindow: null,
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ws,
        };
    }

    it("starts scoped and logs the exact scope", () => {
        const logs: string[] = [];
        const handle = startWorldAdapterEntry({
            workspace: minimalSurface(),
            owner: "test-owner",
            adapter: "test-adapter",
            scopeClass: "OmniTilerTestWindow,kwrite",
            callDbus: () => undefined,
            scheduleOnce: () => () => undefined,
            log: (message: string) => {
                logs.push(message);
            },
        });
        assert.ok(handle !== null);
        assert.ok(logs.some((l) => l === "omnitiler:world:scope class=OmniTilerTestWindow,kwrite"));
        handle.stop();
    });

    it("refuses an invalid scope fail-closed", () => {
        const logs: string[] = [];
        const handle = startWorldAdapterEntry({
            workspace: minimalSurface(),
            owner: "test-owner",
            adapter: "test-adapter",
            scopeClass: "OmniTilerTestWindow,bad scope!",
            callDbus: () => undefined,
            scheduleOnce: () => () => undefined,
            log: (message: string) => {
                logs.push(message);
            },
        });
        assert.equal(handle, null);
        assert.ok(logs.some((l) => l.startsWith("omnitiler:world:scope-invalid")));
    });

    it("refuses a throwing scope reader fail-closed", () => {
        const logs: string[] = [];
        const handle = startWorldAdapterEntry({
            workspace: minimalSurface(),
            owner: "test-owner",
            adapter: "test-adapter",
            readScopeClassFn: () => {
                throw new Error("unreadable config");
            },
            callDbus: () => undefined,
            scheduleOnce: () => () => undefined,
            log: (message: string) => {
                logs.push(message);
            },
        });
        assert.equal(handle, null);
        assert.ok(logs.some((l) => l.startsWith("omnitiler:world:scope-invalid")));
    });

    it("refuses a throwing script-config read fail-closed", () => {
        const prior = (globalThis as Record<string, unknown>)["readConfig"];
        (globalThis as Record<string, unknown>)["readConfig"] = () => {
            throw new Error("unreadable script config");
        };
        try {
            const logs: string[] = [];
            const handle = startWorldAdapterEntry({
                workspace: minimalSurface(),
                owner: "test-owner",
                adapter: "test-adapter",
                callDbus: () => undefined,
                scheduleOnce: () => () => undefined,
                log: (message: string) => {
                    logs.push(message);
                },
            });
            assert.equal(handle, null);
            assert.ok(logs.some((l) => l.startsWith("omnitiler:world:scope-invalid")));
        } finally {
            if (prior === undefined) {
                delete (globalThis as Record<string, unknown>)["readConfig"];
            } else {
                (globalThis as Record<string, unknown>)["readConfig"] = prior;
            }
        }
    });
});

describe("scope-class launcher and build plumbing", () => {
    it("bakes the launcher scope define into every dev bundle build", () => {
        const manifest = readFileSync("package.json", "utf8");
        assert.match(manifest, /"build":.*__OMNITILER_SCOPE_CLASS__/);
        assert.match(manifest, /"build:trace":.*__OMNITILER_SCOPE_CLASS__/);
    });

    it("bakes an always-empty scope into the installed build", () => {
        const manifest = readFileSync("package.json", "utf8");
        assert.match(manifest, /"build:installed":.*__OMNITILER_SCOPE_CLASS__/);
        assert.doesNotMatch(manifest, /"build:installed":[^\n]*OMNITILER_SCOPE_CLASS:-/);
    });

    it("clears any exported dev scope for the installed archive build", () => {
        const archive = readFileSync("../scripts/build-source-archive.sh", "utf8");
        assert.match(archive, /OMNITILER_SCOPE_CLASS= OMNITILER_SOURCE_REV=/);
    });

    it("threads the scope flag through the manual lifecycle", () => {
        const start = readFileSync("../scripts/start-test.sh", "utf8");
        assert.match(start, /--scope-class/);
        assert.match(start, /OMNITILER_SCOPE_CLASS/);
    });

    it("keeps the scope matcher in the shipped bundle", () => {
        const bundle = readFileSync("contents/code/main.js", "utf8");
        assert.match(bundle, /omnitiler:world:scope class=/);
        assert.match(bundle, /resourceClass/);
    });
});

describe("scope-class compiled define semantics", () => {
    // The exact shell fragment the manifest bakes for a script, expanded
    // the way npm would expand it. Any stray quoting (the trailing
    // apostrophe regression) fails here before esbuild runs.
    function manifestFragment(script: string): string {
        const manifest = JSON.parse(readFileSync("package.json", "utf8")) as { scripts: Record<string, string> };
        const line = manifest.scripts[script] ?? "";
        const match = line.match(/--define:__OMNITILER_SCOPE_CLASS__=(\S+)/);
        assert.ok(match !== null && match[1] !== undefined, `no scope define in ${script}`);
        return match[1];
    }

    function expandDefine(fragment: string, scopeEnv: string | undefined): string {
        const env: Record<string, string | undefined> = { ...process.env };
        if (scopeEnv === undefined) {
            delete env["OMNITILER_SCOPE_CLASS"];
        } else {
            env["OMNITILER_SCOPE_CLASS"] = scopeEnv;
        }
        return execFileSync("bash", ["-c", `printf '%s' ${fragment}`], {
            env: env as NodeJS.ProcessEnv,
            encoding: "utf8",
        });
    }

    // Arrays do not survive the vm boundary (different Array prototype),
    // so the probe stringifies inside the realm; the parsed outer value
    // is an ordinary array again.
    function bakedList(script: string, scopeEnv: string | undefined): unknown {
        const outDir = mkdtempSync(join(tmpdir(), "pat-scope-"));
        const outFile = join(outDir, "scope.js");
        execFileSync("npx", [
            "esbuild",
            "src/scope-class.ts",
            "--bundle",
            "--format=iife",
            "--target=es2017",
            "--global-name=PatScope",
            `--define:__OMNITILER_SCOPE_CLASS__=${expandDefine(manifestFragment(script), scopeEnv)}`,
            `--outfile=${outFile}`,
        ], { cwd: process.cwd(), stdio: "pipe" });
        const context = createContext({});
        runInContext(readFileSync(outFile, "utf8"), context, { filename: outFile });
        const probe = (context as unknown as { PatScope?: unknown }).PatScope;
        assert.ok(probe !== undefined, "scope IIFE must be exposed");
        const json: unknown = runInContext(
            "JSON.stringify(PatScope.normalizeScopeClassList(PatScope.readBakedScopeClass()))",
            context,
        );
        assert.ok(typeof json === "string");
        return JSON.parse(json) as unknown;
    }

    it("expands the manifest fragment exactly, without stray quoting", () => {
        assert.equal(expandDefine(manifestFragment("build"), undefined), `""`);
        assert.equal(expandDefine(manifestFragment("build"), "OmniTilerTestWindow"), `"OmniTilerTestWindow"`);
        assert.equal(expandDefine(manifestFragment("build"), "OmniTilerTestWindow,kwrite"), `"OmniTilerTestWindow,kwrite"`);
        assert.equal(expandDefine(manifestFragment("build:trace"), "A,B"), `"A,B"`);
        assert.equal(expandDefine(manifestFragment("build:installed"), "ContaminatedProbe"), `""`);
    });

    it("bakes empty scope for unset env (startup normal, not inert)", () => {
        assert.deepEqual(bakedList("build", undefined), []);
    });

    it("bakes one and two identities through dev builds", () => {
        assert.deepEqual(bakedList("build", "OmniTilerTestWindow"), ["OmniTilerTestWindow"]);
        assert.deepEqual(bakedList("build", "OmniTilerTestWindow,kwrite"), ["OmniTilerTestWindow", "kwrite"]);
        assert.deepEqual(bakedList("build:trace", "A,B"), ["A", "B"]);
    });

    it("bakes empty scope for installed builds regardless of env", () => {
        assert.deepEqual(bakedList("build:installed", "ContaminatedProbe"), []);
        assert.deepEqual(bakedList("build:installed", undefined), []);
    });
});
