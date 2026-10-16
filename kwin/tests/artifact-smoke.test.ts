import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createContext, runInContext } from "node:vm";
import { describe, it } from "node:test";

const SHIPPED_BUNDLE = "contents/code/main.js";
const PLAN_METHOD_TOKEN = "DescribeWorld";
const DISCOVERY_METHOD_TOKEN = "DiscoverWorld";

// Minimal KWin ambient surface for the Stage 4 plan entry: the tray
// heartbeat plus the single DescribePlan adapter. The stub workspace carries
// no windows, so observation fails closed and the entry stays silent.
function makeKWinStub(): KWinStubResult {
    const diagnostics: string[] = [];
    const dbusCalls: string[] = [];

    function QTimer(this: Record<string, unknown>): void {
        const self = this as Record<string, unknown> & {
            interval: number;
            singleShot: boolean;
            timeout: { connect: (callback: () => void) => void };
        };
        self.interval = 0;
        self.singleShot = true;
        self.timeout = { connect: () => {} };
        (this as Record<string, unknown>)["start"] = () => {};
        (this as Record<string, unknown>)["stop"] = () => {};
    }

    function inertSignal(): unknown {
        return {
            connect: (): void => {},
            disconnect: (): void => {},
        };
    }

    const context = createContext({
        callDBus: (...args: unknown[]) => {
            dbusCalls.push(String(args[3]));
        },
        registerShortcut: (): boolean => true,
        readConfig: (): string => "cosmic",
        QTimer: QTimer as unknown,
        workspace: {
            activeWindow: null,
            windowList: (): unknown[] => [],
            windowAdded: inertSignal(),
            windowRemoved: inertSignal(),
            windowActivated: inertSignal(),
            screensChanged: inertSignal(),
            currentDesktopChanged: inertSignal(),
        },
        console: { ...console, log: (message: string) => diagnostics.push(message) },
    });

    return { context, diagnostics, dbusCalls };
}

// Post-ES2017 syntax and non-transpiled built-ins this KWin QJSEngine (ES2017)
// rejects. These are the confirmed-unsupported tokens; pre-ES2017 methods are
// deliberately not listed.
const POST_ES2017_PATTERNS: ReadonlyArray<RegExp> = [
    /\.flatMap\(/,
    /\.flat\(/,
    /Object\.fromEntries/,
    /\.finally\(/,
    /Promise\.(?:allSettled|any)\(/,
    /\.(?:trimStart|trimEnd|matchAll|replaceAll)\(/,
    /\bcatch\s*\{/,
];

function collectSourceFiles(dir: string): string[] {
    const files: string[] = [];
    for (const entry of readdirSync(dir)) {
        const full = join(dir, entry);
        if (statSync(full).isDirectory()) {
            files.push(...collectSourceFiles(full));
        } else if (full.endsWith(".ts")) {
            files.push(full);
        }
    }
    return files;
}

interface KWinStubResult {
    readonly context: ReturnType<typeof createContext>;
    readonly diagnostics: readonly string[];
    readonly dbusCalls: readonly string[];
}

describe("shipped artifact smoke execution", () => {
    it("executes the built contents/code/main.js plan entry through a KWin stub", () => {
        const bundle = readFileSync(SHIPPED_BUNDLE, "utf8");
        const stub = makeKWinStub();
        try {
            runInContext(bundle, stub.context, { filename: SHIPPED_BUNDLE });
        } catch (error) {
            assert.fail(`evaluating ${SHIPPED_BUNDLE} threw ${String(error)}`);
        }
        assert.ok(!stub.diagnostics.some((entry) => entry.includes("legacy-engine-removed")));
        // Slice 1 inert observer is production-started: the empty-window stub
        // has no finished subscription, so the only permitted drag route-diag
        // line is the single bounded no-windows entry rejection. Tray startup
        // additionally emits exactly its two bounded records (lifecycle start
        // plus fire-and-forget send initiation); nothing else may log. The
        // bridge send line carries the production snapshot identity
        // (generation token, revision 0, enabled true); the token is random
        // per process, so it is matched by pattern, never by value.
        const routeDiag = stub.diagnostics.filter((entry) => entry.includes("omnitiler:route-diag"));
        const dragDiag = routeDiag.filter((entry) => !entry.includes("component=tray "));
        assert.ok(dragDiag.length <= 1);
        assert.ok(dragDiag.every((entry) => entry === "omnitiler:route-diag:drag-entry-no-windows"));
        const trayDiag = routeDiag.filter((entry) => entry.includes("component=tray "));
        assert.equal(trayDiag.length, 2);
        assert.equal(trayDiag[0], "omnitiler:route-diag component=tray stage=tray event=started outcome=ok");
        assert.ok(trayDiag[1] !== undefined);
        assert.match(
            trayDiag[1],
            /^omnitiler:route-diag component=tray stage=bridge event=send-initiated outcome=ok generation=[a-z0-9-]{1,32} revision=0 enabled=true tiled=true defaultTiled=true$/,
        );
        assert.ok(!stub.diagnostics.some((entry) => entry.includes("drag-attach")));
        assert.ok(!bundle.includes("TileController"));
        assert.ok(bundle.includes(PLAN_METHOD_TOKEN));
        assert.ok(bundle.includes(DISCOVERY_METHOD_TOKEN));
        assert.ok(bundle.includes("registerShortcut"));
        assert.ok(!bundle.includes("DescribeMovement"));
        assert.ok(!bundle.includes("DescribeFocus"));
        assert.ok(!bundle.includes("DescribeResize"));
        assert.ok(!bundle.includes("DescribePlan"));
    });

    it("builds a fresh plan bundle with the single DescribeWorld route", () => {
        const outDir = mkdtempSync(join(tmpdir(), "pat-bundle-"));
        const outFile = join(outDir, "main.js");
        execFileSync("npx", ["esbuild", "src/entry.ts", "--bundle", "--format=iife", "--target=es2017", `--outfile=${outFile}`], {
            cwd: process.cwd(),
            stdio: "pipe",
        });
        const bundle = readFileSync(outFile, "utf8");
        assert.ok(bundle.includes(PLAN_METHOD_TOKEN));
        assert.ok(bundle.includes(DISCOVERY_METHOD_TOKEN));
        assert.ok(!bundle.includes("TileController"));
        assert.ok(bundle.includes("registerShortcut"));
        assert.ok(!bundle.includes("legacy-engine-removed"));
        assert.ok(!bundle.includes("DescribeMovement"));
        assert.ok(!bundle.includes("DescribeFocus"));
        assert.ok(!bundle.includes("DescribeResize"));
        assert.ok(!bundle.includes("DescribePlan"));
        const stub = makeKWinStub();
        try {
            runInContext(bundle, stub.context, { filename: outFile });
        } catch (error) {
            assert.fail(`evaluating temp bundle threw ${String(error)}`);
        }
        assert.ok(!stub.diagnostics.some((entry) => entry.includes("legacy-engine-removed")));
    });

    it("keeps ordered module initialization without deferred CJS wrappers", () => {
        const bundle = readFileSync(SHIPPED_BUNDLE, "utf8");
        assert.doesNotMatch(bundle, /__esm|__commonJS/);
        assert.doesNotMatch(bundle, /\b(?:init|require)_[a-z_]+\s*\(/);
    });

    it("keeps production source and shipped bundle free of post-ES2017 built-ins", () => {
        const sources = collectSourceFiles("src");
        assert.ok(sources.length > 0);
        const texts: ReadonlyArray<readonly [string, string]> = [
            [SHIPPED_BUNDLE, readFileSync(SHIPPED_BUNDLE, "utf8")],
            ...sources.map((file) => [file, readFileSync(file, "utf8")] as const),
        ];
        for (const [file, text] of texts) {
            for (const pattern of POST_ES2017_PATTERNS) {
                assert.doesNotMatch(text, pattern, `${file} matched ${String(pattern)}`);
            }
        }
    });
});
