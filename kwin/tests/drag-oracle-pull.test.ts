import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    DRAG_ORACLE_INTERFACE,
    DRAG_ORACLE_METHOD,
    DRAG_ORACLE_OBJECT,
    DRAG_ORACLE_SERVICE,
    DragOraclePull,
    DragOraclePullEnv,
    formatDragOracleVerdict,
    parseDragOracleVerdict,
    startDragOraclePullEntry,
} from "../src/drag-oracle-pull";

interface DbusCall {
    readonly service: string;
    readonly path: string;
    readonly iface: string;
    readonly method: string;
}

function movedVerdict(): string {
    return JSON.stringify({
        v: 1,
        cancelled: false,
        finalRect: { x: 10, y: 0, w: 120, h: 90 },
        windowIdentity: "win-1",
        correlation: "drag-3",
        reason: "ok-moved",
    });
}

function cancelledVerdict(): string {
    return JSON.stringify({
        v: 1,
        cancelled: true,
        finalRect: { x: 10, y: 20, w: 300, h: 200 },
        windowIdentity: "win-1",
        correlation: "drag-4",
        reason: "no-change",
    });
}

function connectable(): { signal: { connect: (h: () => void) => void; disconnect: (h: () => void) => void }; fire: () => void; disconnects: () => number } {
    const handlers: Array<() => void> = [];
    let disconnects = 0;
    return {
        signal: {
            connect: (handler: () => void): void => {
                handlers.push(handler);
            },
            disconnect: (): void => {
                disconnects += 1;
            },
        },
        fire: (): void => {
            for (const handler of [...handlers]) {
                handler();
            }
        },
        disconnects: (): number => disconnects,
    };
}

describe("drag-oracle pull verdict parsing", () => {
    it("accepts the exact Rust oracle shape with all fields retained", () => {
        const verdict = parseDragOracleVerdict(movedVerdict());
        assert.deepEqual(verdict, {
            cancelled: false,
            finalRect: { x: 10, y: 0, w: 120, h: 90 },
            windowIdentity: "win-1",
            correlation: "drag-3",
            reason: "ok-moved",
        });
    });

    it("accepts the equal-geometry cancelled verdict with all fields retained", () => {
        const verdict = parseDragOracleVerdict(cancelledVerdict());
        assert.deepEqual(verdict, {
            cancelled: true,
            finalRect: { x: 10, y: 20, w: 300, h: 200 },
            windowIdentity: "win-1",
            correlation: "drag-4",
            reason: "no-change",
        });
    });

    it("retains empty identity on cancelled fail-closed verdicts with exact reasons", () => {
        for (const reason of ["empty-identity", "geometry-invalid", "no-observation", "oracle-unavailable", "oracle-panic"]) {
            const verdict = parseDragOracleVerdict(
                JSON.stringify({
                    v: 1,
                    cancelled: true,
                    finalRect: { x: 0, y: 0, w: 1, h: 1 },
                    windowIdentity: "",
                    correlation: "drag-0",
                    reason,
                }),
            );
            assert.deepEqual(verdict, {
                cancelled: true,
                finalRect: { x: 0, y: 0, w: 1, h: 1 },
                windowIdentity: "",
                correlation: "drag-0",
                reason,
            });
        }
        assert.equal(
            parseDragOracleVerdict(
                JSON.stringify({
                    v: 1,
                    cancelled: false,
                    finalRect: { x: 10, y: 0, w: 120, h: 90 },
                    windowIdentity: "",
                    correlation: "drag-3",
                    reason: "ok-moved",
                }),
            ),
            null,
        );
    });

    it("rejects malformed verdicts fail-closed", () => {
        assert.equal(parseDragOracleVerdict("not-json"), null);
        assert.equal(parseDragOracleVerdict(JSON.stringify({ v: 2 })), null);
        assert.equal(
            parseDragOracleVerdict(
                JSON.stringify({
                    v: 1,
                    cancelled: "yes",
                    finalRect: { x: 0, y: 0, w: 1, h: 1 },
                    windowIdentity: "win-1",
                    correlation: "drag-1",
                    reason: "ok-moved",
                }),
            ),
            null,
        );
        // Empty identity is only valid before any observation.
        assert.equal(
            parseDragOracleVerdict(
                JSON.stringify({
                    v: 1,
                    cancelled: true,
                    finalRect: { x: 0, y: 0, w: 1, h: 1 },
                    windowIdentity: "",
                    correlation: "drag-1",
                    reason: "ok-moved",
                }),
            ),
            null,
        );
        assert.equal(
            parseDragOracleVerdict(
                JSON.stringify({
                    v: 1,
                    cancelled: true,
                    finalRect: { x: 0, y: 0, w: 1, h: 1 },
                    windowIdentity: "",
                    correlation: "drag-1",
                    reason: "no-change",
                }),
            ),
            null,
        );
        assert.deepEqual(
            parseDragOracleVerdict(
                JSON.stringify({
                    v: 1,
                    cancelled: true,
                    finalRect: { x: 0, y: 0, w: 1, h: 1 },
                    windowIdentity: "",
                    correlation: "drag-0",
                    reason: "no-observation",
                }),
            ),
            {
                cancelled: true,
                finalRect: { x: 0, y: 0, w: 1, h: 1 },
                windowIdentity: "",
                correlation: "drag-0",
                reason: "no-observation",
            },
        );
    });

    it("accepts only exact drag-<digits> correlations", () => {
        const base = {
            v: 1,
            cancelled: false,
            finalRect: { x: 10, y: 0, w: 120, h: 90 },
            windowIdentity: "win-1",
            reason: "ok-moved",
        };
        for (const correlation of ["drag-0", "drag-1", "drag-42", "drag-9876543210"]) {
            assert.notEqual(parseDragOracleVerdict(JSON.stringify({ ...base, correlation })), null);
        }
        for (const correlation of ["", "drag-", "drag", "DRAG-1", "drag-1a", "drag-a", "drag- 1", "drag+1", "move-1", "drag-1 ", " drag-1", "drag_1", "drag-01x", "other-token", "ok-moved"]) {
            assert.equal(parseDragOracleVerdict(JSON.stringify({ ...base, correlation })), null, correlation);
        }
    });

    it("accepts only the exact emitted reason tokens", () => {
        const base = {
            v: 1,
            cancelled: false,
            finalRect: { x: 10, y: 0, w: 120, h: 90 },
            windowIdentity: "win-1",
            correlation: "drag-7",
        };
        for (const reason of ["ok-moved", "no-change"]) {
            assert.notEqual(parseDragOracleVerdict(JSON.stringify({ ...base, reason })), null, reason);
        }
        for (const reason of ["failure", "error", "unknown", "ok_moved", "OK-MOVED", "ok-moved ", "nochange", "cancelled", "drag-1"]) {
            assert.equal(parseDragOracleVerdict(JSON.stringify({ ...base, reason })), null, reason);
        }
    });

    it("preserves every exact fail-closed empty-identity reason", () => {
        for (const reason of [
            "no-observation",
            "oracle-unavailable",
            "oracle-panic",
            "empty-identity",
            "identity-invalid",
            "identity-too-long",
            "geometry-invalid",
            "geometry-out-of-range",
        ]) {
            const verdict = parseDragOracleVerdict(
                JSON.stringify({
                    v: 1,
                    cancelled: true,
                    finalRect: { x: 0, y: 0, w: 1, h: 1 },
                    windowIdentity: "",
                    correlation: "drag-0",
                    reason,
                }),
            );
            assert.notEqual(verdict, null, reason);
            assert.equal((verdict as { reason: string }).reason, reason);
        }
    });

    it("formats a bounded route-diag line without geometry or identity bytes", () => {
        const line = formatDragOracleVerdict({ cancelled: false, correlation: "drag-3", reason: "ok-moved" });
        assert.equal(line, "omnitiler:route-diag:drag-verdict cancelled=false correlation=drag-3 reason=ok-moved");
    });
});

describe("drag-oracle pull transport", () => {
    it("pulls LastVerdict once and logs the bounded verdict", () => {
        const calls: DbusCall[] = [];
        let pending: ((reply: unknown) => void) | null = null;
        const logs: string[] = [];
        const pull = new DragOraclePull({
            callDbus: (service, path, iface, method, callback) => {
                calls.push({ service, path, iface, method });
                pending = callback;
            },
            log: (message) => {
                logs.push(message);
            },
        });
        pull.pullVerdict();
        assert.equal(calls.length, 1);
        assert.deepEqual(calls[0], {
            service: DRAG_ORACLE_SERVICE,
            path: DRAG_ORACLE_OBJECT,
            iface: DRAG_ORACLE_INTERFACE,
            method: DRAG_ORACLE_METHOD,
        });
        assert.equal(DRAG_ORACLE_METHOD, "LastVerdict");
        assert.ok(pending !== null);
        (pending as (reply: unknown) => void)(movedVerdict());
        assert.deepEqual(logs, [
            "omnitiler:route-diag:drag-pull action=dispatch",
            "omnitiler:route-diag:drag-verdict cancelled=false correlation=drag-3 reason=ok-moved",
            "omnitiler:route-diag:drag-route-missing",
        ]);
        assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-unavailable"));
    });

    it("logs a bounded dispatch when the endpoint never answers", () => {
        const logs: string[] = [];
        const pull = new DragOraclePull({
            callDbus: (): void => {},
            log: (message): void => {
                logs.push(message);
            },
        });
        pull.pullVerdict();
        assert.deepEqual(logs, ["omnitiler:route-diag:drag-pull action=dispatch"]);
    });

    it("logs dispatch before the verdict callback and remains bounded", () => {
        const order: string[] = [];
        const pull = new DragOraclePull({
            callDbus: (_service, _path, _iface, _method, callback) => {
                order.push("call");
                callback(movedVerdict());
            },
            log: (message) => {
                order.push(message);
            },
        });
        pull.pullVerdict();
        assert.deepEqual(order, [
            "omnitiler:route-diag:drag-pull action=dispatch",
            "call",
            "omnitiler:route-diag:drag-verdict cancelled=false correlation=drag-3 reason=ok-moved",
            "omnitiler:route-diag:drag-route-missing",
        ]);
        assert.ok(order.every((line) => line.length <= 128));
    });

    it("still dispatches when diagnostic logging throws", () => {
        let calls = 0;
        const pull = new DragOraclePull({
            callDbus: () => {
                calls += 1;
            },
            log: () => {
                throw new Error("log-failed");
            },
        });
        assert.doesNotThrow(() => pull.pullVerdict());
        assert.equal(calls, 1);
    });

    it("fails closed to one bounded reply-invalid token when the reply is malformed", () => {
        const logs: string[] = [];
        const pull = new DragOraclePull({
            callDbus: (_service, _path, _iface, _method, callback) => {
                callback("garbage");
            },
            log: (message) => {
                logs.push(message);
            },
        });
        pull.pullVerdict();
        assert.deepEqual(logs, [
            "omnitiler:route-diag:drag-pull action=dispatch",
            "omnitiler:route-diag:drag-reply-invalid correlation=none",
        ]);
        assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-unavailable"));
    });

    it("fails closed to one bounded call-thrown token when the transport throws", () => {
        const logs: string[] = [];
        const pull = new DragOraclePull({
            callDbus: () => {
                throw new Error("no-bus");
            },
            log: (message) => {
                logs.push(message);
            },
        });
        pull.pullVerdict();
        assert.deepEqual(logs, [
            "omnitiler:route-diag:drag-pull action=dispatch",
            "omnitiler:route-diag:drag-call-thrown",
        ]);
        assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-unavailable"));
    });

    it("fails closed to one bounded call-missing token when no call binding exists", () => {
        const logs: string[] = [];
        const pull = new DragOraclePull({
            log: (message) => {
                logs.push(message);
            },
        } as DragOraclePullEnv);
        pull.pullVerdict();
        assert.deepEqual(logs, ["omnitiler:route-diag:drag-call-missing"]);
        assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-unavailable"));
    });

    it("fails closed to one bounded route-missing token for a non-cancelled verdict without a route", () => {
        const logs: string[] = [];
        const pull = new DragOraclePull({
            callDbus: (_service, _path, _iface, _method, callback) => {
                callback(movedVerdict());
            },
            log: (message) => {
                logs.push(message);
            },
        });
        pull.pullVerdict();
        assert.deepEqual(logs, [
            "omnitiler:route-diag:drag-pull action=dispatch",
            "omnitiler:route-diag:drag-verdict cancelled=false correlation=drag-3 reason=ok-moved",
            "omnitiler:route-diag:drag-route-missing",
        ]);
        assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-unavailable"));
    });
});

describe("drag-oracle pull entry wiring", () => {
    it("attaches finished signals and pulls on finish", () => {
        const first = connectable();
        const second = connectable();
        const added = connectable();
        const calls: DbusCall[] = [];
        const replies: Array<(reply: unknown) => void> = [];
        const logs: string[] = [];
        const workspace = {
            windowList: (): unknown[] => [
                { interactiveMoveResizeFinished: first.signal },
                { interactiveMoveResizeFinished: second.signal },
            ],
            windowAdded: added.signal,
        };
        const handle = startDragOraclePullEntry({
            workspace,
            callDbus: (service, path, iface, method, callback) => {
                calls.push({ service, path, iface, method });
                replies.push(callback);
            },
            log: (message) => {
                logs.push(message);
            },
        });
        assert.ok(handle !== null);
        first.fire();
        assert.equal(calls.length, 1);
        assert.equal(replies.length, 1);
        (replies[0] as (reply: unknown) => void)(cancelledVerdict());
        assert.deepEqual(logs, [
            "omnitiler:route-diag:drag-pull action=dispatch",
            "omnitiler:route-diag:drag-verdict cancelled=true correlation=drag-4 reason=no-change",
            "omnitiler:route-diag:drag-cancelled correlation=drag-4 reason=no-change",
        ]);
        (handle as { stop: () => void }).stop();
        assert.equal(first.disconnects(), 1);
        assert.equal(second.disconnects(), 1);
        assert.equal(added.disconnects(), 1);
    });

    it("attaches late windows through windowAdded and detaches on stop", () => {
        const first = connectable();
        let addedDisconnects = 0;
        let addedHandler: ((window: unknown) => void) | null = null;
        const addedSignal = {
            connect: (handler: (window: unknown) => void): void => {
                addedHandler = handler;
            },
            disconnect: (): void => {
                addedDisconnects += 1;
            },
        };
        const calls: DbusCall[] = [];
        const replies: Array<(reply: unknown) => void> = [];
        const logs: string[] = [];
        const workspace = {
            windowList: (): unknown[] => [{ interactiveMoveResizeFinished: first.signal }],
            windowAdded: addedSignal,
        };
        const handle = startDragOraclePullEntry({
            workspace,
            callDbus: (service, path, iface, method, callback) => {
                calls.push({ service, path, iface, method });
                replies.push(callback);
            },
            log: (message) => {
                logs.push(message);
            },
        });
        assert.ok(handle !== null);
        assert.ok(addedHandler !== null);
        const late = connectable();
        (addedHandler as (window: unknown) => void)({ interactiveMoveResizeFinished: late.signal });
        late.fire();
        assert.equal(calls.length, 1);
        (replies[0] as (reply: unknown) => void)(movedVerdict());
        assert.deepEqual(logs, [
            "omnitiler:route-diag:drag-pull action=dispatch",
            "omnitiler:route-diag:drag-verdict cancelled=false correlation=drag-3 reason=ok-moved",
            "omnitiler:route-diag:drag-route-missing",
        ]);
        (handle as { stop: () => void }).stop();
        assert.equal(first.disconnects(), 1);
        assert.equal(late.disconnects(), 1);
        assert.equal(addedDisconnects, 1);
    });

    it("fails closed with the added-invalid token when windowAdded is missing and detaches finished signals", () => {
        const first = connectable();
        const logs: string[] = [];
        const handle = startDragOraclePullEntry({
            workspace: {
                windowList: (): unknown[] => [{ interactiveMoveResizeFinished: first.signal }],
            },
            callDbus: (_s, _p, _i, _m, _c) => {
                assert.fail("must not call D-Bus without windowAdded");
            },
            log: (message) => {
                logs.push(message);
            },
        });
        assert.equal(handle, null);
        assert.deepEqual(logs, ["omnitiler:route-diag:drag-entry-added-invalid"]);
        assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-entry-invalid"));
        assert.equal(first.disconnects(), 1);
    });

    it("fails closed with the no-finished token when no window exposes a finished subscription", () => {
        const logs: string[] = [];
        const handle = startDragOraclePullEntry({
            workspace: { windowList: (): unknown[] => [{ caption: "no-signals" }] },
            callDbus: (_s, _p, _i, _m, _c) => {
                assert.fail("must not call D-Bus without a subscription");
            },
            log: (message) => {
                logs.push(message);
            },
        });
        assert.equal(handle, null);
        assert.deepEqual(logs, ["omnitiler:route-diag:drag-entry-no-finished"]);
        assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-entry-invalid"));
    });

    it("refuses each distinct startup branch with its own exact drag-entry token", () => {
        const reject = (workspace: unknown, logs: string[]): void => {
            const handle = startDragOraclePullEntry({
                workspace,
                callDbus: (_s, _p, _i, _m, _c) => {
                    assert.fail("must not call D-Bus on a refused start");
                },
                log: (message) => {
                    logs.push(message);
                },
            });
            assert.equal(handle, null);
        };
        const expectToken = (workspace: unknown, token: string): void => {
            const logs: string[] = [];
            reject(workspace, logs);
            assert.deepEqual(logs, [token]);
            assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-entry-invalid"));
        };
        expectToken(null, "omnitiler:route-diag:drag-entry-workspace-missing");
        expectToken({}, "omnitiler:route-diag:drag-entry-list-missing");
        expectToken(
            {
                windowList: (): unknown[] => {
                    throw new Error("list-boom");
                },
            },
            "omnitiler:route-diag:drag-entry-list-thrown",
        );
        expectToken(
            { windowList: (): unknown => "garbage" },
            "omnitiler:route-diag:drag-entry-list-invalid",
        );
        expectToken(
            { windowList: (): unknown[] => [] },
            "omnitiler:route-diag:drag-entry-no-windows",
        );
        const connectThrows = {
            connect: (): void => {
                throw new Error("connect-boom");
            },
            disconnect: (): void => {},
        };
        expectToken(
            {
                windowList: (): unknown[] => [{ interactiveMoveResizeFinished: connectThrows }],
            },
            "omnitiler:route-diag:drag-entry-finished-invalid",
        );
        const addedThrows = {
            connect: (): void => {
                throw new Error("added-boom");
            },
            disconnect: (): void => {},
        };
        const finished = connectable();
        expectToken(
            {
                windowList: (): unknown[] => [{ interactiveMoveResizeFinished: finished.signal }],
                windowAdded: addedThrows,
            },
            "omnitiler:route-diag:drag-entry-added-connect-failed",
        );
    });

    it("refuses a missing callDBus global with the entry-call token", () => {
        (globalThis as Record<string, unknown>)["callDBus"] = "not-a-function";
        try {
            const logs: string[] = [];
            const handle = startDragOraclePullEntry({
                workspace: { windowList: (): unknown[] => [] },
                log: (message) => {
                    logs.push(message);
                },
            });
            assert.equal(handle, null);
            assert.deepEqual(logs, ["omnitiler:route-diag:drag-entry-call-missing"]);
        } finally {
            delete (globalThis as Record<string, unknown>)["callDBus"];
        }
    });

    it("refuses an unavailable callDBus global with the entry-call-thrown token", () => {
        delete (globalThis as Record<string, unknown>)["callDBus"];
        try {
            const logs: string[] = [];
            const handle = startDragOraclePullEntry({
                workspace: { windowList: (): unknown[] => [] },
                log: (message) => {
                    logs.push(message);
                },
            });
            assert.equal(handle, null);
            assert.deepEqual(logs, ["omnitiler:route-diag:drag-entry-call-thrown"]);
        } finally {
            delete (globalThis as Record<string, unknown>)["callDBus"];
        }
    });

    it("keeps Slice 2 routing pull-based with strict no-op cancelled and no planner push", () => {
        const entry = readFileSync("src/entry.ts", "utf8");
        assert.ok(entry.includes("startWorldAdapterEntry"));
        const module = readFileSync("src/drag-oracle-pull.ts", "utf8");
        assert.ok(!module.includes("DescribePlan"));
        assert.ok(!module.includes("DescribeWorld"));
        assert.ok(!module.includes("frameGeometry"));
        assert.ok(!module.includes("setGeometry"));
        assert.ok(module.includes("interactiveMoveResizeFinished"));
        assert.ok(module.includes("windowAdded"));
        assert.ok(module.includes("LastVerdict"));
        assert.ok(module.includes("omnitiler:route-diag"));
        assert.ok(module.includes("finalRect"));
        assert.ok(module.includes("windowIdentity"));
        assert.ok(module.includes("routePointer"));
        assert.ok(module.includes("deriveOracleEdge"));
        // The world production path owns the single finished-handler verdict
        // route (strict move-drop/resize operations, cancelled no-op),
        // split coherently into world-drag and driven by world-entry; the
        // legacy plan path (including its own oracle wiring) is retired.
        const worldEntry = readFileSync("src/world-entry.ts", "utf8");
        assert.ok(worldEntry.includes("world-drag"));
        assert.ok(worldEntry.includes("dragRoute.attach()"));
        const worldDrag = readFileSync("src/world-drag.ts", "utf8");
        assert.ok(worldDrag.includes("drag-oracle-pull"));
        assert.ok(worldDrag.includes("routeOracleVerdict"));
        assert.ok(worldDrag.includes("routePointer"));
        assert.ok(worldDrag.includes("pointer-resize"));
        assert.ok(worldDrag.includes("drag-drop"));
        // The legacy plan route (including its own oracle wiring) is
        // retired: its adapter modules are gone and no production path
        // references them.
        assert.throws(() => readFileSync("src/plan-adapter-entry.ts", "utf8"));
        assert.throws(() => readFileSync("src/plan-adapter.ts", "utf8"));
    });
});
