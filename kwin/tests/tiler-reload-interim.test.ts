import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it } from "node:test";

const read = (path: string): string => readFileSync(join(process.cwd(), path), "utf8");

const module = read("native-effect/unifiedsettings_module.cpp");
const header = read("native-effect/unifiedsettings_module.h");
const ui = read("native-effect/unifiedsettings.ui");
const gaps = read("src/domain-gap.ts");
const productionEntry = read("src/entry.ts");

function functionBody(source: string, signature: string): string {
    const start = source.indexOf(signature);
    assert.notEqual(start, -1, `${signature} must exist`);
    const open = source.indexOf("{", start);
    assert.notEqual(open, -1, `${signature} must open a body`);
    let depth = 0;
    for (let index = open; index < source.length; index += 1) {
        const ch = source[index];
        if (ch === "{") {
            depth += 1;
        } else if (ch === "}") {
            depth -= 1;
            if (depth === 0) {
                return source.slice(open, index + 1);
            }
        }
    }
    assert.fail(`${signature} must close its body`);
}

describe("interim tiler reload contract", () => {
    it("keeps the production entry bound to the world adapter", () => {
        assert.match(productionEntry, /startWorldAdapterEntry/);
        assert.match(productionEntry, /owner: "kwin-world-adapter"/);
    });

    it("types the deliberate reload as one KWin reconfigure send without claiming application", () => {
        assert.match(module, /QStringLiteral\("org\.kde\.KWin"\)/);
        assert.match(module, /QStringLiteral\("\/KWin"\)/);
        assert.match(module, /QStringLiteral\("org\.kde\.KWin"\)/);
        assert.match(module, /QStringLiteral\("reconfigure"\)/);
        assert.match(module, /Q_NOREPLY/);
        assert.match(module, /never proves the/);
        assert.match(module, /never claims the running tiler applied/);
        assert.match(header, /requestScriptReconfigure/);
        assert.match(header, /isScriptRestartRequired/);
        assert.match(header, /scriptStatusText/);
        assert.doesNotMatch(header, /requestTilerReload/);
        assert.doesNotMatch(header, /isTilerReloadRequired/);
        assert.doesNotMatch(header, /tilerReloadStatusText/);
        assert.doesNotMatch(module, /m_scriptReconfigurePending/);
        const reconfigureBody = functionBody(module, "bool UnifiedSettingsModule::requestScriptReconfigure()");
        assert.match(reconfigureBody, /\.send\(/);
        assert.doesNotMatch(reconfigureBody, /\.call\(/);
    });

    it("sends nothing on an unchanged save and requests reconfigure only for changed live settings", () => {
        const saveBody = functionBody(module, "void UnifiedSettingsModule::save()");
        assert.match(saveBody, /scriptRetryArmed = m_gapReconfigurePending/);
        assert.match(saveBody, /const bool liveChanged = gapChanged \|\| sameAxisMoveChanged \|\| fixedPredicateChanged/);
        assert.match(saveBody, /if \(liveChanged \|\| scriptRetryArmed\)/);
        assert.match(saveBody, /requestScriptReconfigure\(\)/);
        assert.doesNotMatch(module, /setEnabled\(m_tilerReloadRequired\)/);
        assert.doesNotMatch(module, /tilerReloadButton/);
    });

    it("marks changed gaps for reconfigure and workspace mode restart-required with auto-send", () => {
        assert.match(module, /m_scriptRestartRequired = true/);
        assert.match(module, /gapChanged/);
        assert.match(module, /startupConsumedChanged/);
        assert.match(module, /Reconfigure request sent/);
        assert.match(module, /Session restart remains required/);
        assert.match(module, /m_gapReconfigurePending = true/);
        assert.match(module, /retry on the next save/);
        assert.doesNotMatch(module, /tilingAlgorithm|automaticSplitTarget|dropOutlinePreview|unconsumed settings/);
        assert.match(module, /startup gap values/);
        const saveBody = functionBody(module, "void UnifiedSettingsModule::save()");
        assert.match(saveBody, /scriptRetryArmed = m_gapReconfigurePending/);
        assert.match(saveBody, /if \(liveChanged \|\| scriptRetryArmed\)/);
        assert.match(saveBody, /m_scriptRestartRequired = true/);
        assert.match(saveBody, /requestScriptReconfigure\(\)/);
        assert.match(gaps, /re-resolve/);
    });

    it("reports sent-but-unconfirmed and failed states without an applied claim and keeps restart residual", () => {
        assert.match(module, /Reconfigure request sent; application unconfirmed/);
        assert.match(module, /application unconfirmed/i);
        assert.match(module, /Reconfigure request failed; the running tiler still uses startup gap values/);
        assert.match(module, /restart the session to guarantee pickup/i);
        assert.match(module, /session restart remains required for workspace mode/i);
        assert.doesNotMatch(module, /No running tiler effect for unconsumed settings/);
        const saveBody = functionBody(module, "void UnifiedSettingsModule::save()");
        const saveStrings = saveBody
            .split("\n")
            .filter((line) => line.includes("QStringLiteral"));
        assert.ok(saveStrings.length >= 2);
        for (const line of saveStrings) {
            assert.doesNotMatch(line, /applied/i);
        }
        const statusLines = module
            .split("\n")
            .filter((line) => line.includes("m_scriptStatus ="));
        assert.ok(statusLines.length >= 3);
        for (const line of statusLines) {
            assert.doesNotMatch(line, /applied/i);
        }
    });

    it("names both live keys in combined save status and keeps retry wording generic", () => {
        assert.match(module, /gaps and same-axis move/);
        assert.match(module, /Tiling gaps and same-axis move saved to kwinrc/);
        assert.match(module, /gaps and same-axis move is unconfirmed/);
        assert.match(module, /pending live settings/);
        assert.match(module, /pickup of pending live settings is unconfirmed/);
    });

    it("keeps shortcut mutation out of ordinary save", () => {
        const saveBody = functionBody(module, "void UnifiedSettingsModule::save()");
        for (const forbidden of [
            /ShortcutReconciler/,
            /KGlobalAccel/,
            /setShortcutKeys/,
            /globalShortcut/i,
            /confirmShortcutAction/,
            /runShortcutApply/,
        ]) {
            assert.doesNotMatch(saveBody, forbidden);
        }
    });

    it("exposes gap save status with restart residual in the unified dialog", () => {
        assert.match(ui, /name="scriptStatusLabel"/);
        assert.doesNotMatch(ui, /name="tilerReloadButton"/);
        assert.match(ui, /No pending script setting in this dialog\./);
        assert.match(ui, /Session restart is the guaranteed pickup mechanism/);
        assert.match(ui, /never claims the running tiler applied the settings/);
        assert.match(ui, /Saving changed gaps sends one typed KWin reconfigure request/);
        assert.match(ui, /workspace mode requires a session restart/i);
        assert.doesNotMatch(ui, /unconsumed settings have no running effect/i);
        assert.match(ui, /Border changes apply immediately through the KWin effect reconfigure\./);
        assert.match(ui, /Tiling gaps and workspace mode are on the Tiling &amp; Workspaces tab\./);
    });
});
