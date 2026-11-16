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
    type WorldCommand,
} from "../src/world-report";
import { registerWorldShortcuts, type WorldShortcutsEnv } from "../src/world-shortcuts";

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

    it("submits explicit empty win for directional focus with no active window", () => {
        const commands: WorldCommand[] = [];
        const failures: string[] = [];
        const registered = new Map<string, () => void>();
        const env: WorldShortcutsEnv = {
            readProp: (value, property) => Reflect.get(value as object, property),
            isDirection: (value): value is "left" | "right" | "up" | "down" =>
                value === "left" || value === "right" || value === "up" || value === "down",
            shortcutFailed: (action) => { failures.push(action); },
            submitCommand: (command, _label) => { commands.push(command); return true; },
            submitCatalogMove: () => {},
            submitOutputSend: () => false,
            submitMigration: () => false,
            submitFloat: () => false,
            activeOutputName: () => "out-a",
            activeWindowId: () => null,
            activeWindowRef: () => null,
            currentScopeId: () => "out-a/ws-a",
            registerShortcutFn: (name, _text, _seq, callback) => { registered.set(name, callback); return true; },
        };
        registerWorldShortcuts(env);
        registered.get("omnitiler-focus-left")?.();
        assert.deepEqual(commands, [{ op: "focus-directional", win: "", direction: "left" }]);
        assert.deepEqual(failures, []);
    });

    it("still refuses move with no active window", () => {
        const commands: WorldCommand[] = [];
        const failures: string[] = [];
        const registered = new Map<string, () => void>();
        const env: WorldShortcutsEnv = {
            readProp: (value, property) => Reflect.get(value as object, property),
            isDirection: (value): value is "left" | "right" | "up" | "down" =>
                value === "left" || value === "right" || value === "up" || value === "down",
            shortcutFailed: (action) => { failures.push(action); },
            submitCommand: (command, _label) => { commands.push(command); return true; },
            submitCatalogMove: () => {},
            submitOutputSend: () => false,
            submitMigration: () => false,
            submitFloat: () => false,
            activeOutputName: () => "out-a",
            activeWindowId: () => null,
            activeWindowRef: () => null,
            currentScopeId: () => "out-a/ws-a",
            registerShortcutFn: (name, _text, _seq, callback) => { registered.set(name, callback); return true; },
        };
        registerWorldShortcuts(env);
        registered.get("omnitiler-move-left")?.();
        assert.deepEqual(commands, []);
        assert.deepEqual(failures, ["move-left"]);
    });

    it("refuses directional focus when a foreign raw foreground exists but the filtered id is null", () => {
        const commands: WorldCommand[] = [];
        const failures: string[] = [];
        const registered = new Map<string, () => void>();
        const env: WorldShortcutsEnv = {
            readProp: (value, property) => Reflect.get(value as object, property),
            isDirection: (value): value is "left" | "right" | "up" | "down" =>
                value === "left" || value === "right" || value === "up" || value === "down",
            shortcutFailed: (action) => { failures.push(action); },
            submitCommand: (command, _label) => { commands.push(command); return true; },
            submitCatalogMove: () => {},
            submitOutputSend: () => false,
            submitMigration: () => false,
            submitFloat: () => false,
            activeOutputName: () => "out-a",
            activeWindowId: () => null,
            activeWindowRef: () => ({ internalId: "win-foreign" }),
            currentScopeId: () => "out-a/ws-a",
            registerShortcutFn: (name, _text, _seq, callback) => { registered.set(name, callback); return true; },
        };
        registerWorldShortcuts(env);
        registered.get("omnitiler-focus-left")?.();
        assert.deepEqual(commands, []);
        assert.deepEqual(failures, ["focus-left"]);
    });
});
