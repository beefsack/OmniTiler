// Strict v4 broker-transport shim for real-Engine integration tests.
//
// SINGLE-PARKED-COMMAND LIMITATION: this shim stamps at most one queued
// command per capture and is NOT a model of production batch semantics.
// Production (`WorldService` via `broker_world_eval`) drains ALL parked
// commands FIFO in one capture. Any test exercising queued bursts or
// multiple parked commands must drive the genuine retained broker
// evaluator (see world-entry-fifo-engine.test.ts), never this shim.
// `stampCapture` throws fail-fast when more than one command is parked,
// so a burst silently modeled under old one-command-per-capture batching
// becomes a hard RED instead of a false GREEN.
//
// The production broker owns one sequencing namespace plus the bound
// response correlation; `world_eval` is a retained strict `WorldEngine`
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
//   the dispatch clock; terminal command outcomes come from the real
//   issuing `handle_report` `Vec<BatchSettlement>` via the test-only
//   `DescribeWorldWithSettlements` evaluator method and settle per-command
//   for requester-only `TakeCommandResult` (never fabricated or inferred).
//   Queue/take answers are per-command correlated (monotonic global
//   seq, one terminal settlement per filed command): pending only while
//   nothing drained or unsettled, ready (possibly null) or failed once
//   settled, never a stale last result reused. `gestureAssoc` and
//   `gestureSettledAssoc` stay null here (the seam strips `nativeGesture`
//   and carries no broker gesture map; never a fabricated binding).
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
// Broker auth/replay/eviction semantics live in the Rust broker tests; this
// file is not a full production auth/replay/eviction model; minimal
// queue/take correlation only. No D-Bus, no KWin.

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

export interface RawEvalSettlement {
    seq: string;
    result: unknown;
}

export interface RawEvalReplyWithSettlements {
    reply: string;
    settlements: RawEvalSettlement[];
}

function validateSettlementRow(row: unknown): RawEvalSettlement {
    if (row === null || typeof row !== "object" || Array.isArray(row)) {
        throw new Error(`expected settlement object, got ${JSON.stringify(row)}`);
    }
    const r = row as Record<string, unknown>;
    if (!isCanonicalU64(r["seq"])) {
        throw new Error(`expected canonical u64 settlement seq, got ${JSON.stringify(r["seq"])}`);
    }
    if (!("result" in r) || r["result"] === undefined) {
        throw new Error(`settlement seq=${r["seq"] as string} missing required result`);
    }
    const res = r["result"];
    if (res !== null) {
        if (typeof res !== "object" || Array.isArray(res)) {
            throw new Error(`settlement seq=${r["seq"] as string} expected null or object result`);
        }
        const obj = res as Record<string, unknown>;
        if (obj["op"] === "rejected") {
            const kind = obj["kind"];
            if (typeof kind !== "string" || kind.length === 0) {
                throw new Error(`settlement seq=${r["seq"] as string} rejected result missing valid kind`);
            }
        }
    }
    return { seq: r["seq"] as string, result: res };
}

export function parseRawEvalWithSettlements(line: string): RawEvalReplyWithSettlements {
    const parsed = JSON.parse(line) as unknown;
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error(`expected {reply, settlements} object from DescribeWorldWithSettlements, got ${line.slice(0, 200)}`);
    }
    const obj = parsed as Record<string, unknown>;
    if (typeof obj["reply"] !== "string" || !Array.isArray(obj["settlements"])) {
        throw new Error(`expected {reply, settlements} from DescribeWorldWithSettlements, got ${line.slice(0, 200)}`);
    }
    return {
        reply: obj["reply"],
        settlements: (obj["settlements"] as unknown[]).map(validateSettlementRow),
    };
}

interface TerminalSettlement {
    client: unknown;
    clientCmd: unknown;
    globalCmd: string;
    cmdSeq: string;
    result: unknown;
    failedKind: string | null;
    /** False until the draining capture's issuing BatchSettlement binds. */
    settled: boolean;
    /** True once a take collected this settlement (never re-served twice). */
    collected: boolean;
}

export interface MiniBroker {
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
    // the single parked command (throws when more than one command is
    // parked: batch semantics require the genuine broker evaluator),
    // with the capture-only lanes stripped.
    stampCapture(envelopePayload: string): string;
    // Bind one core reply for the actuator lane: source/watermark/clock
    // (no fabricated result lane). Correlates real issuing BatchSettlements
    // by cmdSeq for the requester-only take lane.
    bindReply(coreReplyJson: string, envelopePayload: string, settlements: ReadonlyArray<RawEvalSettlement>): string;
    // File one queued command (FIFO). Replies queued with a fresh
    // per-command global seq; a re-filed (client, clientCmd) pair is a
    // duplicate under the same seq and never re-executes. A stale
    // generation refuses as generation-mismatch before any state access.
    answerQueue(queuePayload: string): string;
    // Collect one terminal result: pending while nothing drained or the
    // latest drain unsettled; ready with the earliest uncollected
    // settlement's own globalCmd and result (null included), collected
    // exactly once. A stale generation refuses as generation-mismatch.
    // Test-only take correlation only: never authentication or full
    // production broker behavior (see the Rust broker tests).
    answerTake(takePayload: string): string;
}

export function createMiniBroker(): MiniBroker {
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
            for (const brokerKey of ["v", "adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]) {
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
            if (queued.length > 1) {
                throw new Error(`mini-broker supports single-parked-command captures only: ${String(queued.length)} commands parked; use the genuine broker_world_eval WorldService route for batch semantics`);
            }
            const next = queued.length > 0 ? (queued.shift() as { client: unknown; clientCmd: unknown; command: unknown; globalCmd: string }) : null;
            const command = next?.command ?? null;
            let cmdSeqValue: string | null = null;
            if (command !== null && command !== undefined) {
                cmdSeq = increment(cmdSeq);
                cmdSeqValue = cmdSeq;
                const entry = { client: next?.client ?? null, clientCmd: next?.clientCmd ?? null, globalCmd: next?.globalCmd ?? "0" };
                drained.push(entry);
                settlements.push({ ...entry, cmdSeq: cmdSeqValue, result: null, failedKind: null, settled: false, collected: false });
            }
            return JSON.stringify({
                v: 4,
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
        bindReply(coreReplyJson: string, envelopePayload: string, batchSettlements: ReadonlyArray<RawEvalSettlement>): string {
            const envelope = JSON.parse(envelopePayload) as Record<string, unknown>;
            const core = JSON.parse(coreReplyJson) as Record<string, unknown>;
            for (const raw of batchSettlements) {
                const s = validateSettlementRow(raw);
                lastResult = s.result;
                const target = settlements.find((entry) => !entry.settled && entry.cmdSeq === s.seq);
                if (target === undefined) {
                    continue;
                }
                const res = s.result;
                if (res !== null && (res as Record<string, unknown>)["op"] === "rejected") {
                    target.failedKind = (res as Record<string, unknown>)["kind"] as string;
                    target.result = null;
                } else {
                    target.result = res;
                    target.failedKind = null;
                }
                target.settled = true;
            }
            clock = increment(clock);
            return JSON.stringify({
                ...core,
                gestureAssoc: null,
                gestureSettledAssoc: null,
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
                    nativeInvalid: "0", clock, gestureAssoc: null, gestureSettledAssoc: null,
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
                    nativeInvalid: "0", clock, gestureAssoc: null, gestureSettledAssoc: null,
                });
            }
            const open = settlements.find((s) => !s.collected && s.settled);
            if (open === undefined) {
                return JSON.stringify({ v: 4, outcome: "pending", gen });
            }
            open.collected = true;
            if (open.failedKind !== null) {
                return JSON.stringify({
                    v: 4, outcome: "failed", gen, globalCmd: open.globalCmd,
                    kind: open.failedKind, message: open.failedKind,
                });
            }
            return JSON.stringify({ v: 4, outcome: "ready", gen, globalCmd: open.globalCmd, result: open.result });
        },
    };
}

export function assertCanonicalU64(text: unknown): void {
    if (!isCanonicalU64(text)) {
        throw new Error(`expected canonical u64, got ${JSON.stringify(text)}`);
    }
}
