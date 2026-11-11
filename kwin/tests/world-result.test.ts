// World take-lane `result` parsing (strict v4): the correlated
// per-command outcome arrives requester-only via TakeCommandResult; the
// bound core reply decodes the same result truthfully (the broker strips it
// to null on the actuator lane service-side, never a parser default).
// Every v4 op shape (drag-preview, toggle-float, maximize, directional,
// rejected, migrate-workspace) parses strictly; null (or a missing key)
// means the snapshot alone is the outcome; unknown ops, retired group ops,
// bad counters, and malformed bodies refuse.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
    cmdToggleMaximize,
    isCoreWorldReply,
    isWorldRejection,
    parseWorldReply,
    type WorldReply,
    type WorldTakeReply,
} from "../src/world-report";

function baseTake(result: string): string {
    return `{"v":4,"outcome":"ready","gen":"gen-1","globalCmd":"9",${result}}`;
}

// Synthetic strict-v4 bound reply with disabled anchored presentation.
function baseCapture(result: string): string {
    return `{"v":4,"adapter":"broker-1","gen":"gen-1","req":"req-1",` +
        `"obs":"1","rev":"1","ready":true,"consumed":false,` +
        `"scopes":[{"scope":{"kind":"output","out":"out-1"},"auth":"controlled","reason":""}],` +
        `"workspaces":[],"outputs":[],"windows":[],` +
        `"transfers":[],"effects":[],"closeFocus":null,${result},` +
        `"gestureSettled":null,"operationSettled":[],"nativeCreations":[],"warpCursor":null,` +
        `"presentation":{"foreground":"","rev":"0","border":{"visible":false,"reason":"disabled",` +
        `"subject":null,"outsets":null,"width":null,"radius":null,"witness":null},` +
        `"underlay":{"status":"hidden","reason":"disabled","baseRev":null},"pager":[]},` +
        `"intentWrites":[],"switchSettled":[],"diagnostics":[],` +
        `"source":"req-1","nativeInvalid":"1","clock":"0","gestureAssoc":null}`;
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

describe("world reply result", () => {
    it("parses every correlated op with exact provenance", () => {
        const preview = unwrapTake(baseTake(
            `"result":{"op":"drag-preview","rev":"7","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a",` +
            `"hover":{"domainOut":"out-1","domainWs":"ws-1","leaf":"leaf-a","win":"win-a",` +
            `"rev":"7","prior":{"group":"grp","edge":"left"},"priorOut":"out-1","priorWs":"ws-1"}}`,
        ));
        assert.equal(preview.outcome, "ready");
        if (preview.outcome === "ready") {
            assert.equal(preview.result?.op, "drag-preview");
            if (preview.result?.op === "drag-preview") {
                assert.deepEqual(preview.result.rect, [0, 0, 396, 600]);
                assert.equal(preview.result.hover?.win, "win-a");
                assert.equal(preview.result.hover?.prior?.edge, "left");
                assert.equal(preview.result.rev, "7");
            }
        }
        const float = unwrapTake(baseTake(
            `"result":{"op":"toggle-float","win":"win-a","rect":[10,10,300,300]}`,
        ));
        if (float.outcome === "ready") {
            assert.equal(float.result?.op, "toggle-float");
        } else {
            assert.fail("expected ready");
        }
        const unfloat = unwrapTake(baseTake(
            `"result":{"op":"toggle-float","win":"win-a","rect":null}`,
        ));
        if (unfloat.outcome === "ready") {
            assert.equal(unfloat.result?.op, "toggle-float");
        } else {
            assert.fail("expected ready");
        }
        const direction = unwrapTake(baseTake(
            `"result":{"op":"directional","outcome":"focus-float-applied","win":"win-a",` +
            `"rect":[10,10,300,300],"cause":null}`,
        ));
        if (direction.outcome === "ready") {
            assert.equal(direction.result?.op, "directional");
        } else {
            assert.fail("expected ready");
        }
        const rejected = unwrapTake(baseTake(
            `"result":{"op":"rejected","kind":"focus-unknown"}`,
        ));
        if (rejected.outcome === "ready") {
            assert.equal(rejected.result?.op, "rejected");
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

    it("parses the maximize result with exact preimage provenance", () => {
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
        const boundMaximize = parseWorldReply(baseCapture(
            `"result":{"op":"maximize","win":"win-a","maximized":true,"clearFullscreen":true,"wasMaximized":false}`,
        ));
        assert.ok(boundMaximize !== null && isCoreWorldReply(boundMaximize), "bound lane decodes the terminal maximize result truthfully");
        assert.equal((boundMaximize as WorldReply).result?.op, "maximize");
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

    it("refuses retired ops and malformed take bodies, decodes the bound lane truthfully", () => {
        // Retired group ops refuse on both lanes (no v4 contract).
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"active-group","rev":"3","group":"grp"}`)), null);
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"no-group","reason":"pending","rev":null}`)), null);
        assert.equal(parseWorldReply(baseCapture(`"result":{"op":"active-group","rev":"3","group":"grp"}`)), null);
        assert.equal(parseWorldReply(baseCapture(`"result":{"op":"no-group","reason":"pending","rev":null}`)), null);
        const capture = parseWorldReply(baseCapture(`"result":null`));
        assert.ok(capture !== null && isCoreWorldReply(capture));
        assert.equal((capture as WorldReply).result, null);
        // Take-lane strictness mirrors the old reply checks.
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"move","win":"win-a"}`)), null);
        assert.equal(parseWorldReply(baseTake(`"result":{"op":"drag-preview"}`)), null);
        assert.equal(
            parseWorldReply(baseTake(
                `"result":{"op":"drag-preview","rev":"07","domainOut":"out-1","domainWs":"ws-1",` +
                `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"win":"win-a","hover":null}`,
            )),
            null,
            "non-canonical rev refuses",
        );
        assert.equal(
            parseWorldReply(baseTake(
                `"result":{"op":"toggle-float","win":"win a","rect":null}`,
            )),
            null,
            "non-opaque win refuses",
        );
        assert.equal(
            parseWorldReply(baseTake(
                `"result":{"op":"rejected","kind":"bogus"}`,
            )),
            null,
            "unknown reject kind refuses",
        );
        assert.equal(
            parseWorldReply(baseTake(
                `"result":{"op":"migrate-workspace","direction":"sideways",` +
                `"sourceOut":"out-a","sourceWs":"wa1","targetOut":"out-b","targetWs":"wa1",` +
                `"members":[],"floats":0}`,
            )),
            null,
        );
        assert.equal(parseWorldReply(baseTake(`"result":[]`)), null);
    });
});
