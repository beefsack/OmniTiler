// Direction-only world intents (core-owned topology): the adapter sends
// win+direction or direction only, never an endpoint or cross flag. The
// core resolves the strict target from retained FULL rects.
// Offline only: constructor shapes plus production entry dispatch,
// no D-Bus, no KWin.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
    cmdFocusDirectional,
    cmdMigrateDirectional,
    cmdMoveDirectional,
    cmdSendDirectional,
    explicitSource,
    implicitSource,
} from "../src/world-report";

describe("direction-only world intents (no adapter endpoint)", () => {
    it("builds focus/move/send/migrate direction-only shapes", () => {
        assert.deepEqual(cmdFocusDirectional("win-a", "right"), {
            op: "focus-directional", win: "win-a", direction: "right",
        });
        assert.deepEqual(cmdMoveDirectional("win-a", "left", "group-with-neighbor"), {
            op: "move-directional", win: "win-a", direction: "left", sameAxis: "group-with-neighbor",
        });
        assert.deepEqual(cmdSendDirectional("win-a", "down", true), {
            op: "send-directional", win: "win-a", direction: "down", follow: true,
        });
        // Migration is direction-only: the core resolves the action source
        // from the neutral report facts. Legacy from/ws refuse server-side.
        assert.deepEqual(cmdMigrateDirectional("up"), {
            op: "migrate-directional", direction: "up",
        });
    });

    it("refuses bad direction and bad sameAxis", () => {
        assert.equal(cmdFocusDirectional("win-a", "sideways"), null);
        assert.equal(cmdMoveDirectional("win-a", "left", "sideways"), null);
        assert.equal(cmdSendDirectional("win-a", "nowhere", false), null);
        assert.equal(cmdMigrateDirectional("diagonal"), null);
    });

    it("carries no endpoint or cross flag", () => {
        const focus = cmdFocusDirectional("win-a", "right") as unknown as Record<string, unknown>;
        assert.ok(!("out" in focus) && !("crossOutput" in focus) && !("floatSubject" in focus));
        const move = cmdMoveDirectional("win-a", "right", "group-with-neighbor") as unknown as Record<string, unknown>;
        assert.ok(!("out" in move) && !("crossOutput" in move));
        const send = cmdSendDirectional("win-a", "right", true) as unknown as Record<string, unknown>;
        assert.ok(!("out" in send) && !("ws" in send));
        const migrate = cmdMigrateDirectional("right") as unknown as Record<string, unknown>;
        assert.ok(!("to" in migrate) && !("crossOutput" in migrate) && !("from" in migrate) && !("ws" in migrate));
    });

    it("names typed send sources, never bare strings", () => {
        assert.deepEqual(explicitSource("out-a"), { explicit: "out-a" });
        assert.deepEqual(implicitSource("home-then-action"), { implicit: "home-then-action" });
    });
});
