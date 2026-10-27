import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

describe("production bundle compatibility", () => {
    it("uses the declared ES2017 IIFE target without optional catch bindings", () => {
        const manifest = readFileSync("package.json", "utf8");
        const bundle = readFileSync("contents/code/main.js", "utf8");

        assert.match(
            manifest,
            /"build": "rm -rf dist && esbuild src\/entry\.ts --bundle --format=iife --target=es2017 .*__OMNITILER_SCOPE_CLASS__.*--outfile=contents\/code\/main\.js"/,
        );
        assert.match(bundle, /^"use strict";\n\(\(\) => \{/);
        assert.doesNotMatch(bundle, /\bcatch\s*\{/);
        assert.doesNotMatch(bundle, /\?\./);
        assert.doesNotMatch(bundle, /^\s*(?:import|export)\s/m);
        assert.doesNotMatch(bundle, /node:|sourceMappingURL/);
    });

    it("keeps the bundle free of post-ES2017 non-transpiled built-ins", () => {
        const bundle = readFileSync("contents/code/main.js", "utf8");
        assert.doesNotMatch(bundle, /\.flatMap\(/);
        assert.doesNotMatch(bundle, /\.flat\(/);
        assert.doesNotMatch(bundle, /Object\.fromEntries/);
        assert.doesNotMatch(bundle, /\.finally\(/);
        assert.doesNotMatch(bundle, /Promise\.(?:allSettled|any)\(/);
        assert.doesNotMatch(bundle, /\.(?:trimStart|trimEnd|matchAll|replaceAll)\(/);
    });

    it("routes the script metadata to the native script KCM, never the retired generic link", () => {
        const metadata = readFileSync("metadata.json", "utf8");
        const schema = readFileSync("contents/config/main.xml", "utf8");
        assert.match(metadata, /"X-KDE-ConfigModule": "kwin\/scripts\/configs\/omnitiler-kwin_config"/);
        assert.doesNotMatch(metadata, /kcm_kwin4_genericscripted/);
        assert.match(schema, /<entry name="workspaceMode" type="Enum">/);
        assert.match(schema, /<entry name="shortcutProfile" type="Enum">/);
        assert.match(schema, /<entry name="innerGap" type="Int">/);
        assert.match(schema, /<entry name="outerGap" type="Int">/);
        assert.doesNotMatch(schema, /tilingAlgorithm|automaticSplitTarget|dropOutlinePreview/);
        assert.doesNotMatch(schema, /engineAuthorityMode/);
    });

    it("declares the startup defaults in the KConfigXT schema", () => {
        const schema = readFileSync("contents/config/main.xml", "utf8");
        assert.match(schema, /<default>per-output-local<\/default>/);
        assert.match(schema, /<default>cosmic<\/default>/);
        assert.match(schema, /<entry name="innerGap" type="Int">[\s\S]*?<default>8<\/default>[\s\S]*?<min>0<\/min>[\s\S]*?<max>64<\/max>/);
        assert.match(schema, /<entry name="outerGap" type="Int">[\s\S]*?<default>8<\/default>[\s\S]*?<min>0<\/min>[\s\S]*?<max>64<\/max>/);
        assert.doesNotMatch(schema, /engineAuthorityMode/);
        // Aligned KDE offers all three scope modes: both separate modes
        // plus shared.
        for (const mode of ["per-output-local", "global-unique", "shared"]) {
            assert.match(schema, new RegExp(`<choice name="${mode}" value="${mode}"\\/>`));
        }
        for (const profile of ["cosmic", "hyprland", "bspwm"]) {
            assert.match(schema, new RegExp(`<choice name="${profile}" value="${profile}"\\/>`));
        }
    });

    it("declares the standard KCM UI with kcfg-bound controls", () => {
        const ui = readFileSync("contents/ui/config.ui", "utf8");
        assert.match(ui, /<widget class="QWidget"/);
        for (const entry of ["workspaceMode", "shortcutProfile"]) {
            assert.match(ui, new RegExp(`name="kcfg_${entry}"`));
        }
        assert.match(ui, /<widget class="QSpinBox" name="kcfg_innerGap">/);
        assert.match(ui, /<widget class="QSpinBox" name="kcfg_outerGap">/);
        assert.doesNotMatch(ui, /tilingAlgorithm|automaticSplitTarget|dropOutlinePreview/);
        assert.doesNotMatch(ui, /engineAuthorityMode/);
    });

    it("declares windowActivated without a Rust authority dispatcher", () => {
        const globals = readFileSync("src/kwin-globals.d.ts", "utf8");
        assert.ok(globals.includes("windowActivated"));
        assert.throws(() => readFileSync("src/engine-authority.ts", "utf8"));
    });

    it("mints a fresh adapter incarnation per evaluation against the retained Planner", () => {
        const productionEntry = readFileSync("src/entry.ts", "utf8");
        const bundle = readFileSync("contents/code/main.js", "utf8");
        // Stable owner with a fresh stream: reset counters open a fresh
        // sequence namespace instead of fencing as superseded/consumed.
        assert.match(productionEntry, /owner: "kwin-world-adapter"/);
        assert.match(productionEntry, /adapter: mintAdapterIncarnation\(\)/);
        assert.doesNotMatch(productionEntry, /adapter: "kwin-world-adapter"/);
        assert.match(bundle, /mintAdapterIncarnation/);
    });

});
