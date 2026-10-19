import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { chooseDisplacedDestination } from "../src/workspace-native";

describe("chooseDisplacedDestination fallback chain", () => {
    it("returns null with no survivors and the single survivor directly", () => {
        assert.equal(chooseDisplacedDestination([], null, null), null);
        assert.equal(chooseDisplacedDestination([{ key: "a", rect: null }], null, null), "a");
    });
    it("chooses nearest when removed geometry is supplied", () => {
        const dest = chooseDisplacedDestination(
            [
                { key: "left", rect: { x: 0, y: 0, w: 1200, h: 800 } },
                { key: "right", rect: { x: 2000, y: 0, w: 1200, h: 800 } },
            ],
            "left",
            { x: 2100, y: 100, w: 100, h: 100 },
        );
        assert.equal(dest, "right");
    });
    it("falls back to live primary when reference is unavailable", () => {
        const dest = chooseDisplacedDestination(
            [
                { key: "a", rect: { x: 0, y: 0, w: 1200, h: 800 } },
                { key: "b", rect: { x: 1300, y: 0, w: 1200, h: 800 } },
            ],
            "b",
            null,
        );
        assert.equal(dest, "b");
    });
    it("falls back to deterministic existing ordering when primary is absent", () => {
        const dest = chooseDisplacedDestination(
            [
                { key: "a", rect: null },
                { key: "b", rect: null },
            ],
            "missing",
            null,
        );
        assert.equal(dest, "a");
    });
});
