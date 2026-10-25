// R-WS-12 G-37 migration source-refill setting offline coverage: the
// functional ids and their validation, the pure refill selector, and the
// config schema plus native KCM persistence. Included in `npm test`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    MIGRATION_SOURCE_REFILL_DEFAULT,
    MIGRATION_SOURCE_REFILL_MRU,
    normalizeMigrationSourceRefill,
    readMigrationSourceRefillValue,
} from "../src/world-settings-live";

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

// Config carriage only (validation + schema/KCM): the pure refill selector
// is retired, owned by the shared core `select_migration_source_refill` in
// crates/tiler-core/src/output_selection.rs (covered there).

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
