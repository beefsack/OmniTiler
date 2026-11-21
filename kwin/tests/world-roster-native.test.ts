import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";
import { applyWorldRoster, createWorldRosterSpendState, type WorldRosterHost } from "../src/world-roster";
import {
    encodeV4RosterFeedback, isCoreWorldReply, parseWorldReply,
    type WorldRosterFeedback, type WorldRosterHome, type WorldRosterPlan,
} from "../src/world-report";

function setup(grow = false) {
    const target = grow ? { kind: "append" as const, step: "append-c" } : { kind: "existing" as const, native: "n-b" };
    const order = grow ? ["n-a", "n-b"] : ["n-a", "n-b", "n-c"];
    const plan: WorldRosterPlan = {
        op: "roster-4", gen: "gen-a", rev: "4", obs: "5",
        baseline: { epoch: "9", order: [...order], projections: [{ out: "out-a", rows: ["a", "b"] }, { out: "out-b", rows: ["sibling"] }] },
        currents: [{ out: "out-a", native: "n-a" }, { out: "out-b", native: "n-a" }],
        occupants: [
            { native: "n-a", windows: [{ win: "mover", out: "out-a" }, { win: "sibling", out: "out-b" }] },
            ...(grow ? [] : [{ native: "n-b", windows: [] }, { native: "n-c", windows: [] }]),
        ],
        projections: [{ out: "out-a", rows: grow ? ["a", "b", "c"] : ["a", "b"] }, { out: "out-b", rows: ["sibling"] }],
        slots: [{ kind: "existing", native: "n-a" }, { kind: "existing", native: "n-b" }, ...(grow ? [target] : [])],
        members: [{ step: "move", win: "mover", before: { out: "out-a", native: "n-a" }, out: "out-a", target, deps: grow ? ["append-c"] : [] }],
        views: [{ step: "view", out: "out-a", before: "n-a", target, deps: grow ? ["append-c", "move"] : ["move"] }],
        removals: grow ? [] : [{ step: "remove", native: "n-c", deps: ["move", "view"] }],
        focus: { id: "focus", target: "mover", rev: "4", cause: "explicit-command", out: "out-a", ws: grow ? "c" : "b", cmdSeq: "1", afterShown: null },
    };
    const currents = new Map(plan.currents.map((entry) => [entry.out, entry.native]));
    const homes = new Map<string, WorldRosterHome>([
        ["mover", { out: "out-a", native: "n-a" }], ["sibling", { out: "out-b", native: "n-a" }],
    ]);
    const calls: string[] = [];
    const host: WorldRosterHost = {
        fresh: () => true,
        read: () => {
            const occupants = new Map<string, Map<string, string>>(order.map((native) => [native, new Map()]));
            for (const [win, { out, native }] of homes) occupants.get(native)?.set(win, out);
            return { order, currents, occupants };
        },
        memberHome: (win) => homes.get(win) ?? null,
        // The unchanged sibling must not be routed through write eligibility.
        canAffect: (win) => win === "mover",
        canWrite: () => true,
        canRemove: () => true,
        append: () => { calls.push("append"); order.push("n-c"); return true; },
        move: (win, out, native) => { calls.push(`move:${native}`); homes.set(win, { out, native }); return true; },
        select: (out, native) => { calls.push(`view:${native}`); currents.set(out, native); return true; },
        remove: (native) => { calls.push(`remove:${native}`); order.splice(order.indexOf(native), 1); return true; },
        focus: (request, resolve) => { calls.push(`focus:${resolve(request.out, request.ws)}`); return true; },
    };
    return { plan, host, calls, homes, currents, order, state: createWorldRosterSpendState() };
}

describe("shared native roster apply", () => {
    it("replays exact append proof after a lost ACK without minting again", () => {
        const { plan, host, calls, state } = setup(true);
        const result = applyWorldRoster(plan, host, state);
        assert.equal(result.complete, true);
        assert.deepEqual(calls, ["append", "move:n-c", "view:n-c", "focus:n-c"]);
        assert.deepEqual(result.feedback?.steps[0], {
            step: "append-c", status: "appended",
            before: { kind: "order", order: ["n-a", "n-b"] },
            after: { kind: "order", order: ["n-a", "n-b", "n-c"] },
        });
        // The planner redelivers the same plan without having consumed the feedback.
        const replay = applyWorldRoster(plan, host, state);
        assert.deepEqual(replay.feedback, result.feedback);
        assert.equal(replay.complete, true);
        assert.deepEqual(calls, ["append", "move:n-c", "view:n-c", "focus:n-c"]);
    });

    it("grows a one-desktop startup roster to the KDE floor without changing membership or view", () => {
        const { plan, host, calls, order, state } = setup(true);
        order.pop();
        plan.baseline.order.pop();
        const [before] = plan.baseline.projections;
        const [after] = plan.projections;
        assert.ok(before && after);
        before.rows.pop();
        after.rows.pop();
        plan.slots.splice(1, 1);
        plan.members = [];
        plan.views = [];
        plan.occupants = [];
        plan.focus = null;

        const result = applyWorldRoster(plan, host, state);
        assert.equal(result.complete, true);
        assert.deepEqual(calls, ["append"]);
        assert.deepEqual(result.feedback?.steps, [{
            step: "append-c", status: "appended",
            before: { kind: "order", order: ["n-a"] },
            after: { kind: "order", order: ["n-a", "n-c"] },
        }]);
        assert.deepEqual(applyWorldRoster(plan, host, state), result);
        assert.deepEqual(calls, ["append"]);
    });

    it("runs focus before a proved-safe removal and retains the exact before order", () => {
        const { plan, host, calls, state } = setup();
        const result = applyWorldRoster(plan, host, state);
        assert.equal(result.complete, true);
        assert.deepEqual(calls, ["move:n-b", "view:n-b", "focus:n-b", "remove:n-c"]);
        assert.deepEqual(result.feedback?.steps[2], {
            step: "remove", status: "applied",
            before: { kind: "order", order: ["n-a", "n-b", "n-c"] },
            after: { kind: "order", order: ["n-a", "n-b"] },
        });
    });

    it("holds before any setter on extra occupancy or an unsafe affected window", () => {
        const { plan, host, calls, homes, state } = setup(true);
        homes.set("unmanaged", { out: "out-a", native: "n-a" });
        assert.equal(applyWorldRoster(plan, host, state).stopped, "preflight");
        const affected = plan.occupants[0];
        assert.ok(affected);
        affected.windows.push({ win: "unmanaged", out: "out-a" });
        assert.equal(applyWorldRoster(plan, host, state).stopped, "preflight");
        assert.deepEqual(calls, []);
    });

    it("holds an unreadable append baseline without spending and retries preflight once readable", () => {
        const { plan, host, calls, state } = setup(true);
        const read = host.read;
        host.read = () => null;
        const held = applyWorldRoster(plan, host, state);
        assert.equal(held.stopped, "preflight");
        assert.equal(held.feedback, null);
        assert.deepEqual(calls, []);
        host.read = read;
        assert.equal(applyWorldRoster(plan, host, state).complete, true);
        assert.deepEqual(calls, ["append", "move:n-c", "view:n-c", "focus:n-c"]);
    });

    it("spends nothing on a superseded publication and can apply its fresh redelivery", () => {
        const { plan, host, calls, state } = setup(true);
        host.fresh = () => false;
        const stale = applyWorldRoster(plan, host, state);
        assert.equal(stale.complete, false);
        assert.equal(stale.feedback, null);
        assert.deepEqual(calls, []);
        host.fresh = () => true;
        const freshPlan = { ...plan, obs: "6" };
        const redelivery = applyWorldRoster(freshPlan, host, state);
        assert.equal(redelivery.complete, true);
        assert.deepEqual(applyWorldRoster(freshPlan, host, state), redelivery);
        assert.deepEqual(calls, ["append", "move:n-c", "view:n-c", "focus:n-c"]);
    });

    it("does not claim an external tail when the append setter refuses", () => {
        const { plan, host, calls, order, state } = setup(true);
        host.append = () => {
            calls.push("append-refuses");
            order.push("n-user");
            host.fresh = () => false;
            return false;
        };
        const refused = applyWorldRoster(plan, host, state);
        assert.equal(refused.stopped, "unproved");
        assert.deepEqual(refused.feedback?.steps, [{
            step: "append-c", status: "refused",
            before: { kind: "order", order: ["n-a", "n-b"] },
            after: { kind: "order", order: ["n-a", "n-b", "n-user"] },
        }]);
        assert.deepEqual(applyWorldRoster(plan, host, state), refused);
        assert.deepEqual(calls, ["append-refuses"]);
    });

    it("never claims a user desktop arriving after an unproved append", () => {
        const { plan, host, calls, order, state } = setup(true);
        host.append = () => { calls.push("append-noop"); return true; };
        const failed = applyWorldRoster(plan, host, state);
        assert.equal(failed.stopped, "unproved");
        assert.deepEqual(failed.feedback?.steps, [{
            step: "append-c", status: "refused",
            before: { kind: "order", order: ["n-a", "n-b"] },
            after: { kind: "order", order: ["n-a", "n-b"] },
        }]);
        order.push("n-user");
        assert.deepEqual(applyWorldRoster(plan, host, state), failed);
        assert.deepEqual(order, ["n-a", "n-b", "n-user"]);
        assert.deepEqual(calls, ["append-noop"]);
    });

    it("does not re-call append or promote late readback to causal proof", () => {
        const { plan, host, calls, order, state } = setup(true);
        const append = host.append;
        const read = host.read;
        host.append = () => {
            const accepted = append();
            host.read = () => null;
            return accepted;
        };
        const failed = applyWorldRoster(plan, host, state);
        assert.equal(failed.stopped, "unproved");
        assert.deepEqual(failed.feedback?.steps, [{
            step: "append-c", status: "refused",
            before: { kind: "order", order: ["n-a", "n-b"] },
            after: null,
        }]);
        assert.deepEqual(order, ["n-a", "n-b", "n-c"]);
        host.read = read;
        assert.deepEqual(applyWorldRoster(plan, host, state), failed);
        assert.deepEqual(calls, ["append"]);
    });

    it("keeps an accepted asynchronous member spent and resumes only on exact arrival", () => {
        const { plan, host, calls, homes, state } = setup();
        host.move = () => { calls.push("move-pending"); return true; };
        const pending = applyWorldRoster(plan, host, state);
        assert.equal(pending.stopped, "pending");
        assert.equal(pending.feedback, null);
        assert.deepEqual(pending.pendingMember, { step: "move", win: "mover", out: "out-a", native: "n-b" });
        assert.deepEqual(applyWorldRoster(plan, host, state), pending);
        homes.set("mover", { out: "out-a", native: "n-b" });
        const arrived = applyWorldRoster(plan, host, state);
        assert.equal(arrived.complete, true);
        assert.deepEqual(arrived.feedback?.steps[0], {
            step: "move", status: "applied", before: { kind: "home", out: "out-a", native: "n-a" },
            after: { kind: "home", out: "out-a", native: "n-b" },
        });
        assert.deepEqual(calls, ["move-pending", "view:n-b", "focus:n-b", "remove:n-c"]);
    });

    it("allows the destination-current crossing echo only as an intermediate", () => {
        const { plan, host, calls, homes, currents, state } = setup();
        const [member] = plan.members;
        const [view] = plan.views;
        const [, current] = plan.currents;
        const [, projection] = plan.projections;
        assert.ok(member && view && current && projection && plan.focus);
        member.out = "out-b";
        view.out = "out-b";
        view.before = "n-c";
        current.native = "n-c";
        projection.rows.push("target");
        plan.focus.out = "out-b";
        plan.focus.ws = "target";
        currents.set("out-b", "n-c");
        host.move = () => {
            calls.push("send");
            homes.set("mover", { out: "out-b", native: "n-c" });
            return true;
        };
        const crossing = applyWorldRoster(plan, host, state);
        assert.equal(crossing.stopped, "pending");
        assert.equal(crossing.feedback, null);
        assert.deepEqual(calls, ["send"]);
        homes.set("mover", { out: "out-b", native: "n-b" });
        assert.equal(applyWorldRoster(plan, host, state).complete, true);
        assert.deepEqual(calls, ["send", "view:n-b", "focus:n-b", "remove:n-c"]);
    });

    it("never retries a member whose accepted dispatch produced contradictory evidence", () => {
        const { plan, host, calls, homes, state } = setup();
        host.move = () => { calls.push("move-wrong"); homes.set("mover", { out: "out-a", native: "n-c" }); return true; };
        const failed = applyWorldRoster(plan, host, state);
        assert.equal(failed.stopped, "unproved");
        assert.deepEqual(failed.feedback?.steps[0], {
            step: "move", status: "refused", before: { kind: "home", out: "out-a", native: "n-a" },
            after: { kind: "home", out: "out-a", native: "n-c" },
        });
        assert.deepEqual(applyWorldRoster(plan, host, state), failed);
        assert.deepEqual(calls, ["move-wrong"]);
    });

    for (const mutates of [false, true]) {
        it(`spends a throwing member setter without repeats (${mutates ? "changed" : "unchanged"} readback)`, () => {
            const { plan, host, calls, homes, state } = setup();
            host.move = (win, out, native) => {
                calls.push(`move-throws:${native}`);
                if (mutates) homes.set(win, { out, native });
                throw new Error("native membership setter threw");
            };
            const first = applyWorldRoster(plan, host, state);
            const proof = first.feedback?.steps[0];
            assert.equal(proof?.step, "move");
            assert.deepEqual(proof?.before, { kind: "home", out: "out-a", native: "n-a" });
            assert.deepEqual(proof?.after, { kind: "home", out: "out-a", native: mutates ? "n-b" : "n-a" });
            if (mutates) {
                assert.equal(first.complete, true);
                assert.equal(proof?.status, "applied");
                assert.deepEqual(calls, ["move-throws:n-b", "view:n-b", "focus:n-b", "remove:n-c"]);
            } else {
                assert.equal(first.stopped, "unproved");
                assert.equal(proof?.status, "refused");
                assert.deepEqual(calls, ["move-throws:n-b"]);
            }
            assert.deepEqual(applyWorldRoster(plan, host, state), first);
            assert.equal(calls.filter((call) => call === "move-throws:n-b").length, 1);
        });
    }

    it("stops dependent view, focus and removal when external input supersedes a pending move", () => {
        const { plan, host, calls, homes, state } = setup();
        host.move = () => { calls.push("move-pending"); return true; };
        assert.equal(applyWorldRoster(plan, host, state).stopped, "pending");
        homes.set("mover", { out: "out-a", native: "n-c" });
        host.fresh = () => false;
        const superseded = applyWorldRoster(plan, host, state);
        assert.equal(superseded.stopped, "stale");
        assert.equal(superseded.feedback, null);
        assert.deepEqual(homes.get("mover"), { out: "out-a", native: "n-c" });
        assert.deepEqual(calls, ["move-pending"]);
    });

    it("holds focus and removal while an accepted selector still shows its preimage", () => {
        const { plan, host, calls, currents, state } = setup();
        host.select = () => { calls.push("view-pending"); return true; };
        const first = applyWorldRoster(plan, host, state);
        assert.equal(first.stopped, "pending");
        assert.deepEqual(first.feedback?.steps.map((row) => row.step), ["move"]);
        assert.deepEqual(applyWorldRoster(plan, host, state), first);
        assert.deepEqual(calls, ["move:n-b", "view-pending"]);
        currents.set("out-a", "n-b");
        const second = applyWorldRoster(plan, host, state);
        assert.equal(second.complete, true);
        assert.deepEqual(second.feedback?.steps[1], {
            step: "view", status: "applied", before: { kind: "current", native: "n-a" },
            after: { kind: "current", native: "n-b" },
        });
        assert.deepEqual(calls, ["move:n-b", "view-pending", "focus:n-b", "remove:n-c"]);
    });

    it("notifies the independent frame lane before dependent writes and does not redeliver the move", () => {
        const { plan, host, calls, state } = setup();
        let fresh = false;
        host.memberArrived = (win, out, native) => {
            calls.push(`arrived:${win}:${out}:${native}`);
            return fresh;
        };
        const first = applyWorldRoster(plan, host, state);
        assert.equal(first.stopped, "stale");
        assert.equal(first.feedback?.steps[0]?.status, "applied");
        assert.deepEqual(calls, ["move:n-b", "arrived:mover:out-a:n-b"]);
        fresh = true;
        assert.equal(applyWorldRoster(plan, host, state).complete, true);
        assert.deepEqual(calls, ["move:n-b", "arrived:mover:out-a:n-b", "arrived:mover:out-a:n-b",
            "view:n-b", "focus:n-b", "remove:n-c"]);
    });

    it("retains exact append proof across a synchronous reentrant signal without a second setter", () => {
        const { plan, host, calls, state } = setup(true);
        const append = host.append;
        host.append = () => {
            const accepted = append();
            assert.equal(applyWorldRoster(plan, host, state).stopped, "reentrant");
            return accepted;
        };
        const result = applyWorldRoster(plan, host, state);
        assert.equal(result.stopped, "reentrant");
        assert.deepEqual(result.feedback?.steps, [{
            step: "append-c", status: "appended",
            before: { kind: "order", order: ["n-a", "n-b"] },
            after: { kind: "order", order: ["n-a", "n-b", "n-c"] },
        }]);
        assert.deepEqual(applyWorldRoster(plan, host, state), result);
        assert.deepEqual(calls, ["append"]);
    });
});

describe("shared native roster wire", () => {
    it("accepts only the exact optional bound-plan shape", () => {
        const body = JSON.parse(readFileSync("../test-fixtures/world-wire/v4-active/messages/reply-observe.json", "utf8"));
        const reply = { ...body, source: "src-a", nativeInvalid: "1", clock: "2", gestureAssoc: null, gestureSettledAssoc: null };
        const { plan } = setup(true);
        plan.gen = body.gen;
        plan.rev = body.rev;
        plan.obs = body.obs;
        const absent = parseWorldReply(JSON.stringify(reply));
        assert.ok(isCoreWorldReply(absent));
        assert.ok(!Object.prototype.hasOwnProperty.call(absent, "rosterPlan"));
        const parsed = parseWorldReply(JSON.stringify({ ...reply, rosterPlan: plan }));
        assert.ok(isCoreWorldReply(parsed));
        assert.deepEqual(parsed.rosterPlan, plan);
        for (const rosterPlan of [null, { ...plan, rev: 4 }, { ...plan, unexpected: 1 },
            { ...plan, slots: [{ kind: "create", step: "append-c" }] },
            { ...plan, members: [{ ...plan.members[0], target: { kind: "append", step: "append-c", extra: true } }] }]) {
            assert.equal(parseWorldReply(JSON.stringify({ ...reply, rosterPlan })), null);
        }
    });

    it("writes exact raw feedback and rejects wrong proofs", () => {
        const feedback: WorldRosterFeedback = { op: "roster", gen: "gen-a", rev: "4", steps: [
            { step: "app", status: "appended", before: { kind: "order", order: ["a", "b"] }, after: { kind: "order", order: ["a", "b", "c"] } },
            { step: "mv", status: "refused", before: { kind: "home", out: "x", native: "b" }, after: null },
        ] };
        const [appended, refused] = feedback.steps;
        assert.ok(appended && refused);
        assert.equal(encodeV4RosterFeedback(feedback), '{"op":"roster","gen":"gen-a","rev":"4","steps":[{"step":"app","status":"appended","before":{"kind":"order","order":["a","b"]},"after":{"kind":"order","order":["a","b","c"]}},{"step":"mv","status":"refused","before":{"kind":"home","out":"x","native":"b"},"after":null}]}');
        assert.equal(encodeV4RosterFeedback({ ...feedback, steps: [
            { ...appended, after: { kind: "order", order: ["a", "c", "b"] } },
        ] }), null);
        assert.equal(encodeV4RosterFeedback({ ...feedback, steps: [{ ...refused, status: "applied" }] }), null);
    });
});
