// World take-lane `result` and core `dragPreview` parsing (strict v4):
// correlated per-command outcomes arrive requester-only via
// TakeCommandResult (toggle-float, maximize, directional, rejected,
// migrate-workspace), while active gesture preview state rides the core
// WorldReply `dragPreview` lane (no `op`, canonical quoted u64 `id`/`rev`).
// Old core `result`, missing `dragPreview`, `drag-preview` on Take, retired
// group ops, bad counters, and malformed bodies refuse.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
    cmdToggleMaximize,
    isCoreWorldReply,
    isWorldRejection,
    parseWorldReply,
    type WorldDirectionalCause,
    type WorldDirectionalOutcome,
    type WorldRejectKind,
    type WorldReply,
    type WorldTakeReply,
} from "../src/world-report";

function baseTake(result: string): string {
    return `{"v":4,"outcome":"ready","gen":"gen-1","globalCmd":"9",${result}}`;
}

// Synthetic strict-v4 bound reply with disabled anchored presentation.
function baseCapture(dragPreview: string): string {
    return `{"v":4,"adapter":"broker-1","gen":"gen-1","req":"req-1",` +
        `"obs":"1","rev":"1","ready":true,"consumed":false,` +
        `"scopes":[{"scope":{"kind":"output","out":"out-1"},"auth":"controlled","reason":""}],` +
        `"workspaces":[],"outputs":[],"windows":[],` +
        `"transfers":[],"focusRequest":null,${dragPreview},` +
        `"gestureSettled":null,"operationSettled":[],"nativeCreations":[],"warpCursor":null,` +
        `"presentation":{"foreground":"","rev":"0","border":{"visible":false,"reason":"disabled",` +
        `"subject":null,"outsets":null,"width":null,"radius":null,"witness":null},` +
        `"underlay":{"status":"hidden","reason":"disabled","baseRev":null},"pager":[]},` +
        `"intentDesired":[],"switchSettled":[],"diagnostics":[],` +
        `"source":"req-1","nativeInvalid":"1","clock":"0","gestureAssoc":null,"gestureSettledAssoc":null}`;
}

function unwrapTake(text: string): WorldTakeReply {
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && !isWorldRejection(parsed));
    if (parsed === null || isWorldRejection(parsed)) {
        throw new Error("unreachable");
    }
    assert.equal((parsed as { outcome?: unknown }).outcome, "ready");
    return parsed as WorldTakeReply;
}

describe("world reply result and dragPreview", () => {
    it("parses dragPreview on core WorldReply and every non-preview op on TakeCommandResult", () => {
        const preview = parseWorldReply(baseCapture(
            `"dragPreview":{"id":"1","rev":"7","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a",` +
            `"hover":{"domainOut":"out-1","domainWs":"ws-1","leaf":"leaf-a","win":"win-a",` +
            `"rev":"7","prior":{"group":"grp","edge":"left"},"priorOut":"out-1","priorWs":"ws-1"}}`,
        ));
        assert.ok(preview !== null && isCoreWorldReply(preview));
        if (preview !== null && isCoreWorldReply(preview)) {
            assert.ok(preview.dragPreview !== null);
            assert.equal(preview.dragPreview?.id, "1");
            assert.deepEqual(preview.dragPreview?.rect, [0, 0, 396, 600]);
            assert.equal(preview.dragPreview?.hover?.win, "win-a");
            assert.equal(preview.dragPreview?.hover?.prior?.edge, "left");
            assert.equal(preview.dragPreview?.rev, "7");
        }
        const previewNoHover = parseWorldReply(baseCapture(
            `"dragPreview":{"id":"2","rev":"8","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a","hover":null}`,
        ));
        assert.ok(previewNoHover !== null && isCoreWorldReply(previewNoHover));
        if (previewNoHover !== null && isCoreWorldReply(previewNoHover)) {
            assert.equal(previewNoHover.dragPreview?.id, "2");
            assert.equal(previewNoHover.dragPreview?.hover, null);
        }

        const float = unwrapTake(baseTake(
            `"result":{"op":"toggle-float","win":"win-a","rect":[10,10,300,300]}`,
        ));
        if (float.outcome === "ready") {
            assert.deepEqual(float.result, { op: "toggle-float", win: "win-a", rect: [10, 10, 300, 300], noSpace: false });
        } else {
            assert.fail("expected ready");
        }
        const floatNoSpace = unwrapTake(baseTake(
            `"result":{"op":"toggle-float","win":"win-a","rect":null,"noSpace":true}`,
        ));
        if (floatNoSpace.outcome === "ready") {
            assert.deepEqual(floatNoSpace.result, { op: "toggle-float", win: "win-a", rect: null, noSpace: true });
        } else {
            assert.fail("expected ready");
        }
        const unfloat = unwrapTake(baseTake(
            `"result":{"op":"toggle-float","win":"win-a","rect":null}`,
        ));
        if (unfloat.outcome === "ready") {
            assert.deepEqual(unfloat.result, { op: "toggle-float", win: "win-a", rect: null, noSpace: false });
        } else {
            assert.fail("expected ready");
        }
        const migrate = unwrapTake(baseTake(
            `"result":{"op":"migrate-workspace","direction":"right",` +
            `"sourceOut":"out-a","sourceWs":"wa1","targetOut":"out-b","targetWs":"wa1",` +
            `"members":["win-a","win-b"],"floats":1}`,
        ));
        if (migrate.outcome === "ready") {
            assert.equal(migrate.result?.op, "migrate-workspace");
            if (migrate.result?.op === "migrate-workspace") {
                assert.deepEqual(migrate.result.members, ["win-a", "win-b"]);
                assert.equal(migrate.result.floats, 1);
            }
        } else {
            assert.fail("expected ready");
        }
    });

    it("parses all 16 directional outcomes and 5 nullable causes on TakeCommandResult", () => {
        const outcomes: WorldDirectionalOutcome[] = [
            "focus-refused-floating", "move-refused-floating",
            "focus-refused-workspace-floating", "move-refused-workspace-floating",
            "move-refused-fullscreen", "focus-refused-maximize",
            "focus-float-retained", "focus-unfocused-retained",
            "focus-float-refused-stale", "move-float-refused-stale",
            "move-float-refused-invalid-geometry", "move-float-refused-constraints",
            "move-float-unknown-output", "focus-float-applied", "move-float-applied",
            "move-maximize-clear",
        ];
        const causes: Array<WorldDirectionalCause | null> = [
            null, "no-target", "unknown-window", "focus-present", "no-source", "no-adjacent-output",
        ];
        for (const outcome of outcomes) {
            for (const cause of causes) {
                const parsed = unwrapTake(baseTake(
                    `"result":{"op":"directional","outcome":${JSON.stringify(outcome)},"win":"win-a","rect":[10,10,300,300],"cause":${JSON.stringify(cause)}}`,
                ));
                assert.equal(parsed.outcome, "ready");
                if (parsed.outcome === "ready") {
                    assert.deepEqual(parsed.result, {
                        op: "directional", outcome, win: "win-a", rect: [10, 10, 300, 300], cause,
                    });
                }
            }
        }
        assert.equal(parseWorldReply(baseTake(
            `"result":{"op":"directional","outcome":"unknown-outcome","win":null,"rect":null,"cause":null}`,
        )), null);
        assert.equal(parseWorldReply(baseTake(
            `"result":{"op":"directional","outcome":"focus-float-applied","win":null,"rect":null,"cause":"unknown-cause"}`,
        )), null);
    });

    it("parses all 17 rejection kinds on TakeCommandResult", () => {
        const kinds: WorldRejectKind[] = [
            "focus-unknown", "send-target-unknown", "send-window-unknown", "send-refused",
            "view-output-unknown", "view-target-unknown", "transfer-in-flight", "direction-invalid",
            "scope-held", "selection-unknown", "migration-refused", "operation-refused",
            "tiling-target-unknown", "state-unsupported", "state-unknown", "fullscreen-not-exitable",
            "stale-render-target",
        ];
        for (const kind of kinds) {
            const parsed = unwrapTake(baseTake(`"result":{"op":"rejected","kind":${JSON.stringify(kind)}}`));
            assert.equal(parsed.outcome, "ready");
            if (parsed.outcome === "ready") {
                assert.deepEqual(parsed.result, { op: "rejected", kind });
            }
        }
    });

    it("parses the maximize result with exact preimage provenance on TakeCommandResult", () => {
        assert.deepEqual(cmdToggleMaximize("win-a"), { op: "toggle-maximize", win: "win-a" });
        const parsed = unwrapTake(baseTake(
            `"result":{"op":"maximize","win":"win-a","maximized":true,"clearFullscreen":true,"wasMaximized":false}`,
        ));
        if (parsed.outcome === "ready") {
            assert.equal(parsed.result?.op, "maximize");
            if (parsed.result?.op === "maximize") {
                assert.equal(parsed.result.win, "win-a");
                assert.equal(parsed.result.maximized, true);
                assert.equal(parsed.result.clearFullscreen, true);
                assert.equal(parsed.result.wasMaximized, false);
            }
        } else {
            assert.fail("expected ready");
        }
        const both = unwrapTake(baseTake(
            `"result":{"op":"maximize","win":"win-a","maximized":true,"clearFullscreen":true,"wasMaximized":true}`,
        ));
        if (both.outcome === "ready") {
            assert.equal(both.result?.op, "maximize");
        } else {
            assert.fail("expected ready");
        }
        assert.equal(parseWorldReply(baseTake(
            `"result":{"op":"maximize","win":"win-a","maximized":true,"clearFullscreen":true}`,
        )), null, "missing preimage refuses");
        assert.equal(parseWorldReply(baseTake(
            `"result":{"op":"maximize","win":"win-a","maximized":"yes","clearFullscreen":false,"wasMaximized":false}`,
        )), null, "non-boolean maximized refuses");
        assert.equal(parseWorldReply(baseTake(
            `"result":{"op":"maximize","win":"win a","maximized":true,"clearFullscreen":false,"wasMaximized":false}`,
        )), null, "non-opaque win refuses");
    });

    it("reads a null take result as the snapshot-only outcome", () => {
        const parsed = parseWorldReply(
            `{"v":4,"outcome":"ready","gen":"gen-1","globalCmd":"9","result":null}`,
        );
        assert.ok(parsed !== null && !isWorldRejection(parsed));
        const outcome = (parsed as { outcome?: unknown } | null)?.outcome;
        if (parsed !== null && !isWorldRejection(parsed) && outcome === "ready") {
            assert.equal((parsed as Extract<WorldTakeReply, { outcome: "ready" }>).result, null);
        } else {
            assert.fail("expected ready with null result");
        }
    });

    it("refuses retired ops, legacy core result, missing dragPreview, and malformed shapes", () => {
        // Retired group ops and drag-preview refuse on TakeCommandResult.
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"active-group","rev":"3","group":"grp"}`)), null);
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"no-group","reason":"pending","rev":null}`)), null);
        assert.equal(parseWorldReply(baseTake(
            `"result":{"op":"drag-preview","rev":"7","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a","hover":null}`,
        )), null, "drag-preview is removed from TakeCommandResult");
        // Core reply with null dragPreview decodes; legacy result or missing dragPreview refuses.
        const capture = parseWorldReply(baseCapture(`"dragPreview":null`));
        assert.ok(capture !== null && isCoreWorldReply(capture));
        assert.equal((capture as WorldReply).dragPreview, null);
        assert.equal(parseWorldReply(baseCapture(`"result":null`)), null, "legacy result key on core reply refuses");
        assert.equal(parseWorldReply(baseCapture(`"dragPreview":null,"result":null`)), null, "extra result key on core reply refuses");
        // Malformed and counter-overflow dragPreview shapes refuse.
        assert.equal(parseWorldReply(baseCapture(
            `"dragPreview":{"op":"drag-preview","id":"1","rev":"7","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a","hover":null}`,
        )), null, "op key on dragPreview refuses");
        assert.equal(parseWorldReply(baseCapture(
            `"dragPreview":{"id":"01","rev":"7","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a","hover":null}`,
        )), null, "non-canonical id refuses");
        assert.equal(parseWorldReply(baseCapture(
            `"dragPreview":{"id":"18446744073709551616","rev":"7","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a","hover":null}`,
        )), null, "u64 overflow id refuses");
        assert.equal(parseWorldReply(baseCapture(
            `"dragPreview":{"id":"1","rev":"07","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a","hover":null}`,
        )), null, "non-canonical rev refuses");
        assert.equal(parseWorldReply(baseCapture(
            `"dragPreview":{"id":"1","rev":"7","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,-1,600],"win":"win-a","hover":null}`,
        )), null, "invalid rect refuses");
        // Take-lane strictness.
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"move","win":"win-a"}`)), null);
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"toggle-float","win":"win-a","rect":null,"noSpace":false}`)), null, "present-but-false noSpace refuses");
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"toggle-float","win":"win a","rect":null}`)), null, "non-opaque win refuses");
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"rejected","kind":"bogus"}`)), null, "unknown reject kind refuses");
        assert.equal(parseWorldReply(baseTake(
            `"result":{"op":"migrate-workspace","direction":"sideways",` +
            `"sourceOut":"out-a","sourceWs":"wa1","targetOut":"out-b","targetWs":"wa1",` +
            `"members":[],"floats":0}`,
        )), null);
        assert.equal(parseWorldReply(baseTake(`"result":[]`)), null);
    });
});
