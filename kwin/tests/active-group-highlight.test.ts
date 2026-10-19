import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    ACTIVE_GROUP_MAX_REPLY_BYTES,
    formatGroupHighlightPayload,
    GROUP_HIGHLIGHT_CLEAR_METHOD,
    GROUP_HIGHLIGHT_INTERFACE,
    GROUP_HIGHLIGHT_OBJECT,
    GROUP_HIGHLIGHT_SERVICE,
    GROUP_HIGHLIGHT_SET_METHOD,
} from "../src/active-group-highlight";

const OWNER = "owner-1";
const GENERATION = "gen-1";
const CORRELATION = "gen-1-g0";

// Retained formatter regression coverage for the trimmed group-highlight
// boundary: the world route forwards core-projected results through these
// endpoint constants and this payload formatter only. The DescribePlan
// client, reply parser, and request builder are retired with the legacy
// plan route.

describe("group highlight effect payload", () => {
    it("formats only identity plus ordered member ids plus union bounds for the effect setter", () => {
        const payload = formatGroupHighlightPayload(CORRELATION, OWNER, GENERATION, 2, "group-1", "win-2", ["win-1", "win-2"], {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        });
        assert.ok(payload !== null);
        assert.deepEqual(JSON.parse(payload as string), {
            v: 1,
            correlation_id: CORRELATION,
            owner: OWNER,
            generation: GENERATION,
            revision: 2,
            group: "group-1",
            focused_window: "win-2",
            members: ["win-1", "win-2"],
            bounds: { x: 0, y: 0, w: 1200, h: 800 },
        });
        assert.ok((payload as string).length <= ACTIVE_GROUP_MAX_REPLY_BYTES);
        // Same normalized id kind as focused_window; ordering preserved.
        assert.equal(formatGroupHighlightPayload(CORRELATION, OWNER, GENERATION, 2, "group-1", "win-2", [], { x: 0, y: 0, w: 1200, h: 800 }), null);
        assert.equal(formatGroupHighlightPayload(CORRELATION, OWNER, GENERATION, 2, "group-1", "win-2", ["win-☃"], { x: 0, y: 0, w: 1200, h: 800 }), null);
        assert.equal(formatGroupHighlightPayload(CORRELATION, OWNER, GENERATION, 2, "group-1", "win-2", ["win-1", "win-1"], { x: 0, y: 0, w: 1200, h: 800 }), null);
    });

    it("pins the owned effect endpoint constants and the formatter-only module shape", () => {
        assert.equal(GROUP_HIGHLIGHT_SERVICE, "com.omnitiler.ActiveBorder");
        assert.equal(GROUP_HIGHLIGHT_OBJECT, "/com/omnitiler/ActiveBorder");
        assert.equal(GROUP_HIGHLIGHT_INTERFACE, "com.omnitiler.ActiveBorder1");
        assert.equal(GROUP_HIGHLIGHT_SET_METHOD, "SetGroupHighlight");
        assert.equal(GROUP_HIGHLIGHT_CLEAR_METHOD, "ClearGroupHighlight");
        assert.equal(ACTIVE_GROUP_MAX_REPLY_BYTES, 65536);
        const module = readFileSync("src/active-group-highlight.ts", "utf8");
        assert.ok(module.includes("SetGroupHighlight"));
        assert.ok(module.includes("ClearGroupHighlight"));
        assert.ok(module.includes("formatGroupHighlightPayload"));
        // No DescribePlan transport client remains on this boundary; the
        // world route carries the core result and only formats here.
        assert.ok(!module.includes("DescribePlan"));
        assert.ok(!module.includes("callDescribePlan"));
        assert.ok(!module.includes("parseActiveGroupReply"));
        assert.ok(!module.includes("buildActiveGroupRequest"));
        assert.ok(!module.includes("class ActiveGroupHighlight"));
        assert.ok(!module.includes("startActiveGroupHighlight"));
        assert.ok(!module.includes("/Effects"));
        assert.ok(!module.includes("reconfigureEffect"));
        assert.ok(!module.includes("registerShortcut"));
    });
});
