// V4 anchored-border subscriber consumption (K1): the wire carries
// `visible,reason,subject,outsets,width,radius,witness`, never absolute
// `inner`/`outer`. K1 shows no OmniTiler active border: hidden and visible
// anchored rows alike project to `visible:0` with null rects (no drawable
// geometry). Script never resolves `inner`/`outer` from a live read
// outside the native lifetime/freshness floors, and the DTO carries no
// subject channel, so a visible row sends the honest hidden shape instead
// of failing the whole projection.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
    createWorldPresentationSubscriber,
    formatPresentationPayload,
    PRESENTATION_READ_METHOD,
} from "../src/world-presentation";
import {
    isWorldPresentationReply,
    parseWorldReply,
    type WorldPresentationReply,
} from "../src/world-report";

const GEN = "gen-1";

function presentationReply(
    req: string,
    border: Record<string, unknown>,
    extra?: Record<string, unknown>,
): string {
    return JSON.stringify({
        v: 4,
        outcome: "presentation",
        gen: GEN,
        req,
        rev: "1",
        obs: "1",
        coveredInstance: "effect-1",
        coveredInvalid: "5",
        scopes: [],
        workspaces: [],
        outputs: [],
        windows: [],
        presentation: {
            foreground: "win-a",
            rev: "1",
            border,
            underlay: { status: "hidden", reason: "no-group", baseRev: null },
            pager: [],
        },
        preview: null,
        ...(extra ?? {}),
    });
}

const HIDDEN_BORDER = {
    visible: false,
    reason: "no-target",
    subject: null,
    outsets: null,
    width: null,
    radius: null,
    witness: null,
};

// Verbatim anchored shape from the Rust v4 fixture
// (test-fixtures/world-wire/v4/messages/reply-observe.json presentation
// border, plus the anchored-border scenario witness frame): visible with a
// joined witness, uniform outsets, and explicit width/radius.
const VISIBLE_BORDER = {
    visible: true,
    reason: "",
    subject: "win-a",
    outsets: [3, 3, 3, 3],
    width: 3,
    radius: 6,
    witness: { win: "win-a", obs: "1", frame: [8, 8, 400, 600], foreground: "win-a" },
};

function replyObject(req: string, border: Record<string, unknown>): WorldPresentationReply {
    const parsed = parseWorldReply(presentationReply(req, border));
    if (parsed === null || !isWorldPresentationReply(parsed)) {
        throw new Error("v4 presentation fixture refused");
    }
    return parsed;
}

describe("world presentation v4 anchored border", () => {
    it("forwards hidden anchored rows with null rects", () => {
        const parsed = replyObject("pres-x", HIDDEN_BORDER);
        const payload = formatPresentationPayload(parsed);
        assert.ok(payload !== null);
        const body = JSON.parse(payload) as Record<string, unknown>;
        assert.deepEqual(body["border"], { visible: 0, inner: null, outer: null });
        assert.equal(body["foreground"], "win-a");
        assert.equal(body["coveredInstance"], "effect-1");
        assert.equal(body["coveredInvalid"], "5");
    });

    it("projects visible anchored rows to hidden instead of fabricating geometry", () => {
        const parsed = replyObject("pres-x", VISIBLE_BORDER);
        assert.equal(parsed.presentation.border.visible, true);
        const payload = formatPresentationPayload(parsed);
        assert.ok(payload !== null);
        const body = JSON.parse(payload) as Record<string, unknown>;
        assert.deepEqual(body["border"], { visible: 0, inner: null, outer: null });
        assert.equal(body["foreground"], "win-a");
        assert.equal(body["coveredInstance"], "effect-1");
        assert.equal(body["coveredInvalid"], "5");
    });

    it("paints hidden and paints disabled-hidden for visible-true through the subscriber", () => {
        const calls: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
        const paints: string[] = [];
        const logs: string[] = [];
        let hides = 0;
        const sub = createWorldPresentationSubscriber({
            callDbus: (_s, _p, _i, method, payload, callback) => {
                assert.equal(method, PRESENTATION_READ_METHOD);
                calls.push({ payload, callback });
            },
            getGeneration: () => GEN,
            getInvalidation: () => ({ instance: "effect-1", counter: "5" }),
            isStopped: () => false,
            log: (m) => { logs.push(m); },
            scheduleOnce: () => () => {},
            renderer: {
                renderPresentation: (payload) => {
                    paints.push(payload);
                    return true;
                },
                hidePresentation: () => {
                    hides += 1;
                },
                clearPresentation: () => {},
            },
        });
        sub.notifyGeneration(GEN);
        assert.equal(calls.length, 1);
        const req = (JSON.parse(calls[0]?.payload as string) as { req: string }).req;
        calls.splice(0)[0]?.callback(presentationReply(req, HIDDEN_BORDER));
        assert.equal(paints.length, 1);
        assert.deepEqual(
            (JSON.parse(paints[0] as string) as Record<string, unknown>)["border"],
            { visible: 0, inner: null, outer: null },
        );
        sub.notifyInvalidated();
        assert.equal(calls.length, 1);
        const req2 = (JSON.parse(calls[0]?.payload as string) as { req: string }).req;
        const hidesBefore = hides;
        const paintsBefore = paints.length;
        calls.splice(0)[0]?.callback(presentationReply(req2, VISIBLE_BORDER, { rev: "2", obs: "2" }));
        // The visible row paints the honest hidden shape: the previously
        // held paint is replaced, never hidden as malformed, and no
        // fail-closed noise is logged for valid traffic.
        assert.equal(paints.length, paintsBefore + 1);
        assert.deepEqual(
            (JSON.parse(paints[paints.length - 1] as string) as Record<string, unknown>)["border"],
            { visible: 0, inner: null, outer: null },
        );
        assert.equal((JSON.parse(paints[paints.length - 1] as string) as Record<string, unknown>)["rev"], "2");
        assert.equal(hides, hidesBefore);
        assert.ok(!logs.some((l) => l.includes("presentation-malformed")), logs.join("\n"));
        // A repeated valid hidden read repaints nothing and hides nothing.
        sub.notifyInvalidated();
        assert.equal(calls.length, 1);
        const req3 = (JSON.parse(calls[0]?.payload as string) as { req: string }).req;
        calls.splice(0)[0]?.callback(presentationReply(req3, VISIBLE_BORDER, { rev: "2", obs: "2" }));
        assert.equal(paints.length, paintsBefore + 1);
        assert.equal(hides, hidesBefore);
        assert.ok(!logs.some((l) => l.includes("presentation-malformed")), logs.join("\n"));
    });
});
