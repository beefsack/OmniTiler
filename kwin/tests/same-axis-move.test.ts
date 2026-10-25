import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    SAME_AXIS_MOVE_DEFAULT,
    SAME_AXIS_MOVE_SWAP,
    normalizeSameAxisMove,
    readSameAxisMoveValue,
} from "../src/world-settings-live";

function entryXml(path: string): string {
    return readFileSync(path, "utf8");
}

describe("same-axis move setting validation", () => {
    it("defaults to group-with-neighbor", () => {
        assert.equal(SAME_AXIS_MOVE_DEFAULT, "group-with-neighbor");
        assert.equal(SAME_AXIS_MOVE_SWAP, "swap-with-neighbor");
        assert.equal(readSameAxisMoveValue(), "group-with-neighbor");
    });

    it("accepts exactly group-with-neighbor and swap-with-neighbor while rejecting the rest to group-with-neighbor", () => {
        assert.equal(normalizeSameAxisMove("group-with-neighbor"), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove("swap-with-neighbor"), "swap-with-neighbor");
        assert.equal(normalizeSameAxisMove(undefined), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove(null), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove(""), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove("cosmic"), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove("COSMIC-WRAP"), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove("flat_swap"), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove("sameAxisMove"), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove("cosmic-wrap"), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove("flat-swap"), "group-with-neighbor");
        assert.equal(normalizeSameAxisMove(0), "group-with-neighbor");
        assert.equal(readSameAxisMoveValue(() => "swap-with-neighbor"), "swap-with-neighbor");
        assert.equal(readSameAxisMoveValue(() => "bogus"), "group-with-neighbor");
        assert.equal(
            readSameAxisMoveValue(() => {
                throw new Error("boom");
            }),
            "group-with-neighbor",
        );
    });
});

describe("same-axis move schema and native KCM persistence", () => {
    it("declares sameAxisMove as an Enum setting defaulting to group-with-neighbor", () => {
        const schema = entryXml("contents/config/main.xml");
        const match = schema.match(/<entry name="sameAxisMove" type="Enum">([\s\S]*?)<\/entry>/);
        assert.ok(match !== null, "sameAxisMove must be declared");
        const body = match[1] as string;
        assert.match(body, /<default>group-with-neighbor<\/default>/);
        assert.match(body, /<choice name="group-with-neighbor" value="group-with-neighbor"\/>/);
        assert.match(body, /<choice name="swap-with-neighbor" value="swap-with-neighbor"\/>/);
        const ui = entryXml("contents/ui/config.ui");
        assert.match(ui, /name="kcfg_sameAxisMove"/);
    });

    it("persists sameAxisMove through the native script KCM with validation", () => {
        const module = entryXml("native-effect/unifiedsettings_module.cpp");
        const adapter = entryXml("native-effect/kdesettings_adapter.cpp");
        const descriptors = entryXml("native-effect/generated/kde_settings_descriptors.h");
        const ui = entryXml("native-effect/unifiedsettings.ui");
        assert.match(ui, /name="sameAxisMoveCombo"/);
        assert.match(ui, /name="label_sameAxisMove"[\s\S]*?sameAxisMoveCombo/);
        assert.match(descriptors, /"sameAxisMove", "sameAxisMoveCombo"/);
        assert.match(descriptors, /kli18n\("Group with neighbor"\)/);
        assert.match(descriptors, /kli18n\("Swap with neighbor"\)/);
        assert.match(descriptors, /kli18n\("COSMIC"\)/);
        assert.match(descriptors, /kli18n\("i3, sway"\)/);
        assert.match(adapter, /readEnumToken/);
        assert.match(module, /KdeSettingsAdapter::configureNativeControls\(widget\(\)\)/);
        assert.match(module, /KdeSettingsAdapter::readEnumToken\(/);
        assert.match(module, /writeEntry\(QStringLiteral\("sameAxisMove"\)/);
        assert.match(module, /KdeSettingsAdapter::selectEnumCombo\(/);
    });
});
