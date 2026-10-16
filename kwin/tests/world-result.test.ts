// World reply `result` parsing (D1-D5): the correlated per-command
// outcome coexists with the snapshot. Every op shape parses strictly;
// pre-contract replies without `result` still parse (null); unknown ops,
// bad counters, and malformed bodies refuse the whole reply.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { parseWorldReply, isWorldRejection, type WorldReply } from "../src/world-report";

function baseReply(result: string): string {
    return `{"v":1,"adapter":"adapter-1","gen":"gen-1","req":"req-1",` +
        `"obs":"1","rev":"1","ready":true,"consumed":false,` +
        `"scopes":[],"workspaces":[],"outputs":[],"windows":[],` +
        `"transfers":[],"effects":[],${result}}`;
}

function unwrap(text: string): WorldReply {
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && !isWorldRejection(parsed));
    return parsed as WorldReply;
}

describe("world reply result", () => {
    it("parses every correlated op with exact provenance", () => {
        const preview = unwrap(baseReply(
            `"result":{"op":"drag-preview","rev":"7","domainOut":"out-1","domainWs":"ws-1",` +
            `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],` +
            `"hover":{"domainOut":"out-1","domainWs":"ws-1","leaf":"leaf-a","win":"win-a",` +
            `"rev":"7","prior":{"group":"grp","edge":"left"},"priorOut":"out-1","priorWs":"ws-1"}}`,
        ));
        assert.equal(preview.result?.op, "drag-preview");
        if (preview.result?.op === "drag-preview") {
            assert.deepEqual(preview.result.rect, [0, 0, 396, 600]);
            assert.equal(preview.result.hover?.win, "win-a");
            assert.equal(preview.result.hover?.prior?.edge, "left");
            assert.equal(preview.result.rev, "7");
        }
        const float = unwrap(baseReply(
            `"result":{"op":"toggle-float","win":"win-a","rect":[10,10,300,300]}`,
        ));
        assert.equal(float.result?.op, "toggle-float");
        const unfloat = unwrap(baseReply(
            `"result":{"op":"toggle-float","win":"win-a","rect":null}`,
        ));
        assert.equal(unfloat.result?.op, "toggle-float");
        const group = unwrap(baseReply(
            `"result":{"op":"active-group","rev":"3","group":"grp","leaf":"leaf-a","win":"win-a",` +
            `"bounds":[0,0,800,600],"members":[{"leaf":"leaf-a","win":"win-a","rect":[0,0,400,600]}]}`,
        ));
        assert.equal(group.result?.op, "active-group");
        const nogroup = unwrap(baseReply(
            `"result":{"op":"no-group","reason":"no-parent-group","rev":null}`,
        ));
        assert.equal(nogroup.result?.op, "no-group");
        const migrate = unwrap(baseReply(
            `"result":{"op":"migrate-workspace","direction":"right",` +
            `"sourceOut":"out-a","sourceWs":"wa1","targetOut":"out-b","targetWs":"wa1",` +
            `"members":["win-a","win-b"],"floats":1}`,
        ));
        assert.equal(migrate.result?.op, "migrate-workspace");
        if (migrate.result?.op === "migrate-workspace") {
            assert.deepEqual(migrate.result.members, ["win-a", "win-b"]);
            assert.equal(migrate.result.floats, 1);
        }
    });

    it("reads a missing result as null for pre-contract replies", () => {
        const parsed = parseWorldReply(
            `{"v":1,"adapter":"adapter-1","gen":"gen-1","req":"req-1",` +
            `"obs":"1","rev":"1","ready":true,"consumed":false,` +
            `"scopes":[],"workspaces":[],"outputs":[],"windows":[],` +
            `"transfers":[],"effects":[]}`,
        );
        assert.ok(parsed !== null && !isWorldRejection(parsed));
        assert.equal((parsed as WorldReply).result, null);
    });

    it("refuses unknown ops and malformed bodies without partial reads", () => {
        assert.equal(parseWorldReply(baseReply(`"result":{"op":"move","win":"win-a"}`)), null);
        assert.equal(parseWorldReply(baseReply(`"result":{"op":"drag-preview"}`)), null);
        assert.equal(
            parseWorldReply(baseReply(
                `"result":{"op":"drag-preview","rev":"07","domainOut":"out-1","domainWs":"ws-1",` +
                `"sourceOut":"out-1","sourceWs":"ws-1","rect":[0,0,396,600],"hover":null}`,
            )),
            null,
            "non-canonical rev refuses",
        );
        assert.equal(
            parseWorldReply(baseReply(
                `"result":{"op":"toggle-float","win":"win a","rect":null}`,
            )),
            null,
            "non-opaque win refuses",
        );
        assert.equal(
            parseWorldReply(baseReply(
                `"result":{"op":"no-group","reason":"bogus","rev":null}`,
            )),
            null,
        );
        assert.equal(
            parseWorldReply(baseReply(
                `"result":{"op":"migrate-workspace","direction":"sideways",` +
                `"sourceOut":"out-a","sourceWs":"wa1","targetOut":"out-b","targetWs":"wa1",` +
                `"members":[],"floats":0}`,
            )),
            null,
        );
        assert.equal(parseWorldReply(baseReply(`"result":[]`)), null);
    });
});
