// V2 validated pass-through: the wire carries
// `visible,reason,subject,outsets,width,radius,witness`, never absolute
// `inner`/`outer`. The v2 formatter forwards the exact 7-key descriptor
// unchanged, even with null or unjoined witness; native event authority
// owns the narrow border hide. Malformed shapes fail closed to null
// (the caller hides).
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

// Verbatim anchored shape from the Rust v4 fixture: visible with a joined
// witness, uniform outsets, and explicit width/radius.
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

describe("world presentation v2 anchored border", () => {
    it("forwards hidden anchored rows verbatim", () => {
        const parsed = replyObject("pres-x", HIDDEN_BORDER);
        const payload = formatPresentationPayload(parsed);
        assert.ok(payload !== null);
        const body = JSON.parse(payload) as Record<string, unknown>;
        assert.equal(body["v"], 2);
        assert.deepEqual(body["border"], HIDDEN_BORDER);
        assert.equal(body["foreground"], "win-a");
        assert.equal(body["coveredInstance"], "effect-1");
        assert.equal(body["coveredInvalid"], "5");
    });

    it("forwards every hidden reason token verbatim", () => {
        for (const reason of ["", "disabled", "no-target", "minimized", "maximized", "fullscreen", "host-hidden", "hidden-workspace", "shell", "suspended"]) {
            const border = { ...HIDDEN_BORDER, reason };
            const parsed = replyObject("pres-x", border);
            const payload = formatPresentationPayload(parsed);
            assert.ok(payload !== null, `reason ${reason} forwards`);
            assert.deepEqual((JSON.parse(payload) as Record<string, unknown>)["border"], border);
        }
    });

    it("forwards joined visible anchored rows verbatim", () => {
        const parsed = replyObject("pres-x", VISIBLE_BORDER);
        assert.equal(parsed.presentation.border.visible, true);
        const payload = formatPresentationPayload(parsed);
        assert.ok(payload !== null);
        const body = JSON.parse(payload) as Record<string, unknown>;
        assert.equal(body["v"], 2);
        assert.deepEqual(body["border"], VISIBLE_BORDER);
    });

    it("passes visible rows with null witness unchanged (native authority pending)", () => {
        const border = { ...VISIBLE_BORDER, witness: null };
        const parsed = replyObject("pres-x", border);
        assert.equal(parsed.presentation.border.visible, true);
        const payload = formatPresentationPayload(parsed);
        assert.ok(payload !== null);
        assert.deepEqual((JSON.parse(payload) as Record<string, unknown>)["border"], border);
    });

    it("passes visible rows with dangling or unjoined witness unchanged (native authority pending)", () => {
        const nullSubject = { ...VISIBLE_BORDER, subject: null };
        const winMismatch = {
            ...VISIBLE_BORDER,
            witness: { win: "win-b", obs: "1", frame: [8, 8, 400, 600], foreground: "win-a" },
        };
        const fgMismatch = {
            ...VISIBLE_BORDER,
            witness: { win: "win-a", obs: "1", frame: [8, 8, 400, 600], foreground: "win-b" },
        };
        for (const border of [nullSubject, winMismatch, fgMismatch]) {
            const parsed = replyObject("pres-x", border);
            const payload = formatPresentationPayload(parsed);
            assert.ok(payload !== null);
            assert.deepEqual((JSON.parse(payload) as Record<string, unknown>)["border"], border);
        }
    });

    it("fails closed to null on malformed border shapes", () => {
        const malformed: Array<Record<string, unknown>> = [
            { ...VISIBLE_BORDER, reason: "no-target" },
            { ...VISIBLE_BORDER, reason: "cloaked" },
            { ...VISIBLE_BORDER, outsets: [3, 3, 3] },
            { ...VISIBLE_BORDER, width: -1 },
            { ...HIDDEN_BORDER, witness: { win: "win-a", obs: "1", frame: [8, 8, 400, 600] } },
            { ...VISIBLE_BORDER, extra: 1 },
        ];
        for (const border of malformed) {
            const text = presentationReply("pres-x", border);
            const parsed = parseWorldReply(text);
            if (parsed !== null && isWorldPresentationReply(parsed)) {
                assert.equal(formatPresentationPayload(parsed), null, `formatter rejects ${JSON.stringify(border).slice(0, 80)}`);
            } else {
                assert.equal(parsed, null, `lane rejects ${JSON.stringify(border).slice(0, 80)}`);
            }
        }
    });

    it("paints hidden and visible rows through the subscriber", () => {
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
            HIDDEN_BORDER,
        );
        sub.notifyInvalidated();
        assert.equal(calls.length, 1);
        const req2 = (JSON.parse(calls[0]?.payload as string) as { req: string }).req;
        const hidesBefore = hides;
        const paintsBefore = paints.length;
        calls.splice(0)[0]?.callback(presentationReply(req2, VISIBLE_BORDER, { rev: "2", obs: "2" }));
        assert.equal(paints.length, paintsBefore + 1);
        assert.deepEqual(
            (JSON.parse(paints[paints.length - 1] as string) as Record<string, unknown>)["border"],
            VISIBLE_BORDER,
        );
        assert.equal((JSON.parse(paints[paints.length - 1] as string) as Record<string, unknown>)["rev"], "2");
        assert.equal(hides, hidesBefore);
        assert.ok(!logs.some((l) => l.includes("presentation-malformed")), logs.join("\n"));
        // A repeated valid read repaints nothing and hides nothing.
        sub.notifyInvalidated();
        assert.equal(calls.length, 1);
        const req3 = (JSON.parse(calls[0]?.payload as string) as { req: string }).req;
        calls.splice(0)[0]?.callback(presentationReply(req3, VISIBLE_BORDER, { rev: "2", obs: "2" }));
        assert.equal(paints.length, paintsBefore + 1);
        assert.equal(hides, hidesBefore);
        assert.ok(!logs.some((l) => l.includes("presentation-malformed")), logs.join("\n"));
    });
});
