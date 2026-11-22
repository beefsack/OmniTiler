import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it } from "node:test";
import {
    registerWorldShortcuts,
    WORKSPACE_TILING_TOGGLE_ACTION,
    WORKSPACE_TILING_TOGGLE_SEQUENCE,
    type WorldShortcutsEnv,
} from "../src/world-shortcuts";
import { workspaceShortcutCatalog } from "../src/workspace-native";

const read = (path: string): string => readFileSync(join(process.cwd(), path), "utf8");

const schema = read("contents/config/main.xml");
const nativeMetadata = JSON.parse(read("native-effect/metadata.json")) as {
    KPlugin: { Description: string };
};
const kcmMetadata = JSON.parse(read("native-effect/activeborderconfig_module.json")) as {
    KPlugin: { Id: string; Name: string; Description: string; Icon: string; License: string };
};
const scriptKcmMetadata = JSON.parse(read("native-effect/scriptconfig_module.json")) as {
    KPlugin: { Id: string; Name: string; Description: string; Icon: string; License: string };
};
const kcfg = read("native-effect/activeborderconfig.kcfg");
const unified = read("native-effect/unifiedsettings_module.cpp");
const unifiedHeader = read("native-effect/unifiedsettings_module.h");
const descriptors = read("native-effect/generated/kde_settings_descriptors.h");
const unifiedUi = read("native-effect/unifiedsettings.ui");
const reconcilerHeader = read("native-effect/shortcutreconciler.h");
const reconciler = read("native-effect/shortcutreconciler.cpp");
const effect = read("native-effect/activewindowborder.cpp");
const stage = read("native-effect/presentation_stage5.cpp");

const SCRIPT_SETTINGS = {
    workspaceMode: { type: "Enum", defaultValue: "per-output-local" },
    shortcutProfile: { type: "Enum", defaultValue: "cosmic" },
    sameAxisMove: { type: "Enum", defaultValue: "group-with-neighbor" },
    fixedSizePredicate: { type: "Enum", defaultValue: "both-axes-fixed" },
    bornMaximized: { type: "Enum", defaultValue: "tile-on-launch" },
    migrationSourceRefill: { type: "Enum", defaultValue: "last-remaining-workspace" },
    switchTarget: { type: "Enum", defaultValue: "cursor-output" },
    freshPlacement: { type: "Enum", defaultValue: "cursor-output-last-focused" },
    focusFollowsCursor: { type: "Bool", defaultValue: "false" },
    innerGap: { type: "Int", defaultValue: "8" },
    outerGap: { type: "Int", defaultValue: "8" },
} as const;

function schemaEntries(): Record<string, { type: string; defaultValue: string }> {
    const entries: Record<string, { type: string; defaultValue: string }> = {};
    for (const match of schema.matchAll(/<entry name="([^"]+)" type="([^"]+)">([\s\S]*?)<\/entry>/g)) {
        const name = match[1];
        const type = match[2];
        const content = match[3];
        if (name === undefined || type === undefined || content === undefined) {
            assert.fail("entry match must declare name, type, and content");
        }
        const defaultMatch = content.match(/<default>([^<]*)<\/default>/);
        if (defaultMatch === null) {
            assert.fail(`${name} must declare a default`);
        }
        const defaultValue = defaultMatch[1];
        if (defaultValue === undefined) {
            assert.fail(`${name} must declare a default value`);
        }
        entries[name] = { type, defaultValue };
    }
    return entries;
}

describe("native KCM static contract", () => {
    it("provides script settings plugin metadata", () => {
        for (const field of ["Name", "Description", "Icon", "License"] as const) {
            assert.notEqual(scriptKcmMetadata.KPlugin[field], "");
        }
        assert.doesNotMatch(scriptKcmMetadata.KPlugin.Description, /shortcut profile/i);
    });

    it("brands the surviving effect OmniTiler", () => {
        assert.match(nativeMetadata.KPlugin.Description, /OmniTiler/);
        assert.equal(kcmMetadata.KPlugin.Name, "OmniTiler");
    });

    it("keeps the shipped script settings schema and defaults", () => {
        // Only the tray changes defaultTiled; the script consumes its value.
        const entries = schemaEntries();
        assert.deepEqual(entries["defaultTiled"], { type: "Bool", defaultValue: "true" });
        const { defaultTiled: _ignored, ...rest } = entries;
        assert.deepEqual(rest, SCRIPT_SETTINGS);

        // Catalog-owned metadata lives in the generated descriptors and the
        // focused adapter: defaults, bounds, tokens and enum rows derive
        // from there, while the module keeps persistence and staging.
        assert.match(descriptors, /"focused-output"/);
        assert.match(descriptors, /"cursor-output-cursor-closest"/);
    });

    it("keeps the native appearance descriptor defaults and bounds", () => {
        for (const bounds of [
            `"3.0", "0.0", "32.0"`,
            `"6.0", "0.0", "64.0"`,
            `"0.0", "0.0", "64.0"`,
            `"-1", "-1", "32.0"`,
        ]) {
            assert.match(descriptors, new RegExp(bounds.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
        }
    });

    it("does not expose global shortcut mutation APIs", () => {
        for (const forbidden of [
            /KGlobalAccel/,
            /globalShortcut/i,
            /registerGlobalShortcut/,
            /unregisterGlobalShortcut/,
            /setGlobalShortcut/,
        ]) {
            assert.doesNotMatch(unified, forbidden);
            assert.doesNotMatch(unifiedHeader, forbidden);
        }
    });

    it("keeps border settings in the native group and hot-applies through the native effect", () => {
        assert.match(kcfg, /<entry name="BorderRadius" type="Double">[\s\S]*?<default>6(\.0)?<\/default>/);
        assert.match(unified, /ActiveBorderConfig::instance\(QStringLiteral\("kwinrc"\)\)/);
        assert.match(unified, /QDBusInterface interface\(effectService\(\), effectPath\(\), effectInterface\(\)/);
        assert.match(unified, /interface\.call\(effectMethod\(\), effectName\(\)\)/);
        assert.match(effect, /ActiveBorderConfig::self\(\)->read\(\);[\s\S]*updateOutline\(\);/);
        assert.match(effect, /ActiveBorderConfig::useThemeColor\(\)/);
        assert.match(effect, /if \(useThemeColor\)[\s\S]*?KColorScheme::isColorSetSupported/);
        // Stage 5 pure renderer: native styling stays in the effect (the
        // active ring keeps its own border color/theme tokens, default live
        // system accent, independently overridable from the underlay/drop
        // colors). No shared visual FFI or border-eligibility derivation
        // remains: the held accepted border descriptor (outsets, width,
        // radius) paints verbatim against the live tracked frame.
        assert.match(effect, /presentationBorderColor\(themeColor, fallback, useThemeColor\)/);
        assert.match(effect, /KColorScheme::isColorSetSupported\(colorConfig, KColorScheme::Selection\)/);
        // Underlay and drop-preview fills read their own config colors and
        // never the border predicate: an override on either side touches
        // neither of the others.
        assert.match(effect, /fill\(ActiveBorderConfig::groupUnderlayColor\(\)\)/);
        assert.match(effect, /fill\(ActiveBorderConfig::dragPreviewColor\(\)\)/);
        const ringBody = effect.match(/BorderOutline ringOutlineForWidth\(double width, double radius\)[\s\S]*?\n\}/)?.[0] ?? "";
        assert.match(ringBody, /BorderRadius\(radius\)/);
        assert.doesNotMatch(ringBody, /ActiveBorderConfig::borderRadius/);
        assert.doesNotMatch(effect, /m_router\.paint\.borderInner/);
        assert.doesNotMatch(effect, /m_router\.paint\.borderOuter/);
        assert.match(effect, /m_router\.paint\.borderHasOutsets/);
        assert.match(effect, /m_router\.paint\.borderWidth/);
        assert.match(effect, /borderGeometryFromLiveFrame\(host\.trackedFrame, m_router\.paint\.borderOutsetLeft,/);
        assert.match(effect, /ringOutlineForWidth\(static_cast<double>\(m_router\.paint\.borderWidth\), m_router\.paint\.borderRadius\)/);
        assert.match(effect, /setInnerRect\(window \? window->windowItem\(\)->mapFromScene\(innerRect\) : RectF\(\)\)/);
        assert.match(effect, /m_stage\.decide\(currentHostView\(\)\)/);
        assert.match(effect, /visibility\.border/);
        assert.match(stage, /visibility\.border = hasPaint && paint\.borderVisible && host\.endpointAvailable && host\.foregroundMatches/);
        assert.match(effect, /setVisible\(visible\)/);
        assert.match(effect, /addRepaintFull\(\)/);
    });

    it("exposes translucent group underlay color and extension through KConfigXT", () => {
        assert.match(kcfg, /<entry name="BorderColor" type="Color">/);
        assert.match(kcfg, /<entry name="DragPreviewColor" type="Color">/);
        assert.match(unifiedUi, /Match border width/);
        assert.match(effect, /groupUnderlayColor\(\)/);
        assert.match(effect, /groupUnderlayExtension\(\)/);
        // Stage 5 pure renderer: the stored core outer rect paints
        // verbatim. Config extension rides the style facts ingress; no
        // native outer-rect recomputation or extension resolution remains.
        assert.match(effect, /m_router\.paint\.underlayOuter/);
        assert.doesNotMatch(effect, /groupUnderlayOuterRect/);
        assert.doesNotMatch(effect, /groupUnderlayEffectiveExtension\(/);
    });

    it("manages the theme override through KConfigXT without disabling the fallback color", () => {
        assert.match(unifiedUi, /Use theme highlight color when available/);
        assert.match(unifiedUi, /When disabled, the configured color is always used/);
        assert.doesNotMatch(unifiedUi, /kcfg_BorderColor[\s\S]{0,400}?enabled[\s\S]{0,20}?false/);
    });

    it("describes script reconfigure, retry, and restart without claiming live application", () => {
        assert.match(unified, /This retry saved nothing/);
        assert.match(unified, /Session restart remains required for workspace mode/);
        assert.doesNotMatch(unified, /startup settings/);
        assert.match(unified, /retry on the next save/);
        assert.match(unified, /omnitiler\.script-config op=/);
        assert.match(unified, /logScriptConfig\("save", "persist", "ok"/);
        assert.match(unified, /logScriptConfig\("save", "reconfigure", "sent-unconfirmed"/);
        assert.match(unified, /logScriptConfig\("save", "reconfigure", "failed"/);
        assert.match(unified, /retry-on-next-save/);
        assert.match(unified, /logScriptConfig\("save", "startup", "restart-required"/);
        assert.match(unifiedUi, /Tiling gaps and workspace mode are on the Tiling &amp; Workspaces tab\./);
        assert.doesNotMatch(unifiedUi, /unconsumed settings have no running effect/i);
        assert.match(unifiedUi, /workspace mode requires a session restart/i);
        assert.match(unifiedUi, /requires session restart/);
        assert.match(unifiedUi, /Saving changed gaps sends one typed KWin reconfigure request/);
        assert.match(unifiedUi, /never claims the running tiler applied the settings/);
        assert.doesNotMatch(unifiedUi, /shortcut profile/i);
        assert.doesNotMatch(read("metadata.json"), /Other script settings require a script reload or session restart\./);
    });

    it("lists the full shortcut catalog with staged Keep-Disable and Authentic-Compatible presets", () => {
        // Conflict list widgets on the shared page.
        assert.match(unifiedUi, /name="shortcutConflictList"/);
        assert.match(unifiedUi, /name="shortcutConflictHintLabel"/);
        assert.match(unifiedUi, /name="shortcutAuthenticButton"/);
        assert.match(unifiedUi, /name="shortcutCompatibleButton"/);
        assert.match(unifiedUi, /checked = Keep, unchecked = Disable/);
        assert.match(reconcilerHeader, /SHORTCUT_META_SHIFT_1 = 301989937/);
        assert.match(reconcilerHeader, /SHORTCUT_META_EXCLAM = 268435489/);
        assert.match(unifiedUi, /Authentic \(use canonical\)/);
        assert.match(unifiedUi, /Compatible \(disable conflicting\)/);
        assert.match(unified, /foreign \[/);
    });

    it("keeps the native shortcut catalog aligned with the real runtime registrations", () => {
        // Runtime source of truth: drive the actual production registration
        // with a stub shortcut sink and read back exactly what the world
        // route binds. This replaces the retired plan catalog builders;
        // nothing in production consumes catalog-builder functions anymore.
        const registered: Array<{ action: string; text: string; sequence: string }> = [];
        const stubEnv: WorldShortcutsEnv = {
            readProp: (value, property) => {
                try {
                    return Reflect.get(value as object, property as string);
                } catch (error) {
                    void error;
                    return undefined;
                }
            },
            isDirection: (value): value is "left" | "right" | "up" | "down" =>
                value === "left" || value === "right" || value === "up" || value === "down",
            shortcutFailed: () => {},
            submitCommand: () => true,
            submitCatalogMove: () => {},
            submitOutputSend: () => true,
            submitMigration: () => true,
            submitFloat: () => true,
            activeOutputName: () => "out-1",
            activeWindowId: () => "win-1",
            activeWindowRef: () => ({}),
            currentScopeId: () => "out-1/ws-1",
            registerShortcutFn: (name, text, sequence) => {
                registered.push({ action: name, text, sequence });
                return true;
            },
            readSameAxisMove: () => "group-with-neighbor",
        };
        registerWorldShortcuts(stubEnv);
        const runtimeByAction = new Map<string, string>();
        const runtimeOrder: string[] = [];
        for (const row of registered) {
            if (!runtimeByAction.has(row.action)) {
                runtimeByAction.set(row.action, row.sequence);
                runtimeOrder.push(row.action);
            }
        }
        // No duplicate registrations.
        assert.equal(registered.length, runtimeOrder.length);
        const workspace = workspaceShortcutCatalog();
        // Runtime catalog shape (world production defaults): 37 directional
        // core rows (focus/move/resize letter+arrow plus 5 state toggles),
        // 75 workspace rows, 8 unbound output follow/stay rows (no arrow
        // aliases), 4 unbound migrate rows, and the bound Meta+Y tiling
        // toggle.
        assert.equal(runtimeByAction.size, 125);
        for (const row of workspace) {
            assert.equal(runtimeByAction.get(row.action), row.sequence);
        }
        assert.equal(runtimeByAction.get("omnitiler-focus-left"), "Meta+H");
        assert.equal(runtimeByAction.get("omnitiler-toggle-orientation"), "Meta+O");
        assert.equal(runtimeByAction.get("omnitiler-send-output-left"), "");
        assert.ok(!runtimeByAction.has("omnitiler-send-output-left-arrow"));
        assert.equal(runtimeByAction.get(WORKSPACE_TILING_TOGGLE_ACTION), "Meta+Y");
        // Pre-existing runtime/catalog discrepancy (world production
        // defaults; not introduced here): the native KCM catalog retains
        // four output follow-arrow actions the runtime never registers, and
        // binds the four output follow chords that the runtime registers
        // unbound for user rebinding.
        const NATIVE_ONLY_ARROW_ACTIONS = [
            "omnitiler-send-output-left-arrow",
            "omnitiler-send-output-down-arrow",
            "omnitiler-send-output-up-arrow",
            "omnitiler-send-output-right-arrow",
        ] as const;
        const OUTPUT_FOLLOW_NATIVE_CHORDS: ReadonlyMap<string, string> = new Map([
            ["omnitiler-send-output-left", "Meta+Ctrl+Alt+H"],
            ["omnitiler-send-output-down", "Meta+Ctrl+Alt+J"],
            ["omnitiler-send-output-up", "Meta+Ctrl+Alt+K"],
            ["omnitiler-send-output-right", "Meta+Ctrl+Alt+L"],
        ]);
        // Test-local mirror of the native reconciler's retained KCM catalog
        // order for the output and migrate blocks (legacy-bound defaults,
        // not runtime registrations): 12 output rows then 4 migrate rows,
        // with the workspace-tiling toggle last.
        const NATIVE_OUTPUT_ACTION_ORDER = [
            "omnitiler-send-output-left",
            "omnitiler-send-output-left-arrow",
            "omnitiler-send-output-left-stay",
            "omnitiler-send-output-down",
            "omnitiler-send-output-down-arrow",
            "omnitiler-send-output-down-stay",
            "omnitiler-send-output-up",
            "omnitiler-send-output-up-arrow",
            "omnitiler-send-output-up-stay",
            "omnitiler-send-output-right",
            "omnitiler-send-output-right-arrow",
            "omnitiler-send-output-right-stay",
        ];
        const NATIVE_MIGRATE_ACTION_ORDER = [
            "omnitiler-migrate-workspace-left",
            "omnitiler-migrate-workspace-right",
            "omnitiler-migrate-workspace-up",
            "omnitiler-migrate-workspace-down",
        ];
        const nativeByAction = new Map<string, string>();
        const nativeOrder: string[] = [];
        const entryPattern =
            /QStringLiteral\("(omnitiler-[^"]+)"\),\s*(?:SHORTCUT_[A-Z0-9_]+|0),\s*QStringLiteral\("([^"]*)"\)/g;
        for (const match of reconciler.matchAll(entryPattern)) {
            const action = match[1];
            const display = match[2];
            if (action === undefined || display === undefined) {
                assert.fail("native catalog entry must declare action and display");
            }
            if (!nativeByAction.has(action)) {
                nativeByAction.set(action, display);
                nativeOrder.push(action);
            }
        }
        // Native catalog size: the 125 runtime actions plus the 4
        // native-only arrow actions.
        assert.equal(nativeByAction.size, 129);
        for (const action of runtimeByAction.keys()) {
            assert.ok(nativeByAction.has(action), `native catalog missing runtime action ${action}`);
        }
        const nativeExtras = [...nativeByAction.keys()].filter((action) => !runtimeByAction.has(action));
        assert.deepEqual([...nativeExtras].sort(), [...NATIVE_ONLY_ARROW_ACTIONS].sort());
        assert.deepEqual(new Set(nativeOrder), new Set([...runtimeOrder, ...NATIVE_ONLY_ARROW_ACTIONS]));
        assert.equal(nativeOrder.length, runtimeOrder.length + NATIVE_ONLY_ARROW_ACTIONS.length);
        // Sequences agree everywhere except the four documented
        // output-follow mismatches (native bound, runtime unbound).
        for (const [action, sequence] of runtimeByAction) {
            const nativeSequence = nativeByAction.get(action);
            const documentedChord = OUTPUT_FOLLOW_NATIVE_CHORDS.get(action);
            if (documentedChord !== undefined) {
                assert.equal(sequence, "", `runtime binds output follow ${action}`);
                assert.equal(nativeSequence, documentedChord);
            } else {
                assert.equal(nativeSequence, sequence, `sequence mismatch for ${action}`);
            }
        }
        // Native order: the 36 item-2 workspace rows follow the 76
        // directional-plus-item-1 rows in exact workspace catalog order;
        // the retained output/migrate blocks follow in native file order;
        // the workspace-tiling toggle row is last.
        assert.deepEqual(nativeOrder.slice(76, 112), workspace.slice(-36).map((row) => row.action));
        assert.deepEqual(nativeOrder.slice(112, 124), NATIVE_OUTPUT_ACTION_ORDER);
        assert.deepEqual(nativeOrder.slice(124, 128), NATIVE_MIGRATE_ACTION_ORDER);
        assert.deepEqual(nativeOrder.slice(128), [WORKSPACE_TILING_TOGGLE_ACTION]);
        for (const row of workspace) {
            if (row.sequence === "") {
                assert.equal(nativeByAction.get(row.action), "");
            }
        }
        for (const action of [
            ...NATIVE_OUTPUT_ACTION_ORDER.filter((name) => name.endsWith("-stay")),
            ...NATIVE_MIGRATE_ACTION_ORDER,
        ]) {
            assert.equal(nativeByAction.get(action), "");
        }
        // Spot-check the 8 bound workspace follow chords, the native half of
        // the 8 output follow chords (native bound; runtime unbound per the
        // documented discrepancy above), and the unbound stay defaults.
        assert.equal(nativeByAction.get("omnitiler-toggle-orientation"), "Meta+O");
        for (const [action, sequence] of [
            ["omnitiler-send-prev-h", "Meta+Ctrl+Shift+H"],
            ["omnitiler-send-prev-k", "Meta+Ctrl+Shift+K"],
            ["omnitiler-send-prev-left-arrow", "Meta+Ctrl+Shift+Left"],
            ["omnitiler-send-prev-up-arrow", "Meta+Ctrl+Shift+Up"],
            ["omnitiler-send-next-j", "Meta+Ctrl+Shift+J"],
            ["omnitiler-send-next-l", "Meta+Ctrl+Shift+L"],
            ["omnitiler-send-next-down-arrow", "Meta+Ctrl+Shift+Down"],
            ["omnitiler-send-next-right-arrow", "Meta+Ctrl+Shift+Right"],
            ["omnitiler-send-output-left", "Meta+Ctrl+Alt+H"],
            ["omnitiler-send-output-left-arrow", "Meta+Ctrl+Alt+Left"],
            ["omnitiler-send-output-down", "Meta+Ctrl+Alt+J"],
            ["omnitiler-send-output-down-arrow", "Meta+Ctrl+Alt+Down"],
            ["omnitiler-send-output-up", "Meta+Ctrl+Alt+K"],
            ["omnitiler-send-output-up-arrow", "Meta+Ctrl+Alt+Up"],
            ["omnitiler-send-output-right", "Meta+Ctrl+Alt+L"],
            ["omnitiler-send-output-right-arrow", "Meta+Ctrl+Alt+Right"],
        ] as const) {
            assert.equal(nativeByAction.get(action), sequence);
        }
        for (const action of [
            "omnitiler-send-output-left-stay",
            "omnitiler-send-output-down-stay",
            "omnitiler-send-output-up-stay",
            "omnitiler-send-output-right-stay",
            "omnitiler-migrate-workspace-left",
            "omnitiler-migrate-workspace-right",
            "omnitiler-migrate-workspace-up",
            "omnitiler-migrate-workspace-down",
        ] as const) {
            assert.equal(nativeByAction.get(action), "");
        }
        // Bound/unbound split of the native catalog: 93 bound, 36 unbound
        // (28 workspace plus 4 output plus 4 migrate), no foreign conflict
        // additions for the migrate arms, both presets Keep.
        {
            let bound = 0;
            let unbound = 0;
            for (const sequence of nativeByAction.values()) {
                if (sequence === "") {
                    unbound += 1;
                } else {
                    bound += 1;
                }
            }
            assert.equal(bound, 93);
            assert.equal(unbound, 36);
        }
        // Workspace-tiling toggle default: Meta+Y bound in the native
        // catalog, the runtime registration, and the shared action constant;
        // every other currently-unbound action stays unbound.
        assert.equal(WORKSPACE_TILING_TOGGLE_SEQUENCE, "Meta+Y");
        assert.equal(nativeByAction.get(WORKSPACE_TILING_TOGGLE_ACTION), "Meta+Y");
        assert.equal(runtimeByAction.get(WORKSPACE_TILING_TOGGLE_ACTION), "Meta+Y");
        // R-WS-12 migrate rows follow the item-5 output-transfer pattern:
        // unbound, workspace-migrate kind, no known foreign holder.
        for (const action of [
            "omnitiler-migrate-workspace-left",
            "omnitiler-migrate-workspace-right",
            "omnitiler-migrate-workspace-up",
            "omnitiler-migrate-workspace-down",
        ] as const) {
            assert.ok(reconciler.includes(action));
        }
        assert.ok(reconciler.includes("workspace-migrate"));
        // Item 5 adds no stock conflicts for the Meta+Ctrl+Alt arms.
        for (const token of [
            "SHORTCUT_META_CTRL_ALT_H = 469762120",
            "SHORTCUT_META_CTRL_ALT_J = 469762122",
            "SHORTCUT_META_CTRL_ALT_K = 469762123",
            "SHORTCUT_META_CTRL_ALT_L = 469762124",
            "SHORTCUT_META_CTRL_ALT_LEFT = 486539282",
            "SHORTCUT_META_CTRL_ALT_UP = 486539283",
            "SHORTCUT_META_CTRL_ALT_DOWN = 486539285",
            "SHORTCUT_META_CTRL_ALT_RIGHT = 486539284",
        ]) {
            assert.match(reconcilerHeader, new RegExp(token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
        }
        // New stock conflicts for the send-arrow follow rows.
        for (const token of [
            "SHORTCUT_META_CTRL_SHIFT_LEFT = 385875986",
            "SHORTCUT_META_CTRL_SHIFT_UP = 385875987",
            "SHORTCUT_META_CTRL_SHIFT_DOWN = 385875989",
            "SHORTCUT_META_CTRL_SHIFT_RIGHT = 385875988",
        ]) {
            assert.match(reconcilerHeader, new RegExp(token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
        }
    });
});
