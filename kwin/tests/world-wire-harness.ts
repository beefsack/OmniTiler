// Entry-test adapter for real-Engine integration tests (strict v2). NOT an
// exact broker: the production entry speaks the broker wire (envelopes,
// queue/take); `raw_world_eval` speaks the stamped raw capture report
// (real-Rust kde_raw normalization inside the bridge). This helper only
// performs the broker-shaped stamping the tests need: discovery binds the
// broker incarnation plus the serving generation, captures stamp the
// broker-owned sequencing namespace around the raw inner report
// (draining at most one queued command), and capture replies bind the
// source correlation, the native watermark, and the dispatch clock with
// the terminal result nulled (requester-only via take). Queue/take
// answers are per-command correlated (monotonic global seq, one terminal
// settlement per filed command): pending only while nothing drained,
// ready (possibly null) once settled, never a stale last result reused.
// Broker auth/replay/duplicate/eviction semantics live in the Rust broker
// tests; this file duplicates none of that. No D-Bus, no KWin.

export const BROKER_INCARNATION = "broker-1";
export const BROKER_OWNER = "omnitiler-broker";

export const DISABLED_PRESENTATION = {
    foreground: "",
    border: { visible: false, reason: "disabled", inner: null, outer: null },
    underlay: { status: "hidden", reason: "disabled", baseRev: null },
    pager: [],
};

function isCanonicalU64(text: unknown): boolean {
    if (typeof text !== "string" || text.length === 0 || text.length > 20) {
        return false;
    }
    if (text === "0") {
        return true;
    }
    return /^[1-9][0-9]*$/.test(text) && !(text.length === 20 && text > "18446744073709551615");
}

function increment(text: string): string {
    if (text === "0") {
        return "1";
    }
    const digits = text.split("");
    for (let i = digits.length - 1; i >= 0; i -= 1) {
        const d = (digits[i] as string).charCodeAt(0) - 48 + 1;
        if (d <= 9) {
            digits[i] = String(d);
            return digits.join("");
        }
        digits[i] = "0";
    }
    return `1${digits.join("")}`;
}


// Raw captures are normalized by the real Rust broker lane
// (crates/omnitiler/src/kde_raw.rs) inside the test-only
// `raw_world_eval` bridge: this file performs transport stamping only
// and carries no classifier copy.

interface TerminalSettlement {
    client: unknown;
    clientCmd: unknown;
    globalCmd: string;
    result: unknown;
    /** False until the draining capture's core result binds. */
    settled: boolean;
    /** True once a take collected this settlement (never re-served twice). */
    collected: boolean;
}

export interface MiniBroker {
    incarnation: string;
    gen: string;
    lastResult: unknown;
    /** Filed commands drained by captures so far, in drain order. */
    readonly drained: Array<{ client: unknown; clientCmd: unknown; globalCmd: string }>;
    // Rewrite a raw engine bootstrap (echoes the caller adapter) into the
    // broker discovery binding (broker incarnation). Records gen.
    bindDiscovery(engineBootstrapReply: string): string;
    // Stamp one capture envelope into the exact core report the broker
    // would dispatch: broker namespace plus at most one queued command.
    stampCapture(envelopePayload: string): string;
    // Bind one core reply for the actuator lane: source/watermark/clock
    // plus a nulled terminal result. Records the core result for take.
    bindReply(coreReplyJson: string, envelopePayload: string): string;
    // File one queued command (FIFO). Replies queued with a fresh
    // per-command global seq; a re-filed (client, clientCmd) pair is a
    // duplicate under the same seq and never re-executes. A stale
    // generation refuses as generation-mismatch before any state access.
    answerQueue(queuePayload: string): string;
    // Collect one terminal result: pending while nothing drained or the
    // latest drain unsettled; ready with the earliest uncollected
    // settlement's own globalCmd and result (null included), collected
    // exactly once. A stale generation refuses as generation-mismatch.
    answerTake(takePayload: string): string;
}

export function createMiniBroker(incarnation = BROKER_INCARNATION): MiniBroker {
    let gen = "";
    let obs = "0";
    let reqSeq = "0";
    let cmdSeq = "0";
    let globalSeq = "0";
    const queued: Array<{ client: unknown; clientCmd: unknown; command: unknown; globalCmd: string }> = [];
    const drained: Array<{ client: unknown; clientCmd: unknown; globalCmd: string }> = [];
    const settlements: TerminalSettlement[] = [];
    const filedGlobal = new Map<string, string>();
    let lastResult: unknown = null;
    const queueKey = (client: unknown, clientCmd: unknown): string => `${String(client)}\u0000${String(clientCmd)}`;
    return {
        incarnation,
        get gen() {
            return gen;
        },
        get lastResult() {
            return lastResult;
        },
        get drained() {
            return drained;
        },
        bindDiscovery(engineBootstrapReply: string): string {
            const parsed = JSON.parse(engineBootstrapReply) as Record<string, unknown>;
            gen = parsed["gen"] as string;
            return JSON.stringify({ ...parsed, adapter: incarnation });
        },
        stampCapture(envelopePayload: string): string {
            const envelope = JSON.parse(envelopePayload) as Record<string, unknown>;
            // Transport stamping only: the raw KWin-shaped `report` rides
            // through untouched; the `raw_world_eval` bridge normalizes it
            // with the real Rust broker lane before the strict core decode.
            const inner = { ...(envelope["report"] as Record<string, unknown>) };
            obs = increment(obs);
            reqSeq = increment(reqSeq);
            const next = queued.length > 0 ? (queued.shift() as { client: unknown; clientCmd: unknown; command: unknown; globalCmd: string }) : null;
            const command = next?.command ?? null;
            let cmdSeqValue: string | null = null;
            if (command !== null && command !== undefined) {
                cmdSeq = increment(cmdSeq);
                cmdSeqValue = cmdSeq;
                const entry = { client: next?.client ?? null, clientCmd: next?.clientCmd ?? null, globalCmd: next?.globalCmd ?? "0" };
                drained.push(entry);
                settlements.push({ ...entry, result: null, settled: false, collected: false });
            }
            return JSON.stringify({
                ...inner,
                owner: BROKER_OWNER,
                adapter: incarnation,
                gen,
                obs,
                reqSeq,
                req: envelope["source"],
                cmdSeq: cmdSeqValue,
                command: command ?? null,
                monotonicMs: "1",
            });
        },
        bindReply(coreReplyJson: string, envelopePayload: string): string {
            const envelope = JSON.parse(envelopePayload) as Record<string, unknown>;
            const core = JSON.parse(coreReplyJson) as Record<string, unknown>;
            const result = (core["result"] as unknown) ?? null;
            lastResult = result;
            for (let i = settlements.length - 1; i >= 0; i -= 1) {
                const settlement = settlements[i] as TerminalSettlement;
                if (!settlement.settled) {
                    settlement.result = result;
                    settlement.settled = true;
                    break;
                }
            }
            return JSON.stringify({
                ...core,
                result: null,
                gestureAssoc: null,
                source: envelope["source"],
                nativeInvalid: envelope["nativeInvalid"],
                clock: "1",
            });
        },
        answerQueue(queuePayload: string): string {
            const body = JSON.parse(queuePayload) as Record<string, unknown>;
            if (body["gen"] !== gen) {
                // Stale generation files nothing: the serving generation
                // refuses before any high-water or queue access.
                return JSON.stringify({
                    v: 3, outcome: "rejected", kind: "generation-mismatch",
                    message: "generation mismatch", gen, source: "",
                    nativeInvalid: "0", clock: "1",
                });
            }
            const key = queueKey(body["client"], body["clientCmd"]);
            const known = filedGlobal.get(key);
            if (known !== undefined) {
                // Duplicate file under the same identity: never re-executes.
                return JSON.stringify({
                    v: 3, outcome: "duplicate", gen,
                    client: body["client"], clientCmd: body["clientCmd"], globalCmd: known,
                });
            }
            globalSeq = increment(globalSeq);
            filedGlobal.set(key, globalSeq);
            queued.push({ client: body["client"], clientCmd: body["clientCmd"], command: body["command"], globalCmd: globalSeq });
            return JSON.stringify({
                v: 3, outcome: "queued", gen,
                client: body["client"], clientCmd: body["clientCmd"], globalCmd: globalSeq,
            });
        },
        answerTake(takePayload: string): string {
            // The earliest uncollected settled command resolves; before any
            // drain (or while the latest drain is unsettled) the lane stays
            // pending. A settled null result is ready-null, never pending.
            const body = JSON.parse(takePayload) as Record<string, unknown>;
            if (body["gen"] !== gen) {
                // Stale generation collects nothing: the serving generation
                // refuses before any settlement or high-water access.
                return JSON.stringify({
                    v: 3, outcome: "rejected", kind: "generation-mismatch",
                    message: "generation mismatch", gen, source: "",
                    nativeInvalid: "0", clock: "1",
                });
            }
            const open = settlements.find((s) => !s.collected && s.settled);
            if (open === undefined) {
                return JSON.stringify({ v: 3, outcome: "pending", gen });
            }
            open.collected = true;
            return JSON.stringify({ v: 3, outcome: "ready", gen, globalCmd: open.globalCmd, result: open.result });
        },
    };
}

export function assertCanonicalU64(text: unknown): void {
    if (!isCanonicalU64(text)) {
        throw new Error(`expected canonical u64, got ${JSON.stringify(text)}`);
    }
}
