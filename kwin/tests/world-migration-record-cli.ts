// Disjoint migration family recording CLI: standalone entry that drives
// `recordMigrationFamily` from `world-migration-record.ts` and writes the
// emitted v4 envelopes plus the bound replies to an explicit fresh OUTDIR
// outside the repo. Split from the pure recorder module so test bundles that
// import the recorder never execute CLI side effects.
//
// Run through the declared development environment, with fresh output paths:
//   cargo build --offline -p omnitiler --example raw_world_eval
//   kwin/node_modules/.bin/esbuild kwin/tests/world-migration-record-cli.ts --bundle --platform=node --format=cjs --target=es2020 --outfile="$RECORDER_JS"
//   node "$RECORDER_JS" --out-dir "$OUT_DIR" [--bridge-binary "$BRIDGE_BIN"]
// BRIDGE_BIN follows Cargo's configured target directory. An existing output
// directory refuses instead of overwriting.

import { recordMigrationFamily } from "./world-migration-record";

function parseCli(argv: ReadonlyArray<string>): { outDir: string; bridgeBinary?: string } {
    let outDir = "";
    let bridgeBinary: string | undefined;
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i] as string;
        if (arg === "--out-dir") {
            outDir = (argv[i + 1] as string | undefined) ?? "";
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
    return { outDir, ...(bridgeBinary === undefined ? {} : { bridgeBinary }) };
}

async function main(argv: ReadonlyArray<string>): Promise<void> {
    const cli = parseCli(argv);
    const result = await recordMigrationFamily({
        outDir: cli.outDir,
        ...(cli.bridgeBinary === undefined ? {} : { bridgeBinary: cli.bridgeBinary }),
    });
    process.stdout.write(JSON.stringify({ ok: true, files: result.files }) + "\n");
}

main(process.argv.slice(2)).then(
    () => undefined,
    (error: unknown) => {
        process.stderr.write(`migrate-record failed: ${error instanceof Error ? error.message : String(error)}\n`);
        process.exitCode = 1;
    },
);
