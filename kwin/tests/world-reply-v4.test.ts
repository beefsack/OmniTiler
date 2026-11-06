// Focused v4 reply-parser coverage (strict exact-shape decode): the parser
// accepts the actual Rust-shaped v4 fixtures (bound with the broker
// transport bindings) and refuses single-mutation violations exactly.
// Fixtures are READ ONLY: every mutation works on an in-memory clone.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { isCoreWorldReply, parseDiscoveryReply, parseWorldReply } from "../src/world-report";

const DIR = join("..", "test-fixtures", "world-wire", "v4", "messages");
const raw = (name: string): string => readFileSync(join(DIR, name), "utf8");
const doc = (name: string): Record<string, unknown> => JSON.parse(raw(name));
// Broker bind_response semantics: quoted-decimal watermark/clock plus a
// null gestureAssoc default on the bound capture lane.
const boundText = (body: Record<string, unknown>): string =>
    JSON.stringify({ ...body, source: "src-a", nativeInvalid: "1", clock: "2", gestureAssoc: null });
const clone = (name: string): Record<string, unknown> => JSON.parse(raw(name));

describe("world-reply-v4", () => {
    it("observe binds, anchored border, visibility lane, rev", () => {
        const parsed = parseWorldReply(boundText(doc("reply-observe.json")));
        assert.ok(isCoreWorldReply(parsed));
        const border = parsed.presentation.border;
        assert.equal(border.visible, true);
        assert.equal(border.reason, "");
        assert.deepEqual(border.subject, "win-a");
        assert.deepEqual(border.outsets, [3, 3, 3, 3]);
        assert.equal(border.width, 3);
        assert.equal(border.radius, 6);
        assert.equal(border.witness?.win, "win-a");
        assert.equal(border.witness?.foreground, "win-a");
        assert.equal(typeof parsed.presentation.rev, "string");
        const win = parsed.windows[0];
        assert.ok(win);
        assert.deepEqual(win.visibility, { shown: true, rev: "1", fallback: false });
        assert.equal(win.visibility_auth, "controlled");
        // Bootstrap discovery stays unbound with ready false.
        const boot = parseDiscoveryReply(raw("reply-bootstrap.json"));
        assert.ok(isCoreWorldReply(boot));
        assert.equal(boot.ready, false);
    });

    it("state authority: four targets including above", () => {
        const parsed = parseWorldReply(boundText(doc("reply-state-authority.json")));
        assert.ok(isCoreWorldReply(parsed));
        const state = parsed.windows[0]?.state ?? [];
        assert.deepEqual(state.map((s) => s.property), ["fullscreen", "maximized", "sticky", "above"]);
        assert.equal(state[3]?.value, true);
        assert.equal(state[0]?.cmdSeq, "1");
        assert.equal(state[1]?.auth, "hold");
        assert.equal(state[1]?.reason, "refused-target");
        assert.deepEqual(parsed.windows[0]?.visibility, { shown: true, rev: "1", fallback: false });
    });

    it("switch and diagnostic lanes", () => {
        const sw = parseWorldReply(boundText(doc("reply-switch-proven.json")));
        assert.ok(isCoreWorldReply(sw));
        assert.deepEqual(sw.switchSettled, [
            { op: "view-a", outcome: "complete", failed: [], focus: "proven", fallback: false },
        ]);
        const dg = parseWorldReply(boundText(doc("reply-row-diagnostics.json")));
        assert.ok(isCoreWorldReply(dg));
        assert.equal(dg.diagnostics.length, 5);
        assert.deepEqual(dg.diagnostics.map((d) => d.row), ["screen", "window", "native", "attempt", "presentation"]);
        assert.deepEqual(dg.diagnostics.map((d) => d.kind),
            ["unreadable", "invalid-fact", "conflicting-fact", "unreadable", "invalid-fact"]);
        assert.equal(dg.diagnostics[4]?.id, null);
    });

    it("effect cause coupling and transfer command correlation", () => {
        const fx = parseWorldReply(boundText(doc("reply-effect-focus.json")));
        assert.ok(isCoreWorldReply(fx));
        assert.deepEqual(fx.effects, [
            {
                id: "effect-a", kind: "focus", target: "win-a", rev: "1",
                cause: "explicit-command", cmdSeq: "1", awaitShown: true,
            },
        ]);
        const tr = parseWorldReply(boundText(doc("reply-transfer-window.json")));
        assert.ok(isCoreWorldReply(tr));
        assert.deepEqual(tr.transfers, [
            {
                op: "transfer-a", kind: "window", win: "win-a",
                out: "out-a", ws: "ws-a", follow: true, held: false, cmdSeq: "1",
            },
        ]);
    });

    it("result vocabulary decodes truthfully", () => {
        const drag = parseWorldReply(boundText(doc("reply-result-drag-left.json")));
        assert.ok(isCoreWorldReply(drag));
        assert.equal(drag.result?.op, "drag-preview");
        if (drag.result?.op === "drag-preview") {
            assert.equal(drag.result.win, "win-a");
            assert.equal(drag.result.hover?.prior?.edge, "left");
        }
        const float = parseWorldReply(boundText(doc("reply-result-float-placed.json")));
        assert.ok(isCoreWorldReply(float));
        assert.deepEqual(float.result, {
            op: "toggle-float", win: "win-a", rect: [10, 20, 640, 480], noSpace: false,
        });
        const mig = parseWorldReply(boundText(doc("reply-result-migration.json")));
        assert.ok(isCoreWorldReply(mig));
        assert.deepEqual(mig.result, {
            op: "migrate-workspace", direction: "right",
            sourceOut: "out-b", sourceWs: "ws-a", targetOut: "out-a", targetWs: "ws-a",
            members: ["win-a"], floats: 0,
        });
        const max = parseWorldReply(boundText(doc("reply-result-maximize.json")));
        assert.ok(isCoreWorldReply(max));
        assert.deepEqual(max.result, {
            op: "maximize", win: "win-a", maximized: true, clearFullscreen: true, wasMaximized: false,
        });
        const rej = parseWorldReply(boundText(doc("reply-rejected-focus-unknown.json")));
        assert.ok(isCoreWorldReply(rej));
        assert.deepEqual(rej.result, { op: "rejected", kind: "focus-unknown" });
        const dir = parseWorldReply(boundText(doc("reply-directional-focus-float-applied.json")));
        assert.ok(isCoreWorldReply(dir));
        assert.deepEqual(dir.result, {
            op: "directional", outcome: "focus-float-applied",
            win: "win-a", rect: [8, 8, 800, 600], cause: null,
        });
        const settled = parseWorldReply(boundText(doc("reply-gesture-committed.json")));
        assert.ok(isCoreWorldReply(settled));
        assert.deepEqual(settled.gestureSettled, { id: "1", committed: true });
        const hide = parseWorldReply(boundText(doc("reply-visibility-hide-staged.json")));
        assert.ok(isCoreWorldReply(hide));
        assert.equal(hide.windows[0]?.visibility?.shown, false);
    });

    it("refuses single-mutation violations", () => {
        const bind = (body: Record<string, unknown>): string => boundText(body);
        // Wrong contract version.
        assert.equal(parseWorldReply(bind({ ...clone("reply-observe.json"), v: 3 })), null);
        // Missing v4 lanes.
        const noSwitch = clone("reply-observe.json");
        delete noSwitch["switchSettled"];
        assert.equal(parseWorldReply(bind(noSwitch)), null);
        const noDiag = clone("reply-observe.json");
        delete noDiag["diagnostics"];
        assert.equal(parseWorldReply(bind(noDiag)), null);
        // Transfer row without retained command correlation.
        const noCmd = clone("reply-transfer-window.json");
        for (const t of noCmd["transfers"] as Array<Record<string, unknown>>) {
            delete t["cmdSeq"];
        }
        assert.equal(parseWorldReply(bind(noCmd)), null);
        // Retired 13-key window shape.
        const oldWin = clone("reply-observe.json");
        const keep = ["win", "out", "ws", "alloc", "frame", "frame_auth", "frame_hold",
            "member_auth", "member_hold", "frame_op", "clear_op", "floating", "float_origin"];
        oldWin["windows"] = (oldWin["windows"] as Array<Record<string, unknown>>).map((w) =>
            Object.fromEntries(keep.map((k) => [k, w[k]])));
        assert.equal(parseWorldReply(bind(oldWin)), null);
        // Retired absolute border shape.
        const oldBorder = clone("reply-observe.json");
        (oldBorder["presentation"] as Record<string, unknown>)["border"] =
            { visible: true, reason: "", inner: null, outer: null };
        // Presentation rev is required: drop it too for the pre-migration shape.
        delete (oldBorder["presentation"] as Record<string, unknown>)["rev"];
        assert.equal(parseWorldReply(bind(oldBorder)), null);
        // Unknown effect cause.
        const badCause = clone("reply-effect-focus.json");
        const fxRows = badCause["effects"] as Array<Record<string, unknown>>;
        const fxRow = fxRows[0];
        assert.ok(fxRow);
        fxRow["cause"] = "fx-spell";
        assert.equal(parseWorldReply(bind(badCause)), null);
        // Show never carries hide-fallback permission.
        const badVis = clone("reply-observe.json");
        const visWins = badVis["windows"] as Array<Record<string, unknown>>;
        const visWin = visWins[0];
        assert.ok(visWin);
        visWin["visibility"] = { shown: true, rev: "1", fallback: true };
        assert.equal(parseWorldReply(bind(badVis)), null);
        // Present-but-false trailing marker refuses.
        const badTrail = clone("reply-observe.json");
        const trailWs = badTrail["workspaces"] as Array<Record<string, unknown>>;
        const trailRow = trailWs[0];
        assert.ok(trailRow);
        trailRow["trailing"] = false;
        assert.equal(parseWorldReply(bind(badTrail)), null);
        // Duplicate window identity refuses.
        const dupWin = clone("reply-observe.json");
        (dupWin["windows"] as Array<unknown>).push((dupWin["windows"] as Array<unknown>)[0]);
        assert.equal(parseWorldReply(bind(dupWin)), null);
        // 17 outputs exceed the 16-output section cap.
        const manyOut = clone("reply-observe.json");
        manyOut["outputs"] = Array.from({ length: 17 }, (_, i) => ({
            out: `out-${i}`, view: "", mode: "tiled", hold: "",
        }));
        assert.equal(parseWorldReply(bind(manyOut)), null);
        // Non-canonical counter refuses.
        assert.equal(parseWorldReply(bind({ ...clone("reply-observe.json"), obs: "01" })), null);
        // Retired result ops refuse.
        const badOp = clone("reply-result-drag-left.json");
        (badOp["result"] as Record<string, unknown>)["op"] = "active-group";
        assert.equal(parseWorldReply(bind(badOp)), null);
        // Present-but-false no-space marker refuses.
        const badSpace = clone("reply-result-float-placed.json");
        (badSpace["result"] as Record<string, unknown>)["noSpace"] = false;
        assert.equal(parseWorldReply(bind(badSpace)), null);
        // Oversize wire (trailing-space padding past 4MiB) refuses.
        const oversizeBound = boundText(doc("reply-observe.json")) + " ".repeat(4 * 1024 * 1024);
        assert.equal(parseWorldReply(oversizeBound), null);
        assert.equal(parseDiscoveryReply(raw("reply-bootstrap.json") + " ".repeat(4 * 1024 * 1024)), null);
    });

    it("broker correlation: strip_result and gestureAssoc", () => {
        // strip_result semantics: the same non-null result, nulled, still binds.
        const stripped = clone("reply-result-drag-left.json");
        stripped["result"] = null;
        const parsed = parseWorldReply(boundText(stripped));
        assert.ok(isCoreWorldReply(parsed));
        assert.equal(parsed.result, null);
        // Broker-owned actuator association round-trips verbatim.
        const assoc = { instance: "inst-a", serial: "1", coreId: "1" };
        const withAssoc = JSON.stringify({ ...clone("reply-observe.json"), source: "src-a", nativeInvalid: "1", clock: "2", gestureAssoc: assoc });
        const assocParsed = parseWorldReply(withAssoc);
        assert.ok(isCoreWorldReply(assocParsed));
        assert.deepEqual(assocParsed.gestureAssoc, assoc);
    });
});
