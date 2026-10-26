// World take-lane `result` parsing (strict v2): the correlated
// per-command outcome arrives requester-only via TakeCommandResult, never
// in the capture reply lane (a non-null capture result refuses). Every op
// shape parses strictly; null (or a missing key) means the snapshot alone
// is the outcome; unknown ops, bad counters, and malformed bodies refuse.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
    isCoreWorldReply,
    isWorldRejection,
    parseWorldReply,
    type WorldReply,
    type WorldTakeReply,
} from "../src/world-report";

function baseTake(result: string): string {
    return `{"v":2,"outcome":"ready","gen":"gen-1","globalCmd":"9",${result}}`;
}

function baseCapture(result: string): string {
    return `{"v":2,"adapter":"broker-1","gen":"gen-1","req":"req-1",` +
        `"obs":"1","rev":"1","ready":true,"consumed":false,` +
        `"scopes":[],"workspaces":[],"outputs":[],"windows":[],` +
        `"transfers":[],"effects":[],${result},` +
        `"gestureSettled":null,"gestureAssoc":null,"operationSettled":[],"nativeCreations":[],"warpCursor":null,` +
        `"presentation":{"foreground":"","border":{"visible":false,"reason":"disabled",` +
        `"inner":null,"outer":null},"underlay":{"status":"hidden","reason":"disabled",` +
        `"baseRev":null},"pager":[]},` +
        `"source":"req-1","nativeInvalid":"1","clock":"0"}`;
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
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],` +
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
        const group = unwrapTake(baseTake(
            `"result":{"op":"active-group","rev":"3","group":"grp","leaf":"leaf-a","win":"win-a",` +
            `"bounds":[0,0,800,600],"members":[{"leaf":"leaf-a","win":"win-a","rect":[0,0,400,600]}]}`,
        ));
        if (group.outcome === "ready") {
            assert.equal(group.result?.op, "active-group");
        } else {
            assert.fail("expected ready");
        }
        const nogroup = unwrapTake(baseTake(
            `"result":{"op":"no-group","reason":"no-parent-group","rev":null}`,
        ));
        if (nogroup.outcome === "ready") {
            assert.equal(nogroup.result?.op, "no-group");
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

    it("reads a null take result as the snapshot-only outcome", () => {
        const parsed = parseWorldReply(
            `{"v":2,"outcome":"ready","gen":"gen-1","globalCmd":"9","result":null}`,
        );
        assert.ok(parsed !== null && !isWorldRejection(parsed));
        const outcome = (parsed as { outcome?: unknown } | null)?.outcome;
        if (parsed !== null && !isWorldRejection(parsed) && outcome === "ready") {
            assert.equal((parsed as Extract<WorldTakeReply, { outcome: "ready" }>).result, null);
        } else {
            assert.fail("expected ready with null result");
        }
    });

    it("refuses a terminal result on the capture lane and malformed take bodies", () => {
        // The actuator lane never carries the terminal result.
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
                `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"hover":null}`,
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
                `"result":{"op":"no-group","reason":"bogus","rev":null}`,
            )),
            null,
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
