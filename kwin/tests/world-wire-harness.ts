// Strict v4 broker-transport shim for real-Engine integration tests.
//
// The production broker owns one sequencing namespace plus the bound
// response correlation; `raw_world_eval` is a retained strict `WorldEngine`
// with no broker in front of it, so this file performs exactly the
// broker-owned transport stamping the tests need and nothing else:
//
//   DiscoverWorld carries the v4 connection family (`handshake` then
//   revisioned `settings`, dispatched by `type` through
//   `route_connection_request`): the handshake reply echoes the producer
//   adapter lifetime verbatim (never a broker incarnation) and mints the
//   serving generation; settings never reset the lifetime.
//   DescribeWorld carries the v4 capture envelope
//   (`{v,adapter,gen,source,sourceSeq,nativeInvalid,report}`): captures
//   stamp the broker-owned sequencing namespace (`adapter` is the envelope's
//   own producer lifetime, `gen` the serving generation, `obs`/`reqSeq` fresh
//   per-capture sequences, `req` the capture source, `cmdSeq`/`command` the
//   at-most-one drained head, `monotonicMs` the advancing dispatch clock)
//   around the raw inner report with at most one queued command drained, and
//   capture replies bind the source correlation, the native watermark, and
//   the dispatch clock with the terminal result nulled (requester-only via
//   take). Queue/take answers are per-command correlated (monotonic global
//   seq, one terminal settlement per filed command): pending only while
//   nothing drained, ready (possibly null) once settled, never a stale last
//   result reused. `gestureAssoc` stays null here (the seam carries no
//   broker gesture map; never a fabricated binding).
//
// The inner report arrives as genuine v4 producer bytes (strict codec owned
// shapes including `rendererInput`, `views` refused, no v3 conversion, no
// classifier verdicts). The broker-owned capture-only lanes
// (`nativeGesture`, `cursorWindow`, `nativeInvalidWitness`) are stripped
// before the strict core decode exactly like the production broker
// (`world_broker.rs`); absent keys pass through. Every other inner key
// rides verbatim: this file carries no classifier copy and never
// normalizes or reconstructs producer facts. Carrying any broker-owned
// sequencing key inside the inner report throws (production refuses it as
// `unknown-field`).
//
// Broker auth/replay/duplicate/eviction semantics live in the Rust broker
// tests; this file duplicates none of that. No D-Bus, no KWin.

// Retired: production discovery binds the producer adapter lifetime, never
// a broker incarnation, and stamps no `owner` key. Kept only so existing
// imports keep compiling; new code must not use them.
export const BROKER_INCARNATION = "broker-1";
export const BROKER_OWNER = "omnitiler-broker";

// Honest v4 disabled presentation (mirrors the strict `parsePresentation`
// shape): hidden border/underlay, empty pager, rev 0, no suspended output.
export const DISABLED_PRESENTATION = {
    foreground: "",
    rev: "0",
    border: { visible: false, reason: "disabled", subject: null, outsets: null, width: null, radius: null, witness: null },
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


// The inner report carries genuine v4 producer facts only: the strict
// broker-owned sequencing namespace is stamped here, never normalized
// there. This file carries no classifier copy.

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
    // Pass one engine handshake reply through verbatim (the producer
    // adapter lifetime echoes back untouched). Records the serving gen.
    bindDiscovery(engineBootstrapReply: string): string;
    // Stamp one capture envelope into the exact strict v4 report the sole
    // broker would dispatch: the envelope's own producer adapter and
    // serving generation, fresh broker sequences, the capture source, and
    // at most one queued command, with the capture-only lanes stripped.
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
    // `incarnation` is retained for call compatibility only: production
    // discovery binds the producer adapter lifetime, so nothing here
    // rewrites to it.
    let gen = "";
    let obs = "0";
    let reqSeq = "0";
    let cmdSeq = "0";
    let globalSeq = "0";
    let clock = "0";
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
            // The handshake reply echoes the producer adapter lifetime the
            // entry bound at discovery: pass it through verbatim and record
            // the serving generation it mints. Never rewritten.
            const parsed = JSON.parse(engineBootstrapReply) as Record<string, unknown>;
            gen = parsed["gen"] as string;
            return JSON.stringify(parsed);
        },
        stampCapture(envelopePayload: string): string {
            const envelope = JSON.parse(envelopePayload) as Record<string, unknown>;
            if (envelope["v"] !== 4) {
                throw new Error(`expected a v4 capture envelope, got ${JSON.stringify(envelope["v"])}`);
            }
            // Genuine producer bytes ride verbatim (never normalized or
            // reconstructed); only the broker-owned capture-only lanes are
            // stripped before the strict core decode, exactly like the
            // production broker. Absent keys pass through.
            const raw = envelope["report"] as Record<string, unknown>;
            for (const brokerKey of ["adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]) {
                if (brokerKey in raw) {
                    throw new Error(`inner report must not carry broker-owned ${brokerKey}`);
                }
            }
            const inner = { ...raw };
            delete inner["nativeGesture"];
            delete inner["cursorWindow"];
            delete inner["nativeInvalidWitness"];
            obs = increment(obs);
            reqSeq = increment(reqSeq);
            clock = increment(clock);
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
                adapter: envelope["adapter"],
                gen: envelope["gen"],
                obs,
                reqSeq,
                req: envelope["source"],
                cmdSeq: cmdSeqValue,
                command: command ?? null,
                monotonicMs: clock,
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
            clock = increment(clock);
            return JSON.stringify({
                ...core,
                result: null,
                gestureAssoc: null,
                source: envelope["source"],
                nativeInvalid: envelope["nativeInvalid"],
                clock,
            });
        },
        answerQueue(queuePayload: string): string {
            const body = JSON.parse(queuePayload) as Record<string, unknown>;
            if (body["gen"] !== gen) {
                // Stale generation files nothing: the serving generation
                // refuses before any high-water or queue access.
                clock = increment(clock);
                return JSON.stringify({
                    v: 4, outcome: "rejected", kind: "generation-mismatch",
                    message: "generation mismatch", gen, source: "",
                    nativeInvalid: "0", clock, gestureAssoc: null,
                });
            }
            const key = queueKey(body["client"], body["clientCmd"]);
            const known = filedGlobal.get(key);
            if (known !== undefined) {
                // Duplicate file under the same identity: never re-executes.
                return JSON.stringify({
                    v: 4, outcome: "duplicate", gen,
                    client: body["client"], clientCmd: body["clientCmd"], globalCmd: known,
                });
            }
            globalSeq = increment(globalSeq);
            filedGlobal.set(key, globalSeq);
            queued.push({ client: body["client"], clientCmd: body["clientCmd"], command: body["command"], globalCmd: globalSeq });
            return JSON.stringify({
                v: 4, outcome: "queued", gen,
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
                clock = increment(clock);
                return JSON.stringify({
                    v: 4, outcome: "rejected", kind: "generation-mismatch",
                    message: "generation mismatch", gen, source: "",
                    nativeInvalid: "0", clock, gestureAssoc: null,
                });
            }
            const open = settlements.find((s) => !s.collected && s.settled);
            if (open === undefined) {
                return JSON.stringify({ v: 4, outcome: "pending", gen });
            }
            open.collected = true;
            return JSON.stringify({ v: 4, outcome: "ready", gen, globalCmd: open.globalCmd, result: open.result });
        },
    };
}

export function assertCanonicalU64(text: unknown): void {
    if (!isCanonicalU64(text)) {
        throw new Error(`expected canonical u64, got ${JSON.stringify(text)}`);
    }
}
