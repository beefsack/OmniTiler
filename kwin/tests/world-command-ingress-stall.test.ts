// Stall regression: retired send source refused + FIFO head drop.
// Fixed builders emit only `home-then-action`; a queue shape refusal
// (`snapshot-invalid`/`command-invalid`) drops the head so later focus,
// select and observe work continue. Uses the real engine via MiniBroker.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";

import { startWorldAdapterEntry } from "../src/world-entry";
import { createMiniBroker } from "./world-wire-harness";
import {
    cmdFocusDirectional,
    cmdMigrateDirectional,
    cmdMoveDirectional,
    cmdResize,
    cmdSelectNumbered,
    cmdSelectPrevious,
    cmdSelectRelative,
    cmdSelectTrailing,
    cmdSendDirectional,
    cmdSendNumbered,
    cmdSendRelative,
    cmdToggleMaximize,
    cmdToggleOrientation,
    cmdToggleWorkspaceTiling,
    implicitSource,
} from "../src/world-report";

function repoRoot(): string {
    let dir = resolve(process.cwd());
    for (let depth = 0; depth < 4; depth += 1) {
        if (existsSync(join(dir, "Cargo.toml"))) {
            return dir;
        }
        dir = dirname(dir);
    }
    throw new Error("fixture root not found");
}

class WorldBridge {
    private readonly proc: ChildProcess;
    private readonly waiters: Array<{ resolve: (r: string) => void; reject: (e: Error) => void }> = [];
    private dead: string | null = null;
    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const stdout = proc.stdout;
        if (stdout === null || stdout === undefined) {
            throw new Error("raw_world_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.on("error", (e) => this.failAll(e instanceof Error ? e : new Error(String(e))));
        proc.on("close", (c, s) => this.failAll(new Error(`raw_world_eval exited ${String(c)}/${String(s)}`)));
    }
    private failAll(e: Error): void {
        if (this.dead === null) {
            this.dead = e.message;
        }
        while (this.waiters.length > 0) {
            this.waiters.shift()?.reject(e);
        }
    }
    static start(): WorldBridge {
        return new WorldBridge(spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "raw_world_eval"], { cwd: repoRoot(), stdio: ["pipe", "pipe", "pipe"] }));
    }
    send(method: string, payload: string): Promise<string> {
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        return new Promise<string>((resolve, reject) => {
            this.waiters.push({ resolve, reject });
            try {
                this.proc.stdin?.write(JSON.stringify({ method, body: payload }) + "\n");
            } catch (e) {
                reject(e instanceof Error ? e : new Error(String(e)));
            }
        });
    }
    async close(): Promise<void> {
        try {
            this.proc.stdin?.end();
        } catch (e) {
            void e;
        }
        await new Promise<void>((resolve) => {
            const t = setTimeout(() => {
                try {
                    this.proc.kill("SIGKILL");
                } catch (e) {
                    void e;
                }
                resolve();
            }, 3000);
            this.proc.on("exit", () => {
                clearTimeout(t);
                resolve();
            });
        });
    }
}

interface Call { method: string; payload: string; callback: (r: unknown) => void }
interface Timer { delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }

describe("kde command ingress stall", () => {
    it("builders decode and a shape-rejected head drops", async () => {
        const ws1 = { id: "ws1" };
        const ws2 = { id: "ws2" };
        const wins = ["win-a", "win-b"].map((id, i) => ({
            internalId: id, normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: i * 300, y: 0, width: 200, height: 200 },
            clientGeometry: { x: i * 300, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [i === 0 ? ws1 : ws2],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null, pid: 1234, resourceClass: "Example", caption: id,
            windowRole: "normal", desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        }));
        const calls: Call[] = [];
        const answered = new Set<number>();
        const timers: Timer[] = [];
        const logs: string[] = [];
        const heldQueue: Array<{ payload: string; callback: (r: unknown) => void }> = [];
        let holdQueue = false;
        const tail = (): string => ` logs-tail:\n${logs.slice(-12).join("\n")}`;
        const surface = {
            windowList: () => wins,
            screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1, ws2],
            activeScreen: { name: "out" },
            cursorPos: { x: 2, y: 2 },
            activeWindow: wins[0],
            currentDesktop: ws1,
            clientArea: () => ({ x: 0, y: 0, width: 1920, height: 400 }),
            currentDesktopForScreen: () => ws1,
            setCurrentDesktopForScreen: () => undefined,
            sendClientToScreen: () => undefined,
            createDesktop: () => null as unknown as object,
            removeDesktop: () => undefined,
        };
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                calls.push({ method, payload, callback });
                if (method === "QueueWorldCommand" && !holdQueue) {
                    callback(broker.answerQueue(payload));
                } else if (method === "QueueWorldCommand" && holdQueue) {
                    heldQueue.push({ payload, callback });
                } else if (method === "TakeCommandResult") {
                    callback(broker.answerTake(payload));
                }
            },
            scheduleOnce: (delay, cb) => {
                const t: Timer = { delayMs: delay, callback: cb, cancelled: false, fired: false };
                timers.push(t);
                return () => {
                    t.cancelled = true;
                };
            },
            log: (m: string) => {
                logs.push(m);
            },
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
        });
        assert.ok(handle !== null);
        const h = handle as NonNullable<typeof handle>;
        const runTimers = (maxDelay = 120): void => {
            for (let i = 0; i < timers.length; i += 1) {
                const t = timers[i] as Timer;
                if (t.delayMs <= maxDelay && !t.cancelled && !t.fired) {
                    t.fired = true;
                    timers.splice(i, 1);
                    i -= 1;
                    t.callback();
                }
            }
        };
        const stampedOps: string[] = [];
        const commandReplies: Array<{ op: string; reply: Record<string, unknown> }> = [];
        const flushAt = async (index: number): Promise<Record<string, unknown>> => {
            assert.ok(calls[index]?.method === "DescribeWorld" && !answered.has(index), `expected unanswered DescribeWorld at ${index}${tail()}`);
            const envelope = calls[index]!.payload;
            const stamped = broker.stampCapture(envelope);
            const stampedCmd = (JSON.parse(stamped) as Record<string, unknown>)["command"] as Record<string, unknown> | null;
            stampedOps.push(stampedCmd === null ? "(commandless)" : String(stampedCmd["op"]));
            const coreText = await bridge.send("DescribeWorld", stamped);
            const core = JSON.parse(coreText) as Record<string, unknown>;
            if (stampedCmd !== null) {
                commandReplies.push({ op: String(stampedCmd["op"]), reply: core });
            }
            const bound = broker.bindReply(coreText, envelope);
            answered.add(index);
            calls[index]!.callback(bound);
            return core;
        };
        const flushAll = async (): Promise<Array<Record<string, unknown>>> => {
            const out: Array<Record<string, unknown>> = [];
            for (let round = 0; round < 16; round += 1) {
                const next = calls.findIndex((c, at) => c.method === "DescribeWorld" && !answered.has(at));
                if (next < 0) {
                    return out;
                }
                out.push(await flushAt(next));
            }
            throw new Error(`flushAll did not converge${tail()}`);
        };
        const settle = async (): Promise<Array<Record<string, unknown>>> => {
            const replies: Array<Record<string, unknown>> = [];
            for (let round = 0; round < 10; round += 1) {
                runTimers();
                const before = calls.length;
                replies.push(...await flushAll());
                runTimers();
                replies.push(...await flushAll());
                if (calls.length === before) {
                    return replies;
                }
            }
            throw new Error(`settle did not converge${tail()}`);
        };
        const commandPayloads = (op: string): Array<Record<string, unknown>> => {
            const out: Array<Record<string, unknown>> = [];
            for (const c of calls) {
                if (c.method !== "QueueWorldCommand") {
                    continue;
                }
                try {
                    const body = JSON.parse(c.payload) as Record<string, unknown>;
                    if ((body["command"] as Record<string, unknown> | null)?.["op"] === op) {
                        out.push(body);
                    }
                } catch {
                    continue;
                }
            }
            return out;
        };
        try {
            const hsIdx = calls.findIndex((c) => {
                if (c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
                } catch (e) {
                    void e;
                    return false;
                }
            });
            assert.ok(hsIdx >= 0, "handshake flies");
            const hsText = await bridge.send("DiscoverWorld", calls[hsIdx]!.payload);
            const hs = JSON.parse(hsText) as Record<string, unknown>;
            assert.equal(hs["type"], "handshake-reply");
            const gen = hs["gen"] as string;
            assert.ok(typeof gen === "string" && gen !== "");
            calls[hsIdx]!.callback(broker.bindDiscovery(hsText));
            const setIdx = calls.findIndex((c, at) => {
                if (at <= hsIdx || c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
                } catch (e) {
                    void e;
                    return false;
                }
            });
            assert.ok(setIdx >= 0, "settings fly");
            const setText = await bridge.send("DiscoverWorld", calls[setIdx]!.payload);
            assert.equal((JSON.parse(setText) as Record<string, unknown>)["status"], "accepted");
            calls[setIdx]!.callback(setText);
            const settled = await settle();
            assert.ok(settled.length > 0, `baseline capture flies${tail()}`);
            let core = settled[settled.length - 1] as Record<string, unknown>;
            assert.notEqual(core["outcome"], "rejected", `baseline accepted, got ${JSON.stringify(core).slice(0, 200)}${tail()}`);
            const revOf = (r: Record<string, unknown>): string => String(r["rev"] ?? "");
            const revBase = revOf(core);
            assert.ok(revBase !== "", "baseline carries rev");
            assert.ok(Array.isArray(core["windows"]) && (core["windows"] as unknown[]).length > 0, "baseline observes windows");

            // Part A: isolated raw-decoder table of every builder with a src
            // caller. Each candidate is queued on the test broker and stamped
            // into a real capture carrying that exact command, so a pending
            // commandless capture cannot false-positive as a decode pass.
            const templateIdx = [...answered].reverse().find((at) => calls[at]?.method === "DescribeWorld");
            assert.ok(templateIdx !== undefined, `capture template exists${tail()}`);
            const template = calls[templateIdx as number]!.payload;
            const candidates: Array<{ op: string; command: Record<string, unknown> }> = [];
            const push = (command: unknown, op: string): void => {
                assert.ok(command !== null, `${op} builds${tail()}`);
                candidates.push({ op, command: command as Record<string, unknown> });
            };
            const home = implicitSource("home-then-action");
            push(cmdFocusDirectional("win-a", "left"), "focus-directional");
            push(cmdMoveDirectional("win-a", "right", "swap-with-neighbor"), "move-directional");
            push(cmdResize("win-a", "left", "inwards", 0), "resize");
            push(cmdToggleOrientation("win-a"), "toggle-orientation");
            candidates.push({ op: "toggle-float", command: { op: "toggle-float", win: "win-a", rect: null } });
            push(cmdToggleMaximize("win-a"), "toggle-maximize");
            push(cmdSelectNumbered(null, 2), "select-numbered");
            push(cmdSelectTrailing(null), "select-trailing");
            push(cmdSendNumbered("win-a", home, 2, true), "send-numbered");
            push(cmdSelectPrevious(null), "select-previous");
            push(cmdSelectRelative(null, 1), "select-relative");
            push(cmdSendRelative("win-a", home, 1, true), "send-relative");
            push(cmdSendDirectional("win-a", "left", true), "send-directional");
            push(cmdMigrateDirectional("left"), "migrate-directional");
            push(cmdToggleWorkspaceTiling("out", "ws1"), "toggle-workspace-tiling");
            let seq = 0;
            for (const { op, command } of candidates) {
                seq += 1;
                broker.answerQueue(JSON.stringify({ v: 4, gen, client: "table", clientCmd: String(seq), command }));
                const stamped = broker.stampCapture(template);
                const stampedCmd = (JSON.parse(stamped) as Record<string, unknown>)["command"] as Record<string, unknown> | null;
                assert.ok(stampedCmd !== null && String(stampedCmd["op"]) === op, `${op} rides the stamped report${tail()}`);
                if (op === "send-numbered" || op === "send-relative") {
                    assert.deepEqual(stampedCmd["source"], { implicit: "home-then-action" }, `${op} callsite source${tail()}`);
                }
                const text = await bridge.send("DescribeWorld", stamped);
                const reply = JSON.parse(text) as Record<string, unknown>;
                broker.bindReply(text, template);
                assert.ok(
                    reply["outcome"] !== "rejected" || (reply["kind"] !== "unknown-value" && reply["kind"] !== "unknown-field" && reply["kind"] !== "malformed"),
                    `${op} decodes, got ${JSON.stringify(reply).slice(0, 200)}${tail()}`,
                );
                if (reply["outcome"] === "rejected") {
                    assert.notEqual(reply["message"], "command-invalid", `${op} no shape refusal${tail()}`);
                }
            }

            // Part B: production entry capture plus the native toggle-float
            // callback on this non-floating fixture (no live-rect read needed
            // on the float path; readLiveRect only guards floating unfloat).
            stampedOps.length = 0;
            h.requestFloat();
            runTimers();
            const floated = await settle();
            assert.ok(commandPayloads("toggle-float").length >= 1, `toggle-float files via native callback${tail()}`);
            assert.ok(stampedOps.includes("toggle-float"), `toggle-float rides its draining capture${tail()}`);
            for (const r of floated) {
                if (r["outcome"] === "rejected") {
                    assert.notEqual((r as Record<string, unknown>)["message"], "command-invalid", `toggle-float no shape refusal${tail()}`);
                }
            }
            assert.ok(!logs.some((l) => l.includes("command-busy")), `no busy while draining${tail()}`);
            core = floated.length > 0 ? (floated[floated.length - 1] as Record<string, unknown>) : core;

            // FIFO poison: hold the next filed head, mutate its command at
            // the transport seam to the retired token, prove those same bytes
            // fail the strict decoder, then relay the genuine failure under
            // the documented production queue mapping
            // (world_broker.rs queue_command: !command_shape_valid ->
            // snapshot-invalid/command-invalid).
            holdQueue = true;
            h.requestWorkspaceMove(1, false);
            runTimers();
            assert.ok(heldQueue.length === 1, `poison head files, got ${heldQueue.length}${tail()}`);
            const poisonBody = JSON.parse(heldQueue[0]!.payload) as Record<string, unknown>;
            assert.equal((poisonBody["command"] as Record<string, unknown>)["op"], "send-numbered", `poison head is send-numbered${tail()}`);
            assert.deepEqual((poisonBody["command"] as Record<string, unknown>)["source"], { implicit: "home-then-action" });
            const poisonCmd = { ...(poisonBody["command"] as Record<string, unknown>), source: { implicit: "home-then-cached-live" } };
            const mutatedQueuePayload = JSON.stringify({ ...(poisonBody as object), command: poisonCmd });
            broker.answerQueue(mutatedQueuePayload);
            const probeStamped = broker.stampCapture(template);
            const probeCmd = (JSON.parse(probeStamped) as Record<string, unknown>)["command"] as Record<string, unknown> | null;
            assert.ok(probeCmd !== null && String(probeCmd["op"]) === "send-numbered", `probe carries the mutated retired head${tail()}`);
            assert.deepEqual(probeCmd["source"], { implicit: "home-then-cached-live" });
            const probeText = await bridge.send("DescribeWorld", probeStamped);
            const probeReply = JSON.parse(probeText) as Record<string, unknown>;
            assert.equal(probeReply["outcome"], "rejected", `retired token refuses on the real decoder${tail()}`);
            assert.equal(probeReply["kind"], "unknown-value", "retired source fails command decoding");
            broker.bindReply(probeText, template);
            heldQueue[0]!.callback(JSON.stringify({ v: 4, outcome: "rejected", kind: "snapshot-invalid", message: "command-invalid", gen, source: "", nativeInvalid: "0", clock: "1", gestureAssoc: null }));
            heldQueue.length = 0;
            holdQueue = false;
            assert.ok(logs.some((l) => l.includes("command-dropped")), `shape refusal drops the head${tail()}`);
            await settle();

            stampedOps.length = 0;
            h.requestFocus("right");
            runTimers();
            const focusReplies = await settle();
            const focusFiled = commandPayloads("focus-directional");
            assert.ok(focusFiled.length >= 1, `focus files after the drop${tail()}`);
            assert.ok(stampedOps.includes("focus-directional"), `focus drains its own command${tail()}`);
            assert.notEqual(commandReplies.find((r) => r.op === "focus-directional")?.reply["outcome"], "rejected", "focus command capture accepted");
            core = focusReplies.length > 0 ? (focusReplies[focusReplies.length - 1] as Record<string, unknown>) : core;
            assert.notEqual(core["outcome"], "rejected", `focus drains core-accepted after the drop${tail()}`);

            stampedOps.length = 0;
            h.requestWorkspaceSelect(1);
            runTimers();
            const selectReplies = await settle();
            const selectFiled = commandPayloads("select-numbered");
            assert.ok(selectFiled.length >= 1, `select files after the drop${tail()}`);
            assert.ok(stampedOps.includes("select-numbered"), `select drains its own command${tail()}`);
            assert.notEqual(commandReplies.find((r) => r.op === "select-numbered")?.reply["outcome"], "rejected", "select command capture accepted");
            core = selectReplies.length > 0 ? (selectReplies[selectReplies.length - 1] as Record<string, unknown>) : core;
            assert.notEqual(core["outcome"], "rejected", `select drains core-accepted after the drop${tail()}`);

            // Explicit commandless observe after the drop: settle to idle so
            // no command rides, then stamp a fresh commandless report when
            // the entry is quiet (fallback when no DescribeWorld is owed).
            runTimers();
            const pending = calls.findIndex((c, at) => c.method === "DescribeWorld" && !answered.has(at));
            if (pending >= 0) {
                core = await flushAt(pending);
            } else {
                const stamped = broker.stampCapture(template);
                assert.ok((JSON.parse(stamped) as Record<string, unknown>)["command"] === null, `observe is commandless${tail()}`);
                const text = await bridge.send("DescribeWorld", stamped);
                core = JSON.parse(text) as Record<string, unknown>;
                broker.bindReply(text, template);
            }
            assert.notEqual(core["outcome"], "rejected", `commandless observe accepted after the drop${tail()}`);
            assert.ok(BigInt(revOf(core)) > BigInt(revBase), "observe advances beyond the baseline revision");
            assert.ok(Array.isArray(core["windows"]) && (core["windows"] as unknown[]).length > 0, "nonempty fixture still observes windows");
            assert.ok(!logs.some((l) => l.includes("command-busy")), `drop never wedges as busy${tail()}`);
            h.stop();
        } finally {
            await bridge.close();
        }
    });
});
