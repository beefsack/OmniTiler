import {
    worldRosterAppendedNative, MAX_ROSTER_NATIVE, type WorldFocusRequest, type WorldRosterFeedback,
    type WorldRosterHome, type WorldRosterPlan, type WorldRosterReadback, type WorldRosterSlot,
    type WorldRosterStepFeedback,
} from "./world-report";
import { compareU64, isOpaqueId } from "./world-seq";

export interface WorldRosterWitness {
    order: readonly string[];
    currents: ReadonlyMap<string, string>;
    // Complete occupancy, including unmanaged and subset occupants. A missing
    // entry is allowed only for a proved empty slot, never for an unknown slot.
    occupants: ReadonlyMap<string, ReadonlyMap<string, string>>;
}

export interface WorldRosterHost {
    // Includes generation, issuing-publication applicability, gesture and
    // external-input fences. Synchronous own echoes must be correlated by host.
    fresh(): boolean;
    read(): WorldRosterWitness | null;
    memberHome(win: string): WorldRosterHome | null;
    canAffect(win: string): boolean;
    canWrite(out: string): boolean;
    canRemove(native: string): boolean;
    append(): boolean;
    move(win: string, out: string, native: string): boolean;
    select(out: string, native: string): boolean;
    remove(native: string): boolean;
    // Independent frame lane: one notification per proved member, per apply.
    // False means the publication no longer authorizes dependent work.
    memberArrived?(win: string, out: string, native: string): boolean;
    // True when resolved (including retired/spent grant); false when the grant
    // itself remains unspent and may be considered on another publication.
    focus(request: WorldFocusRequest, resolve: (out: string, ws: string) => string | null): boolean;
}

export interface WorldRosterResult {
    feedback: WorldRosterFeedback | null;
    complete: boolean;
    pendingMember: { step: string; win: string; out: string; native: string } | null;
    stopped: "stale" | "preflight" | "blocked" | "pending" | "unproved" | "focus" | "reentrant" | null;
}

type Stop = Exclude<WorldRosterResult["stopped"], null>;
type PendingArrival =
    | { kind: "member"; row: WorldRosterPlan["members"][number]; before: WorldRosterHome & { kind: "home" }; native: string }
    | { kind: "view"; row: WorldRosterPlan["views"][number]; before: { kind: "current"; native: string }; native: string };
interface RetainedPlan {
    op: string;
    gen: string;
    rev: string;
    steps: Map<string, WorldRosterStepFeedback>;
    pending: PendingArrival | null;
    focusResolved: boolean;
    terminal: Stop | null;
}

export interface WorldRosterSpendState {
    // Retained for one generation. Keep the latest revision as a high-water
    // mark even after its operation is no longer published.
    active: RetainedPlan | null;
    running: boolean;
    interrupted: boolean;
}

export function createWorldRosterSpendState(): WorldRosterSpendState {
    return { active: null, running: false, interrupted: false };
}

const KDE_ROSTER_FLOOR = 2;

function sameOrder(before: readonly string[], after: readonly string[]): boolean {
    return before.length === after.length && before.every((native, i) => after[i] === native);
}

function sameHome(a: WorldRosterHome | null, b: WorldRosterHome): boolean {
    return a !== null && a.out === b.out && a.native === b.native;
}

interface ExpectedRoster {
    order: string[];
    currents: Map<string, string>;
    occupants: Map<string, Map<string, string>>;
    // Safety is scoped to affected (output,native) pairs, not an unchanged
    // sibling that happens to share the same desktop ID.
    affected: Map<string, Set<string>>;
    writable: Set<string>;
    appended: Map<string, string>;
}

function prepare(plan: WorldRosterPlan): ExpectedRoster | null {
    const { baseline, slots } = plan;
    const order = [...baseline.order];
    // Startup may observe one desktop; only growth can bring it to the floor.
    if (order.length === 0 || order.length > MAX_ROSTER_NATIVE ||
        slots.length < KDE_ROSTER_FLOOR || slots.length > MAX_ROSTER_NATIVE ||
        new Set(order).size !== order.length) return null;
    const removals = new Set(plan.removals.map((row) => row.native));
    const surviving = order.filter((native) => !removals.has(native));
    if (removals.size !== plan.removals.length || surviving.length + removals.size !== order.length ||
        (removals.size > 0 && slots.length !== surviving.length) || slots.length < surviving.length) return null;
    const appendSteps = new Set<string>();
    const steps = new Set<string>();
    for (const [index, slot] of slots.entries()) {
        if (index < surviving.length) {
            if (slot.kind !== "existing" || slot.native !== surviving[index]) return null;
        } else {
            if (slot.kind !== "append" || steps.has(slot.step)) return null;
            appendSteps.add(slot.step);
            steps.add(slot.step);
        }
    }
    const currents = new Map(plan.currents.map((row) => [row.out, row.native]));
    const before = new Map(baseline.projections.map((row) => [row.out, row.rows]));
    const after = new Map(plan.projections.map((row) => [row.out, row.rows]));
    if (currents.size === 0 || currents.size !== plan.currents.length || currents.size !== before.size ||
        currents.size !== after.size || before.size !== baseline.projections.length ||
        after.size !== plan.projections.length) return null;
    const occupants = new Map(plan.occupants.map((row) => [row.native, new Map(row.windows.map((w) => [w.win, w.out]))]));
    if (occupants.size !== plan.occupants.length || plan.occupants.some((row) => !order.includes(row.native) ||
        occupants.get(row.native)?.size !== row.windows.length)) return null;
    const affected = new Map<string, Set<string>>();
    const writable = new Set<string>();
    const mark = (native: string, out: string): void => {
        let outputs = affected.get(native);
        if (outputs === undefined) affected.set(native, outputs = new Set());
        outputs.add(out);
        if (currents.has(out)) writable.add(out);
    };
    for (const [out, current] of currents) {
        const old = before.get(out);
        const next = after.get(out);
        if (!old || !next || !order.includes(current) || old.length > order.length || next.length > slots.length) return null;
        for (const [index, native] of order.entries()) {
            const desired = surviving.indexOf(native);
            if (desired < 0 || old[index] !== next[desired]) mark(native, out);
        }
        if (next.length > surviving.length) writable.add(out);
    }
    const register = (step: string, deps: readonly string[]): boolean => {
        if (steps.has(step) || !deps.every((dep) => steps.has(dep))) return false;
        steps.add(step);
        return true;
    };
    const targetAllowed = (slot: WorldRosterSlot): boolean => slot.kind === "existing"
        ? surviving.includes(slot.native) : appendSteps.has(slot.step);
    const moved = new Set<string>();
    for (const row of plan.members) {
        if (!register(row.step, row.deps) || moved.has(row.win) || !currents.has(row.out) ||
            occupants.get(row.before.native)?.get(row.win) !== row.before.out || !targetAllowed(row.target) ||
            (row.before.out === row.out && row.target.kind === "existing" && row.before.native === row.target.native)) return null;
        moved.add(row.win);
        mark(row.before.native, row.before.out);
        writable.add(row.out);
        if (row.target.kind === "existing") mark(row.target.native, row.out);
    }
    const selected = new Set<string>();
    for (const row of plan.views) {
        if (!register(row.step, row.deps) || selected.has(row.out) || currents.get(row.out) !== row.before ||
            !targetAllowed(row.target) || (row.target.kind === "existing" && row.target.native === row.before)) return null;
        selected.add(row.out);
        mark(row.before, row.out);
        if (row.target.kind === "existing") mark(row.target.native, row.out);
    }
    for (const row of plan.removals) {
        if (!register(row.step, row.deps)) return null;
        for (const out of occupants.get(row.native)?.values() ?? []) mark(row.native, out);
    }
    for (const native of affected.keys()) if (!occupants.has(native)) return null;
    return { order, currents, occupants, affected, writable, appended: new Map() };
}

function pendingHomeAllowed(expected: ExpectedRoster, pending: PendingArrival & { kind: "member" }, out: string, native: string): boolean {
    if ((out === pending.before.out || out === pending.row.out) &&
        (native === pending.before.native || native === pending.native)) return true;
    // KWin can carry the source's shown window onto the destination's
    // current desktop before the separate membership setter arrives.
    return pending.before.out !== pending.row.out && out === pending.row.out &&
        expected.currents.get(pending.before.out) === pending.before.native && expected.currents.get(out) === native;
}

function matchesWitness(expected: ExpectedRoster, witness: WorldRosterWitness, pending?: PendingArrival): boolean {
    if (!sameOrder(expected.order, witness.order) || expected.currents.size !== witness.currents.size) return false;
    for (const [out, current] of expected.currents) {
        const actual = witness.currents.get(out);
        if (actual !== current && !(pending?.kind === "view" && out === pending.row.out && actual === pending.native)) return false;
    }
    const ignored = pending?.kind === "member" ? pending.row.win : undefined;
    for (const [native, windows] of expected.occupants) {
        const actual = witness.occupants.get(native);
        const oldSize = windows.size - (ignored !== undefined && windows.has(ignored) ? 1 : 0);
        const newSize = (actual?.size ?? 0) - (ignored !== undefined && actual?.has(ignored) ? 1 : 0);
        if (oldSize !== newSize) return false;
        for (const [win, out] of windows) if (win !== ignored && actual?.get(win) !== out) return false;
    }
    if (pending?.kind === "member") {
        for (const [native, occupants] of witness.occupants) {
            const out = occupants.get(pending.row.win);
            if (out !== undefined && !pendingHomeAllowed(expected, pending, out, native)) return false;
        }
    }
    return true;
}

export function applyWorldRoster(plan: WorldRosterPlan, host: WorldRosterHost, state: WorldRosterSpendState): WorldRosterResult {
    if (state.running) {
        state.interrupted = true;
        return { feedback: null, complete: false, pendingMember: null, stopped: "reentrant" };
    }
    let retained = state.active;
    if (retained === null || (retained.gen === plan.gen && compareU64(plan.rev, retained.rev) > 0)) {
        retained = { op: plan.op, gen: plan.gen, rev: plan.rev, steps: new Map(), pending: null, focusResolved: false, terminal: null };
        state.active = retained;
    } else if (retained.gen !== plan.gen || retained.rev !== plan.rev || retained.op !== plan.op) {
        return { feedback: null, complete: false, pendingMember: null, stopped: "stale" };
    }
    const { op, gen, rev, steps } = retained;
    const result = (stopped: WorldRosterResult["stopped"]): WorldRosterResult => ({
        pendingMember: retained.pending?.kind === "member" ? {
            step: retained.pending.row.step, win: retained.pending.row.win,
            out: retained.pending.row.out, native: retained.pending.native,
        } : null,
        feedback: steps.size === 0 ? null : { op, gen, rev, steps: Array.from(steps.values()) },
        complete: stopped === null,
        stopped,
    });
    if (retained.terminal !== null) return result(retained.terminal);
    const expected = prepare(plan);
    if (expected === null) return result("preflight");

    state.running = true;
    state.interrupted = false;
    try {
        const fresh = (): boolean => !state.interrupted && host.fresh() && !state.interrupted;
        const resolve = (slot: WorldRosterSlot): string | undefined => slot.kind === "existing"
            ? slot.native : expected.appended.get(slot.step);
        const moveExpected = (row: WorldRosterPlan["members"][number], native: string): void => {
            expected.occupants.get(row.before.native)?.delete(row.win);
            expected.occupants.get(native)?.set(row.win, row.out);
            expected.affected.get(native)?.add(row.out);
        };
        const appendExpected = (step: string, native: string): void => {
            expected.order.push(native);
            expected.appended.set(step, native);
            expected.occupants.set(native, new Map());
            expected.affected.set(native, new Set());
        };
        const removeExpected = (native: string): void => {
            expected.order.splice(expected.order.indexOf(native), 1);
            expected.occupants.delete(native);
            expected.affected.delete(native);
        };
        const flagsAllow = (): boolean => {
            for (const out of expected.writable) if (!host.canWrite(out)) return false;
            for (const [native, outputs] of expected.affected) {
                for (const [win, out] of expected.occupants.get(native) ?? []) {
                    if (outputs.has(out) && !host.canAffect(win)) return false;
                }
            }
            return true;
        };
        // Reentrant reads, safety callbacks and setters can all invalidate the
        // bracket. Take the order/current/occupancy cut after safety reads.
        const checked = (): WorldRosterWitness | null => {
            if (!fresh() || !flagsAllow()) return null;
            const live = host.read();
            return live !== null && matchesWitness(expected, live) && fresh() ? live : null;
        };
        const proved = (step: string): boolean => {
            const status = steps.get(step)?.status;
            return status === "applied" || status === "appended";
        };
        const replay = (): boolean => {
            let accounted = 0;
            for (const slot of plan.slots) {
                if (slot.kind !== "append") continue;
                const row = steps.get(slot.step);
                if (row === undefined) break;
                if (row.status !== "appended" || row.after?.kind !== "order" || row.before?.kind !== "order" ||
                    !sameOrder(expected.order, row.before.order)) return false;
                const native = worldRosterAppendedNative(row.before.order, row.after.order);
                if (native === null) return false;
                appendExpected(slot.step, native);
                accounted++;
            }
            for (const row of plan.members) {
                const previous = steps.get(row.step);
                if (previous === undefined) break;
                const native = resolve(row.target);
                if (native === undefined || !row.deps.every(proved) ||
                    previous.before?.kind !== "home" || !sameHome(previous.before, row.before) ||
                    previous.after?.kind !== "home" || !sameHome(previous.after, { out: row.out, native })) return false;
                moveExpected(row, native);
                accounted++;
            }
            for (const row of plan.views) {
                const previous = steps.get(row.step);
                if (previous === undefined) continue;
                const native = resolve(row.target);
                if (native === undefined || !row.deps.every(proved) ||
                    previous.before?.kind !== "current" || previous.before.native !== row.before ||
                    previous.after?.kind !== "current" || previous.after.native !== native) return false;
                expected.currents.set(row.out, native);
                accounted++;
            }
            for (const row of plan.removals) {
                const previous = steps.get(row.step);
                if (previous === undefined) break;
                if (!row.deps.every(proved) || previous.before?.kind !== "order" || !sameOrder(previous.before.order, expected.order) ||
                    previous.after?.kind !== "order" ||
                    !sameOrder(previous.after.order, expected.order.filter((native) => native !== row.native))) return false;
                removeExpected(row.native);
                accounted++;
            }
            return accounted === steps.size;
        };

        const terminal = (reason: Stop): WorldRosterResult => {
            retained.terminal = reason;
            return result(reason);
        };
        const sampleOrder = (live: WorldRosterWitness | null): WorldRosterReadback | null => {
            if (live === null || live.order.length > MAX_ROSTER_NATIVE || !live.order.every(isOpaqueId) ||
                new Set(live.order).size !== live.order.length) return null;
            return { kind: "order", order: [...live.order] };
        };
        const sampleHome = (win: string): WorldRosterReadback & { kind: "home" } | null => {
            const home = host.memberHome(win);
            return home !== null && isOpaqueId(home.out) && isOpaqueId(home.native)
                ? { kind: "home", out: home.out, native: home.native } : null;
        };
        const sampleCurrent = (live: WorldRosterWitness | null, out: string): (WorldRosterReadback & { kind: "current" }) | null => {
            const native = live?.currents.get(out);
            return isOpaqueId(native) ? { kind: "current", native } : null;
        };
        const refuseArrival = (pending: PendingArrival, after: WorldRosterReadback | null, stop: Stop): WorldRosterResult => {
            retained.pending = null;
            steps.set(pending.row.step, { step: pending.row.step, status: "refused", before: pending.before, after });
            return terminal(stop);
        };
        const finishArrival = (pending: PendingArrival): WorldRosterResult | null => {
            const live = host.read();
            const after = pending.kind === "member" ? sampleHome(pending.row.win) : sampleCurrent(live, pending.row.out);
            if (state.interrupted) return refuseArrival(pending, after, "reentrant");
            if (!fresh()) return result("stale");
            const contradictoryHome = pending.kind === "member" && after?.kind === "home" &&
                !pendingHomeAllowed(expected, pending, after.out, after.native);
            if (contradictoryHome || (live !== null && !matchesWitness(expected, live, pending))) {
                return refuseArrival(pending, after, "unproved");
            }
            if (live === null || after === null) return result("pending");
            if (pending.kind === "member") {
                if (after.kind !== "home" || !sameHome(after, { out: pending.row.out, native: pending.native }) ||
                    live.occupants.get(pending.native)?.get(pending.row.win) !== pending.row.out) return result("pending");
                for (const [native, occupants] of live.occupants) {
                    if (native !== pending.native && occupants.has(pending.row.win)) return result("pending");
                }
                if (!host.canAffect(pending.row.win)) return result("blocked");
                moveExpected(pending.row, pending.native);
            } else {
                if (after.kind !== "current" || after.native !== pending.native) return result("pending");
                expected.currents.set(pending.row.out, pending.native);
            }
            if (checked() === null) return result(state.interrupted ? "reentrant" : "blocked");
            retained.pending = null;
            steps.set(pending.row.step, { step: pending.row.step, status: "applied", before: pending.before, after });
            return null;
        };
        const dispatchArrival = (pending: PendingArrival, setter: () => boolean): WorldRosterResult | null => {
            // Arm and spend before dispatch. Accepted async writes retain their
            // original preimage and do not emit a false terminal refusal.
            retained.pending = pending;
            let accepted = false;
            try { accepted = setter() === true; } catch (_error) { /* Setter may have executed. */ }
            const stop = finishArrival(pending);
            if (accepted || stop === null || retained.terminal !== null) return stop;
            let after: WorldRosterReadback | null = null;
            try {
                after = pending.kind === "member" ? sampleHome(pending.row.win) : sampleCurrent(host.read(), pending.row.out);
            } catch (_error) { /* Preserve an unreadable postimage. */ }
            return refuseArrival(pending, after, state.interrupted ? "reentrant" : "unproved");
        };
        if (!replay()) return result("preflight");
        if (retained.pending !== null) {
            if (!fresh()) return result("stale");
            const stop = finishArrival(retained.pending);
            if (stop !== null) return stop;
        }
        if (checked() === null) return result(state.interrupted ? "reentrant" : "preflight");
        for (const row of plan.members) {
            if (!proved(row.step) && (!sameHome(sampleHome(row.win), row.before) || !fresh())) return result("preflight");
        }
        const dispatch = (
            step: string,
            before: WorldRosterReadback,
            setter: () => boolean,
            read: () => WorldRosterReadback | null,
            prove: (after: WorldRosterReadback) => "applied" | "appended" | null,
        ): WorldRosterResult | null => {
            // Arm and spend before the call; never redeliver possible effects.
            steps.set(step, { step, status: "refused", before, after: null });
            let accepted = false;
            try { accepted = setter() === true; } catch (_error) { /* Setter may have executed. */ }
            let after: WorldRosterReadback | null = null;
            try { after = read(); } catch (_error) { /* Preserve an unreadable postimage. */ }
            const status = accepted && after !== null ? prove(after) : null;
            steps.set(step, { step, status: status ?? "refused", before, after });
            if (state.interrupted) return terminal("reentrant");
            if (status === null) return terminal("unproved");
            if (checked() === null) return terminal(state.interrupted ? "reentrant" : "stale");
            return null;
        };
        for (const slot of plan.slots) {
            if (slot.kind !== "append" || proved(slot.step)) continue;
            const before = sampleOrder(checked());
            if (before?.kind !== "order" || before.order.length >= MAX_ROSTER_NATIVE) return result("blocked");
            const stop = dispatch(slot.step, before, () => host.append(), () => sampleOrder(host.read()), (after) => {
                if (after.kind !== "order") return null;
                const native = worldRosterAppendedNative(before.order, after.order);
                if (native === null) return null;
                appendExpected(slot.step, native);
                return "appended";
            });
            if (stop !== null) return stop;
        }
        const runView = (row: WorldRosterPlan["views"][number]): WorldRosterResult | null => {
            if (proved(row.step)) return null;
            const native = resolve(row.target);
            if (native === undefined || !row.deps.every(proved)) return result("blocked");
            const before = sampleCurrent(checked(), row.out);
            if (before?.kind !== "current" || before.native !== row.before) return result("blocked");
            return dispatchArrival({ kind: "view", row, before, native }, () => host.select(row.out, native));
        };
        // An unrelated selector has no arrival prerequisite, even when its
        // descriptive transfer ID resembles a migration's refill ID.
        if (plan.members.length !== 0) {
            for (const row of plan.views) {
                if (row.deps.some((dep) => plan.members.some((member) => member.step === dep)) ||
                    !row.deps.every(proved)) continue;
                const stop = runView(row);
                if (stop !== null) return stop;
            }
        }
        for (const row of plan.members) {
            const native = resolve(row.target);
            if (native === undefined || !row.deps.every(proved)) return result("blocked");
            if (!proved(row.step)) {
                if (checked() === null) return result("blocked");
                const before = sampleHome(row.win);
                if (before === null || !sameHome(before, row.before) || !fresh()) return result("blocked");
                const stop = dispatchArrival({ kind: "member", row, before, native }, () => host.move(row.win, row.out, native));
                if (stop !== null) return stop;
            }
            if (host.memberArrived !== undefined) {
                if (checked() === null || !sameHome(sampleHome(row.win), { out: row.out, native }) || !fresh() ||
                    !host.memberArrived(row.win, row.out, native) || !fresh()) {
                    return result(state.interrupted ? "reentrant" : "stale");
                }
            }
        }
        for (const row of plan.views) {
            const stop = runView(row);
            if (stop !== null) return stop;
        }
        if (plan.focus !== null && !retained.focusResolved) {
            if (checked() === null || !host.canWrite(plan.focus.out) || !fresh()) return result("focus");
            // Existing focus grant code is itself one-shot. A false result
            // leaves it unspent; a throw is ambiguous and cannot be replayed.
            try {
                const projections = new Map(plan.projections.map((row) => [row.out, row.rows]));
                retained.focusResolved = host.focus(plan.focus, (out, ws) => {
                    const index = projections.get(out)?.indexOf(ws) ?? -1;
                    const slot = plan.slots[index];
                    return slot === undefined ? null : resolve(slot) ?? null;
                }) === true;
            }
            catch (_error) { return terminal("focus"); }
            if (state.interrupted) return terminal("reentrant");
            if (!retained.focusResolved) return result("focus");
            if (checked() === null) return terminal(state.interrupted ? "reentrant" : "stale");
        }
        for (const row of plan.removals) {
            if (proved(row.step)) continue;
            if (!row.deps.every(proved) || !fresh() || !host.canRemove(row.native)) return result("blocked");
            const live = checked();
            const before = sampleOrder(live);
            if (live === null || before?.kind !== "order" || before.order.length <= KDE_ROSTER_FLOOR ||
                !before.order.includes(row.native) || (live.occupants.get(row.native)?.size ?? 0) !== 0 ||
                Array.from(live.currents.values()).includes(row.native)) return result("blocked");
            const target = before.order.filter((native) => native !== row.native);
            const stop = dispatch(row.step, before, () => host.remove(row.native), () => sampleOrder(host.read()), (after) => {
                if (after.kind !== "order" || !sameOrder(target, after.order)) return null;
                removeExpected(row.native);
                return "applied";
            });
            if (stop !== null) return stop;
        }
        return result(null);
    } catch (_error) {
        return result(state.interrupted ? "reentrant" : "blocked");
    } finally {
        state.running = false;
    }
}
