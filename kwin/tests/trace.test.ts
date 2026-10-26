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
