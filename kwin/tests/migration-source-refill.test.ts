// R-WS-12 G-37 migration source-refill setting offline coverage: the
// functional ids and their validation, the pure refill selector, the config
// schema plus native KCM persistence, the entry-owned Options configChanged
// live reread for subsequent migrations, and the session map-commit
// discrimination (MRU WS1 vs last trailing empty E, no-history/deleted/
// migrated fallbacks, surviving-empty eligibility, live-current priority,
// and a mode change between two subsequent commits). Included in `npm test`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    MIGRATION_SOURCE_REFILL_DEFAULT,
    MIGRATION_SOURCE_REFILL_MRU,
    normalizeMigrationSourceRefill,
    readMigrationSourceRefillValue,
    selectMigrationSourceRefill,
} from "../src/migration-source-refill";
import { startPlanAdapterEntry } from "../src/plan-adapter-entry";
import { WorkspaceNativeAdapter } from "../src/workspace-native";

function entryFile(path: string): string {
    return readFileSync(path, "utf8");
}

describe("migration source-refill setting validation", () => {
    it("defaults to last-remaining-workspace", () => {
        assert.equal(MIGRATION_SOURCE_REFILL_DEFAULT, "last-remaining-workspace");
        assert.equal(MIGRATION_SOURCE_REFILL_MRU, "most-recently-used-workspace");
        assert.equal(readMigrationSourceRefillValue(), "last-remaining-workspace");
    });

    it("accepts exactly the two functional ids while rejecting the rest", () => {
        assert.equal(normalizeMigrationSourceRefill("last-remaining-workspace"), "last-remaining-workspace");
        assert.equal(normalizeMigrationSourceRefill("most-recently-used-workspace"), "most-recently-used-workspace");
        assert.equal(normalizeMigrationSourceRefill(undefined), "last-remaining-workspace");
        assert.equal(normalizeMigrationSourceRefill(null), "last-remaining-workspace");
        assert.equal(normalizeMigrationSourceRefill(""), "last-remaining-workspace");
        assert.equal(normalizeMigrationSourceRefill("mru"), "last-remaining-workspace");
        assert.equal(normalizeMigrationSourceRefill("last-remaining"), "last-remaining-workspace");
        assert.equal(normalizeMigrationSourceRefill("migrationSourceRefill"), "last-remaining-workspace");
        assert.equal(normalizeMigrationSourceRefill(0), "last-remaining-workspace");
        assert.equal(
            readMigrationSourceRefillValue(() => "most-recently-used-workspace"),
            "most-recently-used-workspace",
        );
        assert.equal(readMigrationSourceRefillValue(() => "bogus"), "last-remaining-workspace");
        assert.equal(
            readMigrationSourceRefillValue(() => {
                throw new Error("boom");
            }),
            "last-remaining-workspace",
        );
    });
});

describe("migration source-refill pure selector", () => {
    it("refills the eligible remembered id under MRU and the last id by default", () => {
        assert.equal(
            selectMigrationSourceRefill("most-recently-used-workspace", "ws-1", ["ws-1", "ws-e"]),
            "ws-1",
        );
        assert.equal(
            selectMigrationSourceRefill("last-remaining-workspace", "ws-1", ["ws-1", "ws-e"]),
            "ws-e",
        );
    });

    it("falls back to last remaining without an eligible entry", () => {
        // No history.
        assert.equal(
            selectMigrationSourceRefill("most-recently-used-workspace", undefined, ["ws-1", "ws-e"]),
            "ws-e",
        );
        assert.equal(
            selectMigrationSourceRefill("most-recently-used-workspace", null, ["ws-1", "ws-e"]),
            "ws-e",
        );
        // Remembered id is the migrated one (excluded from remaining).
        assert.equal(
            selectMigrationSourceRefill("most-recently-used-workspace", "ws-2", ["ws-1", "ws-e"]),
            "ws-e",
        );
        // Remembered id was deleted or moved out of scope.
        assert.equal(
            selectMigrationSourceRefill("most-recently-used-workspace", "ws-gone", ["ws-1", "ws-e"]),
            "ws-e",
        );
        // Empty previous never matches.
        assert.equal(
            selectMigrationSourceRefill("most-recently-used-workspace", "", ["ws-1", "ws-e"]),
            "ws-e",
        );
        // Nothing remains: no recreation.
        assert.equal(selectMigrationSourceRefill("most-recently-used-workspace", "ws-1", []), null);
        assert.equal(selectMigrationSourceRefill("last-remaining-workspace", "ws-1", []), null);
    });

    it("accepts a surviving empty remembered id", () => {
        assert.equal(
            selectMigrationSourceRefill("most-recently-used-workspace", "ws-empty", ["ws-empty", "ws-e"]),
            "ws-empty",
        );
    });
});

describe("migration source-refill schema and native KCM persistence", () => {
    it("declares migrationSourceRefill as an Enum setting defaulting to last-remaining-workspace", () => {
        const schema = entryFile("contents/config/main.xml");
        const match = schema.match(/<entry name="migrationSourceRefill" type="Enum">([\s\S]*?)<\/entry>/);
        assert.ok(match !== null, "migrationSourceRefill must be declared");
        const body = match[1] as string;
        assert.match(body, /<label>Source workspace after migration<\/label>/);
        assert.match(body, /<default>last-remaining-workspace<\/default>/);
        assert.match(body, /<choice name="last-remaining-workspace" value="last-remaining-workspace"\/>/);
        assert.match(body, /<choice name="most-recently-used-workspace" value="most-recently-used-workspace"\/>/);
        const ui = entryFile("contents/ui/config.ui");
        assert.match(ui, /name="kcfg_migrationSourceRefill"/);
        assert.match(ui, /name="label_migrationSourceRefill"[\s\S]*?Source workspace after migration/);
    });

    it("persists migrationSourceRefill through the native script KCM with validation", () => {
        const module = entryFile("native-effect/unifiedsettings_module.cpp");
        const adapter = entryFile("native-effect/kdesettings_adapter.cpp");
        const descriptors = entryFile("native-effect/generated/kde_settings_descriptors.h");
        const ui = entryFile("native-effect/unifiedsettings.ui");
        assert.match(ui, /name="migrationSourceRefillCombo"/);
        assert.match(ui, /name="label_migrationSourceRefill"[\s\S]*?migrationSourceRefillCombo/);
        assert.match(descriptors, /"migrationSourceRefill", "migrationSourceRefillCombo"/);
        assert.match(descriptors, /kli18n\("Last remaining workspace"\)/);
        assert.match(descriptors, /kli18n\("Most recently used workspace"\)/);
        assert.match(descriptors, /kli18n\("COSMIC"\)/);
        assert.match(descriptors, /kli18n\("bspwm, i3, awesome"\)/);
        assert.match(adapter, /readEnumToken/);
        assert.match(module, /KdeSettingsAdapter::configureNativeControls\(widget\(\)\)/);
        assert.match(module, /KdeSettingsAdapter::readEnumToken\(/);
        assert.match(module, /writeEntry\(QStringLiteral\("migrationSourceRefill"\)/);
        assert.match(module, /KdeSettingsAdapter::selectEnumCombo\(/);
    });
});

describe("migration source-refill live setting on Options configChanged", () => {
    it("re-reads the refill on configChanged for subsequent migrations", () => {
        let refill: unknown = "last-remaining-workspace";
        const handlers: Array<() => void> = [];
        const options = {
            configChanged: {
                connect: (handler: () => void): void => { handlers.push(handler); },
                disconnect: (handler: () => void): void => {
                    const index = handlers.indexOf(handler);
                    if (index >= 0) { handlers.splice(index, 1); }
                },
            },
        };
        const logs: string[] = [];
        const output: Record<string, unknown> = { name: "out-1" };
        const desktop: Record<string, unknown> = { id: "ws-1" };
        const inert = (): unknown => ({ connect: (): void => {}, disconnect: (): void => {} });
        const winA: Record<string, unknown> = {
            normalWindow: true,
            internalId: "win-a",
            resourceClass: "test-app",
            output,
            desktops: [desktop],
            frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
            moveResizedChanged: inert(),
            fullScreenChanged: inert(),
            fullScreen: false,
            maximizedChanged: inert(),
            maximizeMode: 0,
            desktopsChanged: inert(),
            onAllDesktops: false,
        };
        const workspace = {
            activeWindow: winA,
            windowList: (): unknown[] => [winA],
            screens: [output],
            currentDesktopForScreen: (): unknown => desktop,
            clientArea: (): unknown => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowAdded: inert(),
            windowRemoved: inert(),
            windowActivated: inert(),
            screensChanged: inert(),
            currentDesktopChanged: inert(),
        };
        const handle = startPlanAdapterEntry({
            workspace,
            options,
            callDbus: (_s, _p, _i, method, _payload, callback): void => {
                if (method === "NameHasOwner") { callback(true); return; }
                if (method === "GetNameOwner") { callback(":1.7"); return; }
                if (method === "StartServiceByName") { callback(1); return; }
                void callback;
            },
            scheduleOnce: () => () => {},
            log: (message): void => { logs.push(message); },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: () => true,
            readProfileFn: () => "cosmic",
            readMigrationSourceRefillFn: (): unknown => refill,
        });
        assert.ok(handle !== null);
        refill = "most-recently-used-workspace";
        for (const fire of [...handlers]) {
            fire();
        }
        assert.ok(
            logs.some((line) =>
                line === "omnitiler:plan:config-reloaded stage=migration-source-refill refill=most-recently-used-workspace"
            ),
        );
        assert.ok(!logs.some((line) => line.includes("restart-required")));
        // An unchanged re-read sends nothing more.
        const before = logs.length;
        for (const fire of [...handlers]) {
            fire();
        }
        assert.equal(logs.length, before);
        handle?.stop();
    });
});

// ---------- map-commit discrimination ----------

interface CommitOutput {
    name: string;
    manufacturer: string;
    model: string;
    serialNumber: string;
}

interface CommitDesktop {
    id: string;
    x11DesktopNumber: number;
}

interface CommitWindow {
    output: CommitOutput;
    desktops: CommitDesktop[];
}

function refillWorld(outputNames: string[], desktopIds: string[], refill: unknown): {
    adapter: WorkspaceNativeAdapter;
    workspace: Record<string, unknown>;
    outputs: CommitOutput[];
    desktops: CommitDesktop[];
    currentByOutput: Map<CommitOutput, CommitDesktop>;
    wins: CommitWindow[];
    logs: string[];
} {
    const logs: string[] = [];
    const outputs = outputNames.map((name) => ({
        name,
        manufacturer: "m",
        model: "d",
        serialNumber: `s-${name}`,
    }));
    const desktops = desktopIds.map((id, index) => ({ id, x11DesktopNumber: index + 1 }));
    const currentByOutput = new Map<CommitOutput, CommitDesktop>();
    for (const output of outputs) {
        const first = desktops[0];
        if (first !== undefined) {
            currentByOutput.set(output, first);
        }
    }
    const wins: CommitWindow[] = [];
    const workspace: Record<string, unknown> = {
        screens: outputs,
        desktops,
        activeWindow: null,
        activeScreen: outputs[0] ?? null,
        currentDesktopForScreen: (output: unknown): unknown => currentByOutput.get(output as CommitOutput) ?? null,
        setCurrentDesktopForScreen: (desktop: unknown, output: unknown): void => {
            currentByOutput.set(output as CommitOutput, desktop as CommitDesktop);
        },
        windowList: (): unknown[] => [...wins],
    };
    const adapter = new WorkspaceNativeAdapter({
        getWorkspace: () => workspace,
        readWorkspaceMode: () => "per-output-local",
        readMigrationSourceRefill: () => refill,
        log: (message) => {
            logs.push(message);
        },
    });
    assert.equal(adapter.enable(), true);
    return { adapter, workspace, outputs, desktops, currentByOutput, wins, logs };
}

function occupy(wins: CommitWindow[], output: CommitOutput, desktop: CommitDesktop): void {
    wins.push({ output, desktops: [desktop] });
}

describe("migrate map-commit source refill", () => {
    it("defaults to the last trailing empty while MRU refills the visited WS1", () => {
        for (const [mode, expected] of [
            ["last-remaining-workspace", "ws-e"],
            ["most-recently-used-workspace", "ws-1"],
        ] as const) {
            const world = refillWorld(["out-1", "out-2"], ["ws-1", "ws-2", "ws-e"], mode);
            const [out1, out2] = world.outputs as [CommitOutput, CommitOutput];
            const [ws1, ws2] = world.desktops as [CommitDesktop, CommitDesktop];
            occupy(world.wins, out1 as CommitOutput, ws1 as CommitDesktop);
            occupy(world.wins, out1 as CommitOutput, ws2 as CommitDesktop);
            // Visit WS1 then WS2: the item-1.2 previous entry names WS1.
            world.currentByOutput.set(out1 as CommitOutput, ws1 as CommitDesktop);
            world.currentByOutput.set(out2 as CommitOutput, ws1 as CommitDesktop);
            world.adapter.handleTopologySignal();
            world.currentByOutput.set(out1 as CommitOutput, ws2 as CommitDesktop);
            world.adapter.handleTopologySignal();
            assert.deepEqual(world.adapter.previousSnapshot(), { "output-0": "ws-1" });
            const committed = world.adapter.commitWorkspaceMigration(
                out1 as CommitOutput,
                out2 as CommitOutput,
                "ws-2",
            );
            assert.deepEqual(committed, { refillId: expected });
        }
    });

    it("falls back to last remaining with no history under MRU", () => {
        const world = refillWorld(
            ["out-1", "out-2"],
            ["ws-1", "ws-2", "ws-e"],
            "most-recently-used-workspace",
        );
        const [out1, out2] = world.outputs as [CommitOutput, CommitOutput];
        const [ws1, ws2] = world.desktops as [CommitDesktop, CommitDesktop];
        occupy(world.wins, out1 as CommitOutput, ws1 as CommitDesktop);
        occupy(world.wins, out1 as CommitOutput, ws2 as CommitDesktop);
        // No topology signal: no previous entry exists.
        assert.deepEqual(world.adapter.previousSnapshot(), {});
        world.currentByOutput.set(out1 as CommitOutput, ws2 as CommitDesktop);
        world.currentByOutput.set(out2 as CommitOutput, ws1 as CommitDesktop);
        assert.deepEqual(
            world.adapter.commitWorkspaceMigration(out1 as CommitOutput, out2 as CommitOutput, "ws-2"),
            { refillId: "ws-e" },
        );
    });

    it("falls back when the remembered id is the migrated one", () => {
        const world = refillWorld(
            ["out-1", "out-2"],
            ["ws-1", "ws-2", "ws-e"],
            "most-recently-used-workspace",
        );
        const [out1, out2] = world.outputs as [CommitOutput, CommitOutput];
        const [ws1, ws2] = world.desktops as [CommitDesktop, CommitDesktop];
        occupy(world.wins, out1 as CommitOutput, ws1 as CommitDesktop);
        occupy(world.wins, out1 as CommitOutput, ws2 as CommitDesktop);
        // Previous names WS2, which is then migrated away.
        world.currentByOutput.set(out1 as CommitOutput, ws2 as CommitDesktop);
        world.currentByOutput.set(out2 as CommitOutput, ws1 as CommitDesktop);
        world.adapter.handleTopologySignal();
        world.currentByOutput.set(out1 as CommitOutput, ws1 as CommitDesktop);
        world.adapter.handleTopologySignal();
        world.currentByOutput.set(out1 as CommitOutput, ws2 as CommitDesktop);
        assert.deepEqual(world.adapter.previousSnapshot(), { "output-0": "ws-2" });
        assert.deepEqual(
            world.adapter.commitWorkspaceMigration(out1 as CommitOutput, out2 as CommitOutput, "ws-2"),
            { refillId: "ws-e" },
        );
        // The migrated previous entry is invalidated like any 1.3 removal.
        assert.deepEqual(world.adapter.previousSnapshot(), {});
        assert.ok(world.logs.some((line) => line.includes("workspace-previous-invalidated:migrated")));
    });

    it("falls back when the remembered id was deleted", () => {
        const world = refillWorld(
            ["out-1", "out-2"],
            ["ws-1", "ws-2", "ws-e"],
            "most-recently-used-workspace",
        );
        const [out1, out2] = world.outputs as [CommitOutput, CommitOutput];
        const [ws1, ws2] = world.desktops as [CommitDesktop, CommitDesktop];
        occupy(world.wins, out1 as CommitOutput, ws1 as CommitDesktop);
        occupy(world.wins, out1 as CommitOutput, ws2 as CommitDesktop);
        world.currentByOutput.set(out1 as CommitOutput, ws1 as CommitDesktop);
        world.currentByOutput.set(out2 as CommitOutput, ws1 as CommitDesktop);
        world.adapter.handleTopologySignal();
        world.currentByOutput.set(out1 as CommitOutput, ws2 as CommitDesktop);
        world.adapter.handleTopologySignal();
        assert.deepEqual(world.adapter.previousSnapshot(), { "output-0": "ws-1" });
        // WS1 is deleted natively before the migration commits: the
        // remembered id no longer resolves, so MRU falls back.
        world.workspace["desktops"] = world.desktops.filter((entry) => entry.id !== "ws-1");
        world.adapter.handleTopologySignal();
        assert.deepEqual(world.adapter.previousSnapshot(), {});
        world.currentByOutput.set(out2 as CommitOutput, world.desktops[2] as CommitDesktop);
        assert.deepEqual(
            world.adapter.commitWorkspaceMigration(out1 as CommitOutput, out2 as CommitOutput, "ws-2"),
            { refillId: "ws-e" },
        );
    });

    it("refills a surviving empty remembered id under MRU", () => {
        for (const [mode, expected] of [
            ["last-remaining-workspace", "ws-e"],
            ["most-recently-used-workspace", "ws-1"],
        ] as const) {
            const world = refillWorld(["out-1", "out-2"], ["ws-1", "ws-2", "ws-e"], mode);
            const [out1, out2] = world.outputs as [CommitOutput, CommitOutput];
            const [ws1, ws2] = world.desktops as [CommitDesktop, CommitDesktop];
            // WS1 stays empty but listed; only WS2 is occupied.
            occupy(world.wins, out1 as CommitOutput, ws2 as CommitDesktop);
            world.currentByOutput.set(out1 as CommitOutput, ws1 as CommitDesktop);
            world.currentByOutput.set(out2 as CommitOutput, ws1 as CommitDesktop);
            world.adapter.handleTopologySignal();
            world.currentByOutput.set(out1 as CommitOutput, ws2 as CommitDesktop);
            world.adapter.handleTopologySignal();
            assert.deepEqual(world.adapter.previousSnapshot(), { "output-0": "ws-1" });
            assert.deepEqual(
                world.adapter.commitWorkspaceMigration(out1 as CommitOutput, out2 as CommitOutput, "ws-2"),
                { refillId: expected },
            );
        }
    });

    it("keeps a still-scoped live current in both modes", () => {
        for (const mode of ["last-remaining-workspace", "most-recently-used-workspace"] as const) {
            const world = refillWorld(["out-1", "out-2"], ["ws-1", "ws-2", "ws-e"], mode);
            const [out1, out2] = world.outputs as [CommitOutput, CommitOutput];
            const [ws1, ws2] = world.desktops as [CommitDesktop, CommitDesktop];
            occupy(world.wins, out1 as CommitOutput, ws1 as CommitDesktop);
            occupy(world.wins, out1 as CommitOutput, ws2 as CommitDesktop);
            world.currentByOutput.set(out1 as CommitOutput, ws2 as CommitDesktop);
            world.currentByOutput.set(out2 as CommitOutput, ws2 as CommitDesktop);
            world.adapter.handleTopologySignal();
            // Migrate WS1 while the live source view already shows the
            // surviving WS2: the visible survivor wins over MRU history.
            assert.deepEqual(
                world.adapter.commitWorkspaceMigration(out1 as CommitOutput, out2 as CommitOutput, "ws-1"),
                { refillId: "ws-2" },
            );
        }
    });

    it("applies a mode change to the subsequent migration only", () => {
        const world = refillWorld(
            ["out-1", "out-2"],
            ["ws-1", "ws-2", "ws-3", "ws-e"],
            "last-remaining-workspace",
        );
        const [out1, out2] = world.outputs as [CommitOutput, CommitOutput];
        const [ws1, ws2, ws3] = world.desktops as [CommitDesktop, CommitDesktop, CommitDesktop];
        occupy(world.wins, out1 as CommitOutput, ws1 as CommitDesktop);
        occupy(world.wins, out1 as CommitOutput, ws2 as CommitDesktop);
        occupy(world.wins, out1 as CommitOutput, ws3 as CommitDesktop);
        assert.equal(world.adapter.getMigrationSourceRefill(), "last-remaining-workspace");
        // First migration under the default: last remaining wins.
        world.currentByOutput.set(out1 as CommitOutput, ws1 as CommitDesktop);
        world.currentByOutput.set(out2 as CommitOutput, ws1 as CommitDesktop);
        world.adapter.handleTopologySignal();
        world.currentByOutput.set(out1 as CommitOutput, ws3 as CommitDesktop);
        world.adapter.handleTopologySignal();
        assert.deepEqual(
            world.adapter.commitWorkspaceMigration(out1 as CommitOutput, out2 as CommitOutput, "ws-3"),
            { refillId: "ws-e" },
        );
        // The entry-owned live reread pushes the new value: already-migrated
        // state is untouched, the next migration selects MRU.
        assert.equal(world.adapter.setMigrationSourceRefill("most-recently-used-workspace"), true);
        assert.equal(world.adapter.setMigrationSourceRefill("most-recently-used-workspace"), false);
        assert.equal(world.adapter.setMigrationSourceRefill("bogus"), true);
        assert.equal(world.adapter.getMigrationSourceRefill(), "last-remaining-workspace");
        assert.equal(world.adapter.setMigrationSourceRefill("most-recently-used-workspace"), true);
        assert.equal(world.adapter.getMigrationSourceRefill(), "most-recently-used-workspace");
        // Native views switched to the post-migration state, then WS1 is
        // visited before WS2 is migrated.
        world.currentByOutput.set(out1 as CommitOutput, ws1 as CommitDesktop);
        world.currentByOutput.set(out2 as CommitOutput, ws3 as CommitDesktop);
        world.adapter.handleTopologySignal();
        world.currentByOutput.set(out1 as CommitOutput, ws2 as CommitDesktop);
        world.adapter.handleTopologySignal();
        assert.deepEqual(
            world.adapter.commitWorkspaceMigration(out1 as CommitOutput, out2 as CommitOutput, "ws-2"),
            { refillId: "ws-1" },
        );
        assert.deepEqual(world.adapter.localSnapshot(), {
            "output-0": ["ws-1", "ws-e"],
            "output-1": ["ws-3", "ws-2"],
        });
    });
});
