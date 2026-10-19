import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    FIXED_SIZE_PREDICATE_DEFAULT,
    FIXED_SIZE_PREDICATE_EITHER,
    normalizeFixedSizePredicate,
    readFixedSizePredicateValue,
} from "../src/fixed-size-predicate";

function entryXml(path: string): string {
    return readFileSync(path, "utf8");
}

describe("fixed-size predicate setting validation", () => {
    it("defaults to both-axes-fixed", () => {
        assert.equal(FIXED_SIZE_PREDICATE_DEFAULT, "both-axes-fixed");
        assert.equal(FIXED_SIZE_PREDICATE_EITHER, "either-axis-fixed");
        assert.equal(readFixedSizePredicateValue(), "both-axes-fixed");
    });

    it("accepts exactly both-axes-fixed and either-axis-fixed while rejecting the rest", () => {
        assert.equal(normalizeFixedSizePredicate("both-axes-fixed"), "both-axes-fixed");
        assert.equal(normalizeFixedSizePredicate("either-axis-fixed"), "either-axis-fixed");
        assert.equal(normalizeFixedSizePredicate(undefined), "both-axes-fixed");
        assert.equal(normalizeFixedSizePredicate(null), "both-axes-fixed");
        assert.equal(normalizeFixedSizePredicate(""), "both-axes-fixed");
        assert.equal(normalizeFixedSizePredicate("both"), "both-axes-fixed");
        assert.equal(normalizeFixedSizePredicate("fixedSizePredicate"), "both-axes-fixed");
        assert.equal(normalizeFixedSizePredicate(0), "both-axes-fixed");
        assert.equal(readFixedSizePredicateValue(() => "either-axis-fixed"), "either-axis-fixed");
        assert.equal(readFixedSizePredicateValue(() => "bogus"), "both-axes-fixed");
        assert.equal(
            readFixedSizePredicateValue(() => {
                throw new Error("boom");
            }),
            "both-axes-fixed",
        );
    });
});

describe("fixed-size predicate schema and native KCM persistence", () => {
    it("declares fixedSizePredicate as an Enum setting defaulting to both-axes-fixed", () => {
        const schema = entryXml("contents/config/main.xml");
        const match = schema.match(/<entry name="fixedSizePredicate" type="Enum">([\s\S]*?)<\/entry>/);
        assert.ok(match !== null, "fixedSizePredicate must be declared");
        const body = match[1] as string;
        assert.match(body, /<default>both-axes-fixed<\/default>/);
        assert.match(body, /<choice name="both-axes-fixed" value="both-axes-fixed"\/>/);
        assert.match(body, /<choice name="either-axis-fixed" value="either-axis-fixed"\/>/);
        const ui = entryXml("contents/ui/config.ui");
        assert.match(ui, /name="kcfg_fixedSizePredicate"/);
    });

    it("persists fixedSizePredicate through the native script KCM with validation", () => {
        const module = entryXml("native-effect/unifiedsettings_module.cpp");
        const adapter = entryXml("native-effect/kdesettings_adapter.cpp");
        const descriptors = entryXml("native-effect/generated/kde_settings_descriptors.h");
        const ui = entryXml("native-effect/unifiedsettings.ui");
        assert.match(ui, /name="fixedSizePredicateCombo"/);
        assert.match(ui, /name="label_fixedSizePredicate"[\s\S]*?fixedSizePredicateCombo/);
        assert.match(descriptors, /"fixedSizePredicate", "fixedSizePredicateCombo"/);
        assert.match(descriptors, /kli18n\("Width and height both fixed"\)/);
        assert.match(descriptors, /kli18n\("Width or height fixed"\)/);
        assert.match(adapter, /readEnumToken/);
        assert.match(module, /KdeSettingsAdapter::configureNativeControls\(widget\(\)\)/);
        assert.match(module, /KdeSettingsAdapter::readEnumToken\(/);
        assert.match(module, /writeEntry\(QStringLiteral\("fixedSizePredicate"\)/);
        assert.match(module, /KdeSettingsAdapter::selectEnumCombo\(/);
    });
});
