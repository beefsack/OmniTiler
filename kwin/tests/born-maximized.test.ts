import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    BORN_MAXIMIZED_DEFAULT,
    BORN_MAXIMIZED_OVERLAY,
    normalizeBornMaximized,
    readBornMaximizedValue,
} from "../src/born-maximized";

// R-MAX-06 KDE `bornMaximized` uses the `bornMaximized` script/kwinrc key,
// following the existing camelCase convention (`sameAxisMove` <->
// `core.same_axis_move`): unknown or missing values fall back to the
// `tile-on-launch` default, matching the Windows `core.born_maximized`
// tokens exactly.

function entryXml(path: string): string {
    return readFileSync(path, "utf8");
}

describe("born-maximized setting validation", () => {
    it("defaults to tile-on-launch", () => {
        assert.equal(BORN_MAXIMIZED_DEFAULT, "tile-on-launch");
        assert.equal(BORN_MAXIMIZED_OVERLAY, "keep-maximized-overlay");
        assert.equal(readBornMaximizedValue(), "tile-on-launch");
    });

    it("accepts exactly tile-on-launch and keep-maximized-overlay while rejecting the rest to tile-on-launch", () => {
        assert.equal(normalizeBornMaximized("tile-on-launch"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("keep-maximized-overlay"), "keep-maximized-overlay");
        assert.equal(normalizeBornMaximized(undefined), "tile-on-launch");
        assert.equal(normalizeBornMaximized(null), "tile-on-launch");
        assert.equal(normalizeBornMaximized(""), "tile-on-launch");
        assert.equal(normalizeBornMaximized("tile"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("COSMIC"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("keep_maximized_overlay"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("bornMaximized"), "tile-on-launch");
        assert.equal(normalizeBornMaximized("tile-on-launch "), "tile-on-launch");
        assert.equal(normalizeBornMaximized(0), "tile-on-launch");
        assert.equal(readBornMaximizedValue(() => "keep-maximized-overlay"), "keep-maximized-overlay");
        assert.equal(readBornMaximizedValue(() => "bogus"), "tile-on-launch");
        assert.equal(
            readBornMaximizedValue(() => {
                throw new Error("boom");
            }),
            "tile-on-launch",
        );
    });
});

describe("born-maximized schema and native KCM persistence", () => {
    it("declares bornMaximized as an Enum setting defaulting to tile-on-launch", () => {
        const schema = entryXml("contents/config/main.xml");
        const match = schema.match(/<entry name="bornMaximized" type="Enum">([\s\S]*?)<\/entry>/);
        assert.ok(match !== null, "bornMaximized must be declared");
        const body = match[1] as string;
        assert.match(body, /<default>tile-on-launch<\/default>/);
        assert.match(body, /<choice name="tile-on-launch" value="tile-on-launch"\/>/);
        assert.match(body, /<choice name="keep-maximized-overlay" value="keep-maximized-overlay"\/>/);
        const ui = entryXml("contents/ui/config.ui");
        assert.match(ui, /name="kcfg_bornMaximized"/);
    });

    it("persists bornMaximized through the native script KCM with validation", () => {
        const module = entryXml("native-effect/unifiedsettings_module.cpp");
        const adapter = entryXml("native-effect/kdesettings_adapter.cpp");
        const descriptors = entryXml("native-effect/generated/kde_settings_descriptors.h");
        const ui = entryXml("native-effect/unifiedsettings.ui");
        assert.match(ui, /name="bornMaximizedCombo"/);
        assert.match(ui, /name="label_bornMaximized"[\s\S]*?bornMaximizedCombo/);
        assert.match(descriptors, /"bornMaximized", "bornMaximizedCombo"/);
        assert.match(descriptors, /kli18n\("Tile on launch"\)/);
        assert.match(descriptors, /kli18n\("Keep maximized overlay"\)/);
        assert.match(descriptors, /kli18n\("bspwm, i3, xmonad, sway, qtile"\)/);
        assert.match(descriptors, /kli18n\("COSMIC, Hyprland"\)/);
        assert.match(adapter, /readEnumToken/);
        assert.match(module, /KdeSettingsAdapter::configureNativeControls\(widget\(\)\)/);
        assert.match(module, /KdeSettingsAdapter::readEnumToken\(/);
        assert.match(module, /writeEntry\(QStringLiteral\("bornMaximized"\)/);
        assert.match(module, /KdeSettingsAdapter::selectEnumCombo\(/);
    });
});

interface BornFixtureCase {
    name: string;
    raw?: unknown;
    normalized: string;
}

// Shared R-MAX-06 normalization table (also consumed by tiler-core
// `born_maximized` unit tests): each row pins the raw setting and the
// normalized token. The legacy adapter clear-vs-keep half is retired with
// the plan route; world-side admission is covered by world-born-maximized
// tests.
describe("born-maximized shared conformance fixture", () => {
    const fixture = JSON.parse(
        readFileSync("../test-fixtures/born-maximized.json", "utf8"),
    ) as { cases: BornFixtureCase[] };
    assert.ok(fixture.cases.length > 0, "fixture carries a case table");
    for (const kase of fixture.cases) {
        it(`${kase.name}: normalizes to ${kase.normalized}`, () => {
            const raw = "raw" in kase ? kase.raw : undefined;
            assert.equal(normalizeBornMaximized(raw), kase.normalized);
        });
    }
});
