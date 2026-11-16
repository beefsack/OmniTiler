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

    it("submits explicit empty win for directional focus when raw foreground is desktopWindow", () => {
        const commands: WorldCommand[] = [];
        const failures: string[] = [];
        const registered = new Map<string, () => void>();
        const desktopRef = { internalId: "desktop-1", desktopWindow: true, normalWindow: false };
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
            activeWindowRef: () => desktopRef,
            currentScopeId: () => "out-a/ws-a",
            registerShortcutFn: (name, _text, _seq, callback) => { registered.set(name, callback); return true; },
        };
        registerWorldShortcuts(env);
        registered.get("omnitiler-focus-left")?.();
        assert.deepEqual(commands, [{ op: "focus-directional", win: "", direction: "left" }]);
        assert.deepEqual(failures, []);
    });

    it("submits empty win when activeWindowId is non-null but raw foreground is desktop", () => {
        const commands: WorldCommand[] = [];
        const failures: string[] = [];
        const registered = new Map<string, () => void>();
        const desktopRef = { internalId: "desktop-1", desktopWindow: true, normalWindow: false };
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
            activeWindowId: () => "desktop-1",
            activeWindowRef: () => desktopRef,
            currentScopeId: () => "out-a/ws-a",
            registerShortcutFn: (name, _text, _seq, callback) => { registered.set(name, callback); return true; },
        };
        registerWorldShortcuts(env);
        registered.get("omnitiler-focus-left")?.();
        assert.deepEqual(commands, [{ op: "focus-directional", win: "", direction: "left" }]);
        assert.deepEqual(failures, []);
    });

    it("still refuses directional focus for non-true desktop markers and foreign dock/normal/fullscreen", () => {
        const cases: ReadonlyArray<object> = [
            { internalId: "dock-1", desktopWindow: false, normalWindow: false, dock: true },
            { internalId: "dock-1", normalWindow: false, dock: true } as object,
            { internalId: "normal-1", desktopWindow: false, normalWindow: true },
            { internalId: "normal-1", normalWindow: true } as object,
            { internalId: "full-1", desktopWindow: false, normalWindow: false, fullScreen: true },
            { internalId: "weird-1", desktopWindow: "true" },
            { internalId: "weird-2", desktopWindow: 1 },
            { internalId: "weird-3", desktopWindow: undefined },
            { internalId: "weird-4", desktopWindow: null },
        ];
        for (const ref of cases) {
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
                activeWindowRef: () => ref,
                currentScopeId: () => "out-a/ws-a",
                registerShortcutFn: (name, _text, _seq, callback) => { registered.set(name, callback); return true; },
            };
            registerWorldShortcuts(env);
            registered.get("omnitiler-focus-left")?.();
            assert.deepEqual(commands, []);
            assert.deepEqual(failures, ["focus-left"]);
        }
    });

    it("retains managed focus subject with normal foreground", () => {
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
            activeWindowId: () => "win-a",
            activeWindowRef: () => ({ internalId: "win-a", desktopWindow: false }),
            currentScopeId: () => "out-a/ws-a",
            registerShortcutFn: (name, _text, _seq, callback) => { registered.set(name, callback); return true; },
        };
        registerWorldShortcuts(env);
        registered.get("omnitiler-focus-left")?.();
        registered.get("omnitiler-move-left")?.();
        assert.deepEqual(commands, [
            { op: "focus-directional", win: "win-a", direction: "left" },
            { op: "move-directional", win: "win-a", direction: "left", sameAxis: "group-with-neighbor" },
        ]);
        assert.deepEqual(failures, []);
    });
});
