import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    ensureTrailingEmptyDesktop,
    orderedDesktopEntries,
    parseWorkspaceMode,
    symbolForDigit,
    workspaceShortcutCatalog,
} from "../src/workspace-native";

// Retained free-function coverage for the trimmed workspace-native module:
// mode parsing, the numbered shortcut catalog, and the pure desktop
// ordering/trailing-empty helpers. The session adapter class and its
// entry wiring are retired with the legacy plan route.

describe("workspace mode parsing and chord catalog", () => {
    it("selects workspaceMode with invalid defaulting to per-output-local", () => {
        assert.equal(parseWorkspaceMode("per-output-local"), "per-output-local");
        assert.equal(parseWorkspaceMode("global-unique"), "global-unique");
        assert.equal(parseWorkspaceMode("shared"), "shared");
        assert.equal(parseWorkspaceMode("bogus"), "per-output-local");
        assert.equal(parseWorkspaceMode(undefined), "per-output-local");
        assert.equal(parseWorkspaceMode(""), "per-output-local");
    });

    it("publishes exact number chords with shifted-symbol aliases and no foreign records", () => {
        const catalog = workspaceShortcutCatalog();
        assert.equal(catalog.length, 75);
        const byAction = new Map(catalog.map((row) => [row.action, row]));
        for (let index = 1; index <= 9; index += 1) {
            assert.equal(byAction.get(`omnitiler-workspace-${String(index)}`)?.sequence, `Meta+${String(index)}`);
            assert.equal(byAction.get(`omnitiler-workspace-${String(index)}`)?.kind, "select");
            assert.equal(byAction.get(`omnitiler-move-workspace-${String(index)}`)?.sequence, `Meta+Shift+${String(index)}`);
            assert.equal(byAction.get(`omnitiler-move-workspace-${String(index)}`)?.kind, "move");
        }
        assert.equal(byAction.get("omnitiler-workspace-0")?.sequence, "Meta+0");
        assert.equal(byAction.get("omnitiler-move-workspace-append")?.sequence, "Meta+Shift+0");
        const expectedSymbols: ReadonlyArray<[number, string]> = [
            [1, "!"], [2, "@"], [3, "#"], [4, "$"], [5, "%"],
            [6, "^"], [7, "&"], [8, "*"], [9, "("], [0, ")"],
        ];
        for (const [digit, symbol] of expectedSymbols) {
            assert.equal(symbolForDigit(digit), symbol);
        }
        assert.equal(byAction.get("omnitiler-move-workspace-1-symbol")?.sequence, "Meta+!");
        assert.equal(byAction.get("omnitiler-move-workspace-2-symbol")?.sequence, "Meta+@");
        assert.equal(byAction.get("omnitiler-move-workspace-9-symbol")?.sequence, "Meta+(");
        assert.equal(byAction.get("omnitiler-move-workspace-append-symbol")?.sequence, "Meta+)");
        for (const row of catalog) {
            assert.ok(!row.action.includes("focus") && !row.action.includes("resize"), row.action);
        }
    });
});

describe("workspace lifecycle mapping and trailing empty", () => {
    it("orders backing desktops by x11 number with positional default", () => {
        const a = { ref: {}, id: "b", num: 2 };
        const b = { ref: {}, id: "a", num: 1 };
        const ordered = orderedDesktopEntries([a, b]);
        assert.deepEqual(ordered.map((entry) => entry.id), ["a", "b"]);
        const positional = orderedDesktopEntries([
            { ref: {}, id: "x", num: null },
            { ref: {}, id: "y", num: 1 },
        ]);
        assert.deepEqual(positional.map((entry) => entry.id), ["x", "y"]);
    });

    it("keeps the literal last ordered desktop as trailing empty and retires only safe owned empties", () => {
        const removed: string[] = [];
        const result = ensureTrailingEmptyDesktop({
            orderedIds: ["a", "b", "c"],
            isEmpty: (id) => id !== "a",
            isVisible: (id) => id === "a",
            removeDesktop: (id) => {
                removed.push(id);
                return true;
            },
            createDesktop: () => null,
        });
        assert.deepEqual(removed, ["b"]);
        assert.deepEqual(result.removedIds, ["b"]);
        assert.equal(result.appendedId, null);
    });

    it("appends one trailing empty when the last ordered desktop is occupied", () => {
        const result = ensureTrailingEmptyDesktop({
            orderedIds: ["a", "b"],
            isEmpty: () => false,
            isVisible: () => false,
            removeDesktop: () => false,
            createDesktop: () => "c",
        });
        assert.equal(result.appendedId, "c");
    });
});
