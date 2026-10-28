import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import { createContext, runInContext } from "node:vm";

import { KWIN_TRACE_ENABLED } from "../src/trace";

function traceValue(define: string | null): boolean {
    const outDir = mkdtempSync(join(tmpdir(), "pat-trace-"));
    const outFile = join(outDir, "main.js");
    const args = [
        "esbuild",
        "src/trace.ts",
        "--bundle",
        "--format=iife",
        "--target=es2017",
        "--global-name=PatTrace",
        `--outfile=${outFile}`,
    ];
    if (define !== null) {
        args.push(`--define:__OMNITILER_TRACE__=${JSON.stringify(define)}`);
    }
    execFileSync("npx", args, { cwd: process.cwd(), stdio: "pipe" });
    const context = createContext({});
    runInContext(readFileSync(outFile, "utf8"), context, { filename: outFile });
    const global = (context as unknown as { PatTrace?: { KWIN_TRACE_ENABLED: boolean } }).PatTrace;
    assert.ok(global !== undefined, "trace IIFE must be exposed");
    return global.KWIN_TRACE_ENABLED;
}

describe("compile-time KWin trace gate", () => {
    it("is disabled in ordinary builds and enabled only by the exact trace define", () => {
        assert.equal(KWIN_TRACE_ENABLED, true);
        assert.equal(traceValue(null), false);
        assert.equal(traceValue("1"), true);
        assert.equal(traceValue("true"), false);
    });

    it("keeps trace build opt-in from the development launcher", () => {
        const manifest = readFileSync("package.json", "utf8");
        const start = readFileSync("../scripts/start-test.sh", "utf8");
        assert.match(manifest, /"build:trace":.*__OMNITILER_TRACE__/);
        assert.match(start, /OMNITILER_TRACE/);
        assert.match(start, /build:trace/);
    });
});

describe("world entry probe compile-time stripping", () => {
    // Mirrors the package.json `build` (ordinary), `build:installed`, and
    // `build:trace` invocations: only the __OMNITILER_TRACE__ define (and
    // the installed source-rev bake) differs. Ordinary and installed builds
    // add --minify-syntax so the statically-false probe gate is dropped
    // from the emitted bundle.
    function bundleEntry(variant: "ordinary" | "installed" | "trace"): string {
        const outDir = mkdtempSync(join(tmpdir(), "pat-world-"));
        const outFile = join(outDir, "main.js");
        const trace = variant === "trace";
        const args = [
            "esbuild",
            "src/entry.ts",
            "--bundle",
            "--format=iife",
            "--target=es2017",
            `--define:__OMNITILER_TRACE__=${JSON.stringify(trace ? "1" : "0")}`,
            `--outfile=${outFile}`,
        ];
        if (!trace) {
            args.push("--minify-syntax");
        }
        if (variant === "installed") {
            args.push(`--define:__OMNITILER_SOURCE_REV__=${JSON.stringify("a".repeat(40))}`);
        }
        execFileSync("npx", args, { cwd: process.cwd(), stdio: "pipe" });
        return readFileSync(outFile, "utf8");
    }

    function assertNoProbe(bundle: string, label: string): void {
        assert.ok(!bundle.includes("omnitiler:world:trace"), `${label} bundle must not emit probe lines`);
        assert.ok(!bundle.includes("__OMNITILER_TRACE__"), `${label} bundle must fold the trace define away`);
        assert.ok(!bundle.includes("frameGeometryChanged"), `${label} bundle must not carry the probe-only subscription`);
        // Every gated use site folds away with the construction branch, so
        // no probe method or state identifier can survive: these names
        // occur nowhere in production code.
        for (const marker of [
            "noteAdded",
            "noteDebounce",
            "noteSend",
            "noteReply",
            "setGeometryFor",
            "tracePending",
            "traceRequested",
            "traceSentBySrc",
            "traceRememberSent",
            "traceDetachRequested",
            "traceEmitPending",
            "traceGeneration",
            "worldTraceProbes",
        ]) {
            assert.ok(!bundle.includes(marker), `${label} bundle must not carry probe identifier ${marker}`);
        }
    }

    it("ships no probe code in ordinary bundles", () => {
        assertNoProbe(bundleEntry("ordinary"), "ordinary");
    });

    it("ships no probe code in installed bundles", () => {
        assertNoProbe(bundleEntry("installed"), "installed");
    });

    it("keeps the probe in trace bundles", () => {
        const traced = bundleEntry("trace");
        assert.ok(traced.includes("omnitiler:world:trace"), "trace bundle must carry the probe");
    });

    it("mirrors the actual package build flag sets", () => {
        const manifest = JSON.parse(readFileSync("package.json", "utf8")) as { scripts: Record<string, string> };
        const build = manifest.scripts["build"] ?? "";
        const installed = manifest.scripts["build:installed"] ?? "";
        const trace = manifest.scripts["build:trace"] ?? "";
        for (const [label, script] of [["build", build], ["build:installed", installed]] as const) {
            assert.match(script, /__OMNITILER_TRACE__.*0/, `${label} must define trace off`);
            assert.match(script, /--minify-syntax/, `${label} must minify-syntax the false gate away`);
        }
        assert.match(installed, /__OMNITILER_SOURCE_REV__/, "build:installed must bake the source rev");
        assert.match(trace, /__OMNITILER_TRACE__.*1/, "build:trace must define trace on");
        assert.doesNotMatch(trace, /--minify-syntax/, "build:trace must leave the probe unfolded");
    });
});
