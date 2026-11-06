// Stage 5 v4 recording CLI: standalone entry that drives
// `recordStage5V4Chain` from `world-stage5-v4-record.ts` and writes the
// emitted v4 envelopes plus the read projections to an explicit fresh
// OUTDIR outside the repo. Split from the pure recorder module so test
// bundles that import the recorder never execute CLI side effects.
//
// Run through the declared development environment, with fresh output paths:
//   cargo build --locked -p omnitiler --example broker_world_eval
//   kwin/node_modules/.bin/esbuild kwin/tests/world-stage5-v4-record-cli.ts --bundle --platform=node --format=cjs --target=es2020 --define:__OMNITILER_TRACE__='"1"' --outfile="$RECORDER_JS"
//   node "$RECORDER_JS" --out-dir "$OUT_DIR" --chain both --bridge-binary "$BRIDGE_BIN"
// BRIDGE_BIN follows Cargo's configured target directory. Existing per-chain
// output directories refuse instead of overwriting.

import { existsSync } from "node:fs";
import {
    chainDir,
    recordStage5V4Chain,
    writeStage5V4Chain,
    type Stage5V4ChainName,
    type Stage5V4ChainResult,
} from "./world-stage5-v4-record";

function parseCli(argv: ReadonlyArray<string>): { outDir: string; chains: ReadonlyArray<Stage5V4ChainName>; bridgeBinary?: string } {
    let outDir = "";
    let chain: Stage5V4ChainName | "both" = "both";
    let bridgeBinary: string | undefined;
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i] as string;
        if (arg === "--out-dir") {
            outDir = (argv[i + 1] as string | undefined) ?? "";
            i += 1;
        } else if (arg === "--chain") {
            const value = (argv[i + 1] as string | undefined) ?? "";
            if (value !== "full" && value !== "s2" && value !== "both") {
                throw new Error(`--chain must be full, s2, or both, got ${value}`);
            }
            chain = value;
            i += 1;
        } else if (arg === "--bridge-binary") {
            const value = (argv[i + 1] as string | undefined) ?? "";
            if (value !== "") {
                bridgeBinary = value;
            }
            i += 1;
        } else {
            throw new Error(`unknown argument: ${arg}`);
        }
    }
    if (outDir === "") {
        throw new Error("missing required --out-dir <fresh-dir-outside-repo>");
    }
    const chains: ReadonlyArray<Stage5V4ChainName> = chain === "both" ? ["full", "s2"] : [chain];
    return { outDir, chains, ...(bridgeBinary === undefined ? {} : { bridgeBinary }) };
}

async function main(argv: ReadonlyArray<string>): Promise<void> {
    const cli = parseCli(argv);
    // Freshness preflight before any recording: no chain directory may
    // exist, so a run never overwrites prior outputs of either chain.
    for (const chain of cli.chains) {
        const dir = chainDir(cli.outDir, chain);
        if (existsSync(dir)) {
            throw new Error(`refusing to overwrite existing chain directory: ${dir}`);
        }
    }
    const all: string[] = [];
    for (const chain of cli.chains) {
        try {
            const result = await recordStage5V4Chain({ chain, ...(cli.bridgeBinary === undefined ? {} : { bridgeBinary: cli.bridgeBinary }) });
            for (const path of writeStage5V4Chain(cli.outDir, result)) {
                all.push(path);
            }
        } catch (error) {
            const partial = (error as { partialResult?: Stage5V4ChainResult }).partialResult;
            if (partial !== undefined) {
                for (const path of writeStage5V4Chain(cli.outDir, partial, error instanceof Error ? error.message : String(error))) {
                    all.push(path);
                }
            }
            throw error;
        }
    }
    process.stdout.write(JSON.stringify({ ok: true, files: all.length, chains: cli.chains }) + "\n");
}

main(process.argv.slice(2)).then(
    () => undefined,
    (error: unknown) => {
        process.stderr.write(`stage5-v4-record failed: ${error instanceof Error ? error.message : String(error)}\n`);
        process.exitCode = 1;
    },
);
