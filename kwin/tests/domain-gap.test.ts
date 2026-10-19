import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    DOMAIN_GAP,
    OUTER_DOMAIN_GAP,
    normalizeGap,
    readDomainGaps,
    readInnerGapValue,
    readOuterGapValue,
} from "../src/domain-gap";

function entryXml(path: string): string {
    return readFileSync(path, "utf8");
}

describe("bounded gap schema and native KCM persistence", () => {
    it("declares innerGap and outerGap as bounded Int settings defaulting to 8", () => {
        const schema = entryXml("contents/config/main.xml");
        for (const key of ["innerGap", "outerGap"]) {
            const match = schema.match(
                new RegExp(`<entry name="${key}" type="Int">([\\s\\S]*?)</entry>`),
            );
            assert.ok(match !== null, `${key} must be declared`);
            const body = match[1] as string;
            assert.match(body, /<default>8<\/default>/);
            assert.match(body, /<min>0<\/min>/);
            assert.match(body, /<max>64<\/max>/);
        }
        const ui = entryXml("contents/ui/config.ui");
        for (const key of ["innerGap", "outerGap"]) {
            assert.match(ui, new RegExp(`name="kcfg_${key}"`));
        }
    });

    it("persists gaps through the native script KCM with bounded validation", () => {
        const module = entryXml("native-effect/unifiedsettings_module.cpp");
        const adapter = entryXml("native-effect/kdesettings_adapter.cpp");
        const descriptors = entryXml("native-effect/generated/kde_settings_descriptors.h");
        const ui = entryXml("native-effect/unifiedsettings.ui");
        for (const key of ["innerGap", "outerGap"]) {
            assert.match(descriptors, new RegExp(`"${key}", "${key}SpinBox"`));
            assert.match(descriptors, new RegExp(`"8", "0", "64"`));
            assert.match(adapter, /readBoundedInt/);
            assert.match(adapter, /isBoundedIntRawValid/);
            assert.match(module, /KdeSettingsAdapter::readBoundedInt\(/);
            assert.match(module, new RegExp(`writeEntry\\(QStringLiteral\\("${key}"\\)`));
            assert.match(ui, new RegExp(`name="${key}SpinBox"`));
            assert.match(ui, new RegExp(`name="label_${key}"[\\s\\S]*?${key}SpinBox`));
        }
        assert.match(module, /m_ui\.innerGapSpinBox->setValue\(innerGap\)/);
        assert.match(module, /m_ui\.outerGapSpinBox->setValue\(outerGap\)/);
        assert.match(module, /KdeSettingsAdapter::configureNativeControls\(widget\(\)\)/);
        assert.match(adapter, /configureBoundedSpinBox/);
        assert.match(ui, /name="innerGapSpinBox"[\s\S]*?<number>64<\/number>/);
        assert.match(ui, /name="outerGapSpinBox"[\s\S]*?<number>64<\/number>/);
    });
});

describe("validated gap binding", () => {
    it("preserves the default effective pair (8, 8)", () => {
        assert.equal(DOMAIN_GAP, 8);
        assert.equal(OUTER_DOMAIN_GAP, 8);
        assert.deepEqual(readDomainGaps(), { innerGap: 8, outerGap: 8 });
    });

    it("accepts boundaries and representative values while rejecting the rest to 8", () => {
        assert.equal(normalizeGap(0), 0);
        assert.equal(normalizeGap(64), 64);
        assert.equal(normalizeGap(12), 12);
        assert.equal(normalizeGap("20"), 20);
        assert.equal(normalizeGap(-1), 8);
        assert.equal(normalizeGap(65), 8);
        assert.equal(normalizeGap(7.5), 8);
        assert.equal(normalizeGap("bogus"), 8);
        assert.equal(normalizeGap(undefined), 8);
        assert.equal(normalizeGap(null), 8);
        assert.equal(readInnerGapValue(() => 12), 12);
        assert.equal(readOuterGapValue(() => 20), 20);
        assert.equal(readInnerGapValue(() => 65), 8);
        assert.equal(readOuterGapValue(() => -1), 8);
        assert.equal(
            readInnerGapValue(() => {
                throw new Error("boom");
            }),
            8,
        );
        assert.deepEqual(readDomainGaps({ readInnerGapFn: () => 0, readOuterGapFn: () => 64 }), {
            innerGap: 0,
            outerGap: 64,
        });
        assert.deepEqual(readDomainGaps({ readInnerGapFn: () => "bogus", readOuterGapFn: () => 99 }), {
            innerGap: 8,
            outerGap: 8,
        });
    });
});
