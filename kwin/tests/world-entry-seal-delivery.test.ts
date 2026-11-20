import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { setImmediate } from "node:timers/promises";

import { WORLD_TIMEOUT_MS } from "../src/world-entry";
import {
    awaitDescribeFrom,
    describeCount,
    describeEnvelopeAt,
    discover,
    lastDescribeReply,
    makeHarness,
    takeReadys,
    winById,
    type CapturedCall,
    type Harness,
} from "./world-producer-harness";

type Body = Record<string, unknown>;
const D = "win-d";
const T = "win-t";
const EXTERNAL_WIN = "win-external";
const EXTERNAL_CLIENT = "external-panel";

function parse(raw: string): Body {
    return JSON.parse(raw) as Body;
}

function press(h: Harness): void {
    const shortcut = h.shortcuts.get("Meta+G");
    assert.ok(shortcut, "registered toggle-float shortcut");
    shortcut();
}

function activateT(h: Harness): void {
    h.activeRef = winById(h, T);
    h.fire("windowActivated", h.activeRef);
}

function queued(h: Harness): CapturedCall[] {
    return h.calls.filter((call) => call.method === "QueueWorldCommand");
}

function report(capture: Body): Body {
    return capture["report"] as Body;
}

function assertFloats(reply: Body, expected: Record<string, boolean>): void {
    assert.equal(reply["ready"], true, `incorporated reply: ${JSON.stringify(reply)}`);
    const rows = reply["windows"] as Body[];
    for (const [win, floating] of Object.entries(expected)) {
        assert.equal(rows.find((row) => row["win"] === win)?.["floating"], floating, win);
    }
}

function assertSubjects(h: Harness, expected: string[]): void {
    const results = takeReadys(h).map((reply) => reply["result"] as Body);
    assert.deepEqual(results.map((result) => [result["op"], result["win"]]),
        expected.map((win) => ["toggle-float", win]));
}

function holdQueueAcks(h: Harness): Array<{ raw: string; deliver: () => void }> {
    const acks: Array<{ raw: string; deliver: () => void }> = [];
    h.deliverQueueAck = (raw, callback) => acks.push({ raw, deliver: () => callback(raw) });
    return acks;
}

async function until(h: Harness, predicate: () => boolean, message: string): Promise<void> {
    // Pump only due fake producer timers and bound the actual pipe wait by
    // wall time, not by the speed of the test runner's event loop.
    const deadline = performance.now() + 5_000;
    while (performance.now() < deadline) {
        if (predicate()) {
            return;
        }
        h.runTimers();
        await setImmediate();
    }
    assert.ok(predicate(), `${message}\n${h.logs.slice(-12).join("\n")}`);
}

function wake(h: Harness, gen: string): void {
    // The eval bridge has one fixed test sender and no Wait endpoint. Only
    // the held Wait completion is synthetic; Queue, Describe and Take are real.
    const wait = h.waits[h.waits.length - 1];
    assert.ok(wait, "producer armed its real Wait transport lane");
    const request = parse(wait.payload);
    assert.equal(request["gen"], gen);
    const cursor = (BigInt(request["cursor"] as string) + 1n).toString();
    wait.callback(JSON.stringify({ v: 4, outcome: "wake", gen, cursor }));
    assert.equal(parse(h.waits[h.waits.length - 1]!.payload)["cursor"], cursor,
        "the producer accepted the wake and rearmed");
}

function deliverDescribe(h: Harness, index: number, raw: string): void {
    h.answered.add(index);
    h.replies.set(index, raw);
    h.calls[index]!.callback(raw);
}

async function fileExternal(h: Harness, gen: string, clientCmd: number): Promise<Body> {
    return parse(await h.bridge.send("QueueWorldCommand", JSON.stringify({
        v: 4, gen, client: EXTERNAL_CLIENT, clientCmd: String(clientCmd),
        command: { op: "toggle-float", win: EXTERNAL_WIN, rect: null },
    })));
}

async function take(h: Harness, gen: string, client: unknown, clientCmd: unknown): Promise<Body> {
    return parse(await h.bridge.send("TakeCommandResult", JSON.stringify({ v: 4, gen, client, clientCmd })));
}

describe("sealed shortcut delivery through the real broker", () => {
    it("refuses an oversized cut before Queue without losing the next coherent press", async () => {
        const h = await makeHarness(D, [D, T]);
        try {
            await discover(h);
            const from = h.calls.length;
            const original = h.wins.length;
            const model = winById(h, T);
            // The host roster still fits the native 10,000-row lane. Its
            // factual report, however, cannot fit the 4 MiB wire envelope.
            for (let i = original; i < 10_000; i += 1) {
                h.wins.push({ ...model, internalId: `saturation-${i}-${"x".repeat(96)}` });
            }
            press(h);
            assert.equal(queued(h).length, 0, "an invalid-sized cut must not park a command");
            assert.ok(h.logs.some((row) => row.includes("reason=unproven-shortcut-cut")));
            h.wins.length = original;

            press(h);
            const next = await awaitDescribeFrom(h, from);
            await h.flushAt(next);
            assertSubjects(h, [D]);
            assertFloats(lastDescribeReply(h), { [D]: true, [T]: false });
        } finally {
            await h.stop();
        }
    });

    it("drains the ACKed prefix on D when a real full queue refuses the same-cut tail", async () => {
        const h = await makeHarness(D, [D, T, EXTERNAL_WIN]);
        try {
            const gen = await discover(h);
            const predecessor = lastDescribeReply(h)["rev"] as string;
            for (let cmd = 1; cmd <= 127; cmd += 1) {
                assert.equal((await fileExternal(h, gen, cmd))["outcome"], "queued");
            }
            const acks = holdQueueAcks(h);
            const from = h.calls.length;
            press(h);
            press(h);
            await until(h, () => acks.length === 1, "first local Queue ACK");
            assert.equal(parse(acks[0]!.raw)["outcome"], "queued");
            acks[0]!.deliver();
            await until(h, () => acks.length === 2, "second local Queue refusal");
            assert.equal(parse(acks[1]!.raw)["kind"], "queue-full");
            activateT(h);
            acks[1]!.deliver();

            const drain = await awaitDescribeFrom(h, from);
            const cut = report(describeEnvelopeAt(h, drain));
            assert.equal(cut["focused"], D);
            assert.deepEqual(cut["focusChanges"], { after: predecessor, events: [] });
            await h.flushAt(drain);
            const reply = parse(h.replies.get(drain)!);
            assert.equal(reply["rev"], (BigInt(predecessor) + 1n).toString());
            assertFloats(reply, { [D]: true, [T]: false, [EXTERNAL_WIN]: true });
            assertSubjects(h, [D]);
            const external = await take(h, gen, EXTERNAL_CLIENT, "127");
            assert.equal(external["outcome"], "ready");
            assert.equal((external["result"] as Body)["win"], EXTERNAL_WIN);

            await h.settle();
            assert.equal(queued(h).length, 2, "the unfrozen tail must not be refiled on T");
            const tail = parse(queued(h)[1]!.payload);
            assert.equal((await take(h, gen, tail["client"], tail["clientCmd"]))["kind"], "unknown-command");
            assertFloats(lastDescribeReply(h), { [D]: true, [T]: false });
        } finally {
            await h.stop();
        }
    });

    it("retries a lost Queue ACK byte-for-byte and holds wake, catchup and later filing behind the seal", async () => {
        const x = "win-removed-first";
        const y = "win-removed-second";
        const h = await makeHarness(D, [D, T, x, y], { withWait: true });
        try {
            const gen = await discover(h);
            const baseline = describeCount(h);
            const from = h.calls.length;
            const priorTimers = new Set(h.timers);
            const acks = holdQueueAcks(h);
            const removeNative = (id: string): void => {
                const win = winById(h, id);
                h.fire("windowRemoved", win);
                h.wins.splice(h.wins.indexOf(win), 1);
            };
            removeNative(x);
            press(h);
            await until(h, () => acks.length === 1, "real admission before dropping its ACK");
            const first = parse(acks[0]!.raw);
            assert.equal(first["outcome"], "queued");
            const watchdog = h.timers.find((timer) =>
                !priorTimers.has(timer) && !timer.cancelled && !timer.fired && timer.delayMs === WORLD_TIMEOUT_MS);
            assert.ok(watchdog, "Queue ACK watchdog");

            activateT(h);
            removeNative(y);
            press(h);
            wake(h, gen);
            h.runTimers();
            assert.equal(describeCount(h), baseline, "wake and native catchup cannot drain during Queue ACK loss");
            assert.equal(queued(h).length, 1, "T's segment must remain unfiled");
            watchdog.fired = true;
            watchdog.callback();
            wake(h, gen);
            await until(h, () => acks.length === 2, "real duplicate Queue ACK");
            assert.equal(queued(h)[1]!.payload, queued(h)[0]!.payload);
            const duplicate = parse(acks[1]!.raw);
            assert.equal(duplicate["outcome"], "duplicate");
            assert.equal(duplicate["globalCmd"], first["globalCmd"]);
            assert.equal(describeCount(h), baseline, "even a broker duplicate needs its ACK delivered");
            const beforeDescribe = new Set(h.timers);
            acks[1]!.deliver();

            const drain = await awaitDescribeFrom(h, from);
            assert.equal(describeCount(h), baseline + 1);
            assert.equal(report(describeEnvelopeAt(h, drain))["focused"], D);
            assert.deepEqual(report(describeEnvelopeAt(h, drain))["closed"], [x]);
            const raw = await h.bridge.send("DescribeWorld", h.calls[drain]!.payload);
            assertFloats(parse(raw), { [D]: true, [T]: false });
            wake(h, gen);
            h.runTimers();
            assert.equal(queued(h).length, 2, "T waits for the delivered ready ACK, not the broker drain");
            assert.equal(describeCount(h), baseline + 1, "no competing Describe before the ready ACK");
            const describeWatchdog = h.timers.find((timer) =>
                !beforeDescribe.has(timer) && !timer.cancelled && !timer.fired && timer.delayMs === WORLD_TIMEOUT_MS);
            assert.ok(describeWatchdog, "ready Describe ACK watchdog");
            describeWatchdog.fired = true;
            describeWatchdog.callback();
            wake(h, gen);
            const replay = await awaitDescribeFrom(h, drain + 1);
            assert.equal(h.calls[replay]!.payload, h.calls[drain]!.payload,
                "the broker incorporated D but its lost ACK is recovered with identical bytes");
            assert.equal(queued(h).length, 2, "T waits even while the exact replay is outstanding");
            await h.flushAt(replay);
            const recovered = parse(h.replies.get(replay)!);
            assert.equal(recovered["rev"], parse(raw)["rev"], "exact replay does not publish another revision");
            assertFloats(recovered, { [D]: true, [T]: false });

            await until(h, () => acks.length === 3, "T files after D's ready ACK");
            assert.equal(parse(acks[2]!.raw)["outcome"], "queued");
            assert.notEqual(queued(h)[2]!.payload, queued(h)[0]!.payload);
            acks[2]!.deliver();
            const second = await awaitDescribeFrom(h, replay + 1);
            const tReport = report(describeEnvelopeAt(h, second));
            assert.equal(tReport["focused"], T);
            assert.equal((tReport["focusChanges"] as Body)["after"], recovered["rev"]);
            assert.deepEqual(tReport["closed"], [y], "retire only the close token acknowledged by replay");
            await h.flushAt(second);
            assertFloats(parse(h.replies.get(second)!), { [D]: true, [T]: true });
            assertSubjects(h, [D, T]);
        } finally {
            await h.stop();
        }
    });

    it("retries definitive not-run with a fresh source and identical frozen facts", async () => {
        const h = await makeHarness(D, [D, T], { withWait: true });
        try {
            const gen = await discover(h);
            const predecessor = lastDescribeReply(h);
            const from = h.calls.length;
            press(h);
            const first = await awaitDescribeFrom(h, from);
            const original = describeEnvelopeAt(h, first);
            assert.equal(report(original)["focused"], D);
            activateT(h);

            // Refuse before forwarding; the real broker still has the command
            // parked, and has never seen this source. Do not edit the report.
            const raw = JSON.stringify({
                v: 4, outcome: "rejected", kind: "busy", message: "transient transport refusal",
                gen, source: original["source"], nativeInvalid: original["nativeInvalid"],
                clock: predecessor["clock"],
                gestureAssoc: null, gestureSettledAssoc: null,
            });
            deliverDescribe(h, first, raw);
            const filing = parse(queued(h)[0]!.payload);
            assert.equal((await take(h, gen, filing["client"], filing["clientCmd"]))["outcome"], "pending");
            wake(h, gen);
            const retry = await awaitDescribeFrom(h, first + 1);
            const fresh = describeEnvelopeAt(h, retry);
            assert.notEqual(fresh["source"], original["source"]);
            assert.ok(BigInt(fresh["sourceSeq"] as string) > BigInt(original["sourceSeq"] as string));
            assert.equal(fresh["nativeInvalid"], original["nativeInvalid"]);
            assert.deepEqual(report(fresh), report(original), "only the capture source changes");
            assert.equal(queued(h).length, 1, "retry does not refile the admitted command");
            await h.flushAt(retry);
            const accepted = parse(h.replies.get(retry)!);
            assert.equal(accepted["rev"], (BigInt(predecessor["rev"] as string) + 1n).toString());
            assertFloats(accepted, { [D]: true, [T]: false });
            assertSubjects(h, [D]);
        } finally {
            await h.stop();
        }
    });

    it("resets the generation when a real source collision makes the filed cut irrecoverable", async () => {
        const h = await makeHarness(D, [D, T, EXTERNAL_WIN]);
        try {
            const gen = await discover(h);
            const from = h.calls.length;
            press(h);
            const drain = await awaitDescribeFrom(h, from);
            const cut = describeEnvelopeAt(h, drain);
            assert.equal(report(cut)["focused"], D);
            assert.equal((await fileExternal(h, gen, 1))["outcome"], "queued");
            activateT(h);
            press(h);
            h.runTimers();
            assert.equal(queued(h).length, 1, "later local segment starts unfiled");

            // Claim the source with a strict-codec rejection, never executing
            // native evidence or draining. Then send untouched producer bytes.
            const poisoned = { ...cut, report: { ...report(cut), unexpectedSealedField: true } };
            const reject = parse(await h.bridge.send("DescribeWorld", JSON.stringify(poisoned)));
            assert.equal(reject["outcome"], "rejected");
            assert.equal(reject["kind"], "unknown-field", "the malformed report claims the source first");
            const raw = await h.bridge.send("DescribeWorld", h.calls[drain]!.payload);
            assert.equal(parse(raw)["kind"], "source-reused");
            const local = parse(queued(h)[0]!.payload);
            assert.equal((await take(h, gen, local["client"], local["clientCmd"]))["outcome"], "pending");
            assert.equal((await take(h, gen, EXTERNAL_CLIENT, "1"))["outcome"], "pending");
            deliverDescribe(h, drain, raw);

            const handshake = (): number => h.calls.findIndex((call, index) =>
                index > drain && call.method === "DiscoverWorld" && parse(call.payload)["type"] === "handshake");
            await until(h, () => handshake() >= 0, "irrecoverable delivery triggers a new handshake");
            const replacement = await discover(h, handshake());
            assert.notEqual(replacement, gen);
            assert.equal((await take(h, gen, EXTERNAL_CLIENT, "1"))["kind"], "generation-mismatch");
            assert.equal((await take(h, replacement, EXTERNAL_CLIENT, "1"))["kind"], "unknown-command");
            assert.equal((await take(h, replacement, local["client"], local["clientCmd"]))["kind"], "unknown-command");
            assert.equal(queued(h).length, 1, "the later T press was not reminted across the reset");
            assertSubjects(h, []);
            assertFloats(lastDescribeReply(h), { [D]: false, [T]: false, [EXTERNAL_WIN]: false });
            press(h);
            const postReset = await awaitDescribeFrom(h, handshake());
            assert.equal(describeEnvelopeAt(h, postReset)["gen"], replacement);
            await h.flushAt(postReset);
            assertSubjects(h, [T]);
            assertFloats(lastDescribeReply(h), { [D]: false, [T]: true, [EXTERNAL_WIN]: false });
        } finally {
            await h.stop();
        }
    });
});
