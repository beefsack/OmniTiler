// Read-only presentation subscriber (Stage 5): event-driven consumer of the
// broker `ReadWorldPresentation` projection. One subscriber per adapter, no
// polling; the only timer is the shared host-deadline watchdog for a lost
// read reply (B1-verified: KWin `callDBus` errors/timeouts never invoke the
// callback), which hides stale shapes and re-drives exactly one read per
// host deadline. No one-shot/write/result lanes.
//
// Triggers (all caller-driven, never self-armed except the bounded
// watchdog retry):
//   - `notifyGeneration(gen)`: discovery bind or rotation (reconnect seed),
//     or unbind (rotation/death: hide immediately, drop flights).
//   - `notifyInvalidated()`: an accepted capture or wait wake may have
//     advanced the cached projection. Coalesces while a read flies; a
//     completion invalidated during flight skips its stale paint and
//     refetches current first (finding E: no old render at all).
//   - `notifyEffectReset()`: the raw presentation-facts instance changed
//     or went unavailable (effect restart/teardown, already read by the
//     entry). The fire-and-forget renderer call succeeds vacuously while
//     the endpoint is down and the same-projection dedupe would never
//     resend, so this event forgets the painted projection and re-reads.
//   - `stop()`: drops flights, cancels the watchdog, clears the renderer.
//
// Fencing: every read carries a caller-minted `{v:2,req}` correlation; the
// serving generation binds server-side. Only the latest issued request may
// paint (older or rotated completions never resurrect or clear newer
// state), replies invalidated during flight never paint, and identical
// (gen, rev, obs) repeats never re-send. A preview that crosses a native
// gesture boundary (read issued before the latest gesture start/finish,
// completing after) paints border/underlay only: the preview is dropped
// (fail-closed hide) because its insertion-neighbor rect belongs to the
// old gesture. Malformed replies hide; transport silence (lost callback)
// hides via the watchdog. The renderer remains the native effect; this
// module never derives geometry, eligibility, or policy.

import {
    buildPresentationReadRequest,
    isWorldPresentationReply,
    parseWorldReply,
    type WorldPresentationReply,
} from "./world-report";
import { isOpaqueId, utf8ByteLength, WORLD_MAX_REPLY_BYTES } from "./world-seq";

export const PRESENTATION_SERVICE = "com.omnitiler.Planner";
export const PRESENTATION_OBJECT = "/com/omnitiler/Planner";
export const PRESENTATION_INTERFACE = "com.omnitiler.Planner1";
export const PRESENTATION_READ_METHOD = "ReadWorldPresentation";

// Shared host call deadline for a lost read reply (finding A): KWin
// scripting uses sessionBus().asyncCall with no timeout, Qt passes -1 to
// libdbus, which substitutes 25 s; errors and timeouts only log and never
// invoke the JS callback (B1-verified transport contract). Same 25 s value
// as world-entry WAIT_HOST_DEADLINE_MS, kept as a local constant to avoid
// an import cycle. One watchdog per read flight; fire hides stale shapes
// and re-drives exactly one read (host-deadline cadence, never a hot
// loop). Recovery needs no native event beyond the deadline itself.
export const PRESENTATION_READ_DEADLINE_MS = 25000;

// Native effect presentation endpoint (owned by the active-border effect,
// never the generic effect bus). The renderer consumes complete core
// shapes only; native code keeps anchoring plus immediate hide-only vetoes.
export const PRESENTATION_EFFECT_SERVICE = "com.omnitiler.ActiveBorder";
export const PRESENTATION_EFFECT_OBJECT = "/com/omnitiler/ActiveBorder";
export const PRESENTATION_EFFECT_INTERFACE = "com.omnitiler.ActiveBorder1";
export const PRESENTATION_EFFECT_SET_METHOD = "SetPresentation";
export const PRESENTATION_EFFECT_CLEAR_METHOD = "ClearPresentation";

export interface WorldPresentationRenderer {
    // Paint one accepted projection. Returns false when the bus is
    // unavailable (fail closed); the subscriber retries on the next event.
    readonly renderPresentation: (payload: string) => boolean;
    readonly clearPresentation: () => void;
}

export interface WorldPresentationSubscriberEnv {
    readonly callDbus: (
        service: string, path: string, iface: string, method: string,
        payload: string, callback: (reply: unknown) => void,
    ) => void;
    readonly getGeneration: () => string | null;
    // Monotonic native gesture boundary counter (finding D): bumped by
    // the entry on every authoritative native gesture boundary (a new
    // native identity adoption or a terminal adoption). A preview painted
    // from a read issued in an older epoch is dropped (fail-closed hide).
    readonly getGestureEpoch: () => number;
    // Read-only authoritative gesture identity (NEXT package): the mapped
    // native lifecycle identity behind the current snapshot, or null when
    // no gesture is open. No freshness enforcement in this package.
    readonly getGestureIdentity?: () => { instance: string; serial: string; coreId: string } | null;
    readonly isStopped: () => boolean;
    readonly log: (message: string) => void;
    readonly token?: (value: unknown) => string;
    readonly renderer: WorldPresentationRenderer;
    // Existing host-deadline timer machinery (no new timers invented):
    // schedule `callback` once after `delayMs`, returning its cancel.
    readonly scheduleOnce: (delayMs: number, callback: () => void) => () => void;
}

export interface WorldPresentationSubscriber {
    readonly notifyGeneration: (gen: string | null) => void;
    readonly notifyInvalidated: () => void;
    readonly notifyEffectReset: () => void;
    readonly stop: () => void;
}

// Strict effect payload formatter: the exact JSON the native `SetPresentation`
// endpoint parses. Carries only accepted core shapes plus the serving
// generation/revision/observation for native order fencing:
//   {v:1,gen,rev,obs,foreground,
//    border:{visible:0/1,inner:[x,y,w,h]|null,outer:[x,y,w,h]|null},
//    underlay:{status:"found",win,outer:[x,y,w,h],members:[win,...]}|
//             {status:"hidden"},
//    preview:{rect,sourceOut,sourceWs,win}|null}
// Member rects never cross: anchoring matches member window ids in native
// stacking order against the already-verbatim core outer rect. `preview.win`
// is the insertion neighbor, never the mover identity. All coordinates ride
// through verbatim in KWin logical units: no rounding, outset, DPI, or
// device scaling at this site (fractional rounding happened once at
// capture with TS Math.round, halves toward +inf; style units converted
// once at ingress). `includePreview=false` paints border/underlay only
// (finding D cross-gesture strip): hiding, never fabrication.
// Returns null on any unexpected shape (the caller hides instead of
// sending).
export function formatPresentationPayload(reply: WorldPresentationReply, includePreview = true): string | null {
    try {
        if (!isOpaqueId(reply.gen)) {
            return null;
        }
        const border = reply.presentation.border;
        const borderOut = {
            visible: border.visible ? 1 : 0,
            inner: border.inner === null ? null : [...border.inner],
            outer: border.outer === null ? null : [...border.outer],
        };
        const underlay = reply.presentation.underlay;
        const underlayOut = underlay.status === "found"
            ? {
                status: "found",
                win: underlay.win,
                outer: [...underlay.outer],
                members: underlay.members.map((m) => m.win),
            }
            : { status: "hidden" };
        // Defensive re-validation (the lane parser already proved these):
        // malformed numbers or ids fail closed to null, never a partial send.
        const ints = (v: unknown): boolean =>
            Array.isArray(v) && v.length === 4 &&
            (v as ReadonlyArray<unknown>).every((n) => typeof n === "number" && Number.isInteger(n));
        if (!(borderOut.inner === null || ints(borderOut.inner)) || !(borderOut.outer === null || ints(borderOut.outer))) {
            return null;
        }
        if (underlayOut.status === "found") {
            const found = underlayOut as { status: string; win: unknown; outer: unknown; members: unknown };
            if (!isOpaqueId(found.win) || !ints(found.outer) || !Array.isArray(found.members)) {
                return null;
            }
            for (const m of found.members as ReadonlyArray<unknown>) {
                if (!isOpaqueId(m)) {
                    return null;
                }
            }
        }
        const preview = reply.preview === null || !includePreview
            ? null
            : {
                rect: [...reply.preview.rect],
                sourceOut: reply.preview.sourceOut,
                sourceWs: reply.preview.sourceWs,
                win: reply.preview.win,
            };
        if (preview !== null && (!ints(preview.rect) || !isOpaqueId(preview.sourceOut) || !isOpaqueId(preview.sourceWs) || !isOpaqueId(preview.win))) {
            return null;
        }
        return JSON.stringify({
            v: 1,
            gen: reply.gen,
            rev: reply.rev,
            obs: reply.obs,
            foreground: reply.presentation.foreground,
            border: borderOut,
            underlay: underlayOut,
            preview,
        });
    } catch (error) {
        void error;
        return null;
    }
}

export function createWorldPresentationSubscriber(env: WorldPresentationSubscriberEnv): WorldPresentationSubscriber {
    const token = env.token ?? ((value: unknown): string => {
        try {
            return String(value);
        } catch (error) {
            void error;
            return "?";
        }
    });
    let boundGen: string | null = null;
    let stopped = false;
    // Latest-issued request correlation plus a nonce fencing completions:
    // only the newest request for the bound generation may paint; older or
    // rotated completions never resurrect or clear newer state.
    let reqCounter = 0;
    let reqNonce = 0;
    let inFlight: { gen: string; req: string; nonce: number; gestureEpoch: number } | null = null;
    let watchdogCancel: (() => void) | null = null;
    const cancelWatchdog = (): void => {
        try {
            watchdogCancel?.();
        } catch (error) {
            void error;
        }
        watchdogCancel = null;
    };
    let dirty = false;
    // Last painted projection identity: identical (gen, rev, obs) repeats
    // never re-send. Cleared with the renderer on generation changes,
    // effect resets, and malformed replies so the next good read repaints.
    let painted: string | null = null;

    const clearRendered = (): void => {
        painted = null;
        try {
            env.renderer.clearPresentation();
        } catch (error) {
            void error;
        }
    };

    const pump = (): void => {
        if (stopped || inFlight !== null || !dirty) {
            return;
        }
        if (env.isStopped()) {
            return;
        }
        const gen = env.getGeneration() ?? boundGen;
        if (gen === null || gen !== boundGen) {
            return;
        }
        reqCounter += 1;
        reqNonce += 1;
        let rand = "0";
        try {
            rand = Math.floor(Math.random() * 0x100000000).toString(36);
        } catch (error) {
            void error;
            rand = "0";
        }
        const req = `pres-${reqCounter.toString(36)}-${rand}`;
        const payload = buildPresentationReadRequest(req);
        if (payload === null) {
            return;
        }
        const nonce = reqNonce;
        let gestureEpoch = Number.MIN_SAFE_INTEGER;
        try {
            gestureEpoch = env.getGestureEpoch();
        } catch (error) {
            void error;
            // A throwing epoch getter fails closed downstream (every
            // preview strips); the counter read itself never throws in
            // production.
            gestureEpoch = Number.MIN_SAFE_INTEGER;
        }
        inFlight = { gen, req, nonce, gestureEpoch };
        dirty = false;
        try {
            env.callDbus(
                PRESENTATION_SERVICE, PRESENTATION_OBJECT, PRESENTATION_INTERFACE,
                PRESENTATION_READ_METHOD, payload, (reply: unknown) => {
                    onReadReply(nonce, gen, req, gestureEpoch, reply);
                },
            );
        } catch (error) {
            void error;
            // Synchronous transport throw: drop the flight, hide any
            // possibly-stale paint, and stay dirty so the next event
            // retries. Never a hot loop: no watchdog is armed on a
            // synchronous throw; recovery rides the next event or the
            // entry's existing host-deadline chains.
            if (inFlight !== null && inFlight.nonce === nonce) {
                inFlight = null;
            }
            cancelWatchdog();
            clearRendered();
            dirty = true;
            env.log("omnitiler:world:presentation-transport recovery=on-next-event");
            return;
        }
        // Host-deadline watchdog for the lost-reply case (finding A):
        // B1-verified silent errors/timeouts never invoke the callback, so
        // without this a lost reply wedges the flight (and its possibly
        // stale paint) until the next native event.
        cancelWatchdog();
        const onFire = (): void => {
            onWatchdog(nonce, gen, req);
        };
        // Lane identity for offline harnesses that enumerate the shared
        // timer pool (wait/facts suites assert exact host-deadline
        // choreography): lets those assertions skip the presentation retry
        // timer without changing its delay or cadence. Production QTimer
        // behavior is unchanged.
        (onFire as unknown as Record<string, unknown>).presentationReadWatchdog = true;
        try {
            watchdogCancel = env.scheduleOnce(PRESENTATION_READ_DEADLINE_MS, onFire);
        } catch (error) {
            void error;
            watchdogCancel = null;
        }
    };

    // Host-deadline recovery for a lost read reply (finding A): drop the
    // wedged flight, hide the possibly-stale paint (never retain visible
    // stale shapes after transport failure), and re-drive exactly one read.
    // Host-deadline cadence bounds the retry (no hot loop on permanent
    // failure); recovery needs no native event beyond the deadline itself.
    // A stale timeout (superseded flight, rotation, or stop) never clears a
    // newer flight: the nonce plus generation fence first.
    const onWatchdog = (nonce: number, gen: string, req: string): void => {
        watchdogCancel = null;
        if (stopped || inFlight === null || nonce !== inFlight.nonce) {
            return;
        }
        if (gen !== boundGen || env.isStopped()) {
            return;
        }
        inFlight = null;
        env.log(`omnitiler:world:presentation-host-timeout req=${token(req)} recovery=hide-and-retry`);
        clearRendered();
        dirty = true;
        pump();
    };

    const onReadReply = (nonce: number, gen: string, req: string, issuedGestureEpoch: number, reply: unknown): void => {
        if (stopped || inFlight === null || nonce !== inFlight.nonce) {
            // Stale completion (superseded read or rotated generation):
            // never paints, never clears newer state.
            return;
        }
        // This flight completed: cancel its watchdog before any other
        // branch so a late fire can never clear a newer flight.
        cancelWatchdog();
        inFlight = null;
        if (gen !== boundGen || env.isStopped()) {
            return;
        }
        if (typeof reply !== "string") {
            // Transport-shaped failure (non-string answer): stay dirty so
            // the next event or watchdog retries. Never self re-arm: a
            // persistently failing transport must not hot-loop the bus.
            dirty = true;
            env.log("omnitiler:world:presentation-transport recovery=on-next-event");
            return;
        }
        if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
            dirty = true;
            env.log("omnitiler:world:presentation-oversize recovery=on-next-event");
            return;
        }
        const parsed = parseWorldReply(reply);
        if (parsed === null || !isWorldPresentationReply(parsed) || parsed.gen !== boundGen || parsed.req !== req) {
            // Malformed, foreign-lane, rotated-generation, or mismatched
            // correlation: hide instead of painting anything stale, then
            // stand down to the next event (no self re-arm, no polling;
            // the armed watchdog was already cancelled above).
            env.log(`omnitiler:world:presentation-malformed req=${token(req)} recovery=hide`);
            clearRendered();
            dirty = true;
            return;
        }
        if (dirty) {
            // Finding E: this reply was invalidated while in flight, so it
            // is known-stale. Skip the paint entirely (no old render at
            // all) and fetch current before painting anything.
            env.log(`omnitiler:world:presentation-stale-during-flight req=${token(req)} recovery=refetch`);
            pump();
            return;
        }
        const signature = `${parsed.gen}|${parsed.rev}|${parsed.obs}`;
        if (painted !== null && painted === signature) {
            return;
        }
        // Finding D: a preview that crosses a native gesture boundary
        // belongs to the old gesture. Paint border/underlay from this
        // accepted projection, but drop the preview (fail-closed hide);
        // the live gesture's own preview arrives with a newer projection.
        let gestureEpoch = issuedGestureEpoch;
        try {
            gestureEpoch = env.getGestureEpoch();
        } catch (error) {
            void error;
            gestureEpoch = issuedGestureEpoch + 1;
        }
        const previewOk = parsed.preview === null || gestureEpoch === issuedGestureEpoch;
        const payload = formatPresentationPayload(parsed, previewOk);
        if (payload === null) {
            env.log(`omnitiler:world:presentation-malformed req=${token(req)} recovery=hide`);
            clearRendered();
            dirty = true;
            return;
        }
        if (!previewOk) {
            env.log(`omnitiler:world:presentation-preview-crossed-gesture req=${token(req)} recovery=hide-preview`);
        }
        let ok = false;
        try {
            ok = env.renderer.renderPresentation(payload);
        } catch (error) {
            void error;
            ok = false;
        }
        if (!ok) {
            // Bus unavailable (fail closed): stay dirty so the next event
            // retries; the last painted signature is untouched so no stale
            // state is asserted.
            dirty = true;
            env.log(`omnitiler:world:presentation-unavailable req=${token(req)} recovery=on-next-event`);
            return;
        }
        painted = signature;
        env.log(`omnitiler:world:presentation-painted rev=${token(parsed.rev)} obs=${token(parsed.obs)} preview=${parsed.preview === null || !previewOk ? "null" : "rect"}`);
    };

    const notifyGeneration = (gen: string | null): void => {
        if (stopped || env.isStopped()) {
            boundGen = gen;
            return;
        }
        cancelWatchdog();
        if (gen === boundGen && gen !== null) {
            // Reconnect seed on the same generation (service owner change
            // or rediscovery): force one fresh read even though nothing
            // rotated, so a restarted service with an empty cache hides
            // instead of holding a paint from before the restart.
            painted = null;
            inFlight = null;
            dirty = true;
            pump();
            return;
        }
        // Rotation or unbind: the old generation's flights and paints are
        // dead. Clear the renderer immediately (never a stale repaint for
        // the replacement) and drop in-flight completions by nonce fence.
        boundGen = gen;
        inFlight = null;
        clearRendered();
        if (gen !== null) {
            dirty = true;
            pump();
        } else {
            dirty = false;
        }
    };

    const notifyInvalidated = (): void => {
        if (stopped || boundGen === null) {
            return;
        }
        dirty = true;
        pump();
    };

    const notifyEffectReset = (): void => {
        // Finding H: the raw presentation-facts instance changed or went
        // unavailable (effect restart/teardown, already read by the entry).
        // The fire-and-forget renderer call succeeds vacuously while the
        // endpoint is down and the same-projection dedupe would never
        // resend, so forget the painted projection and re-read. The clear
        // precedes the seed (same ordered channel), so no old-generation
        // replacement can follow the reset.
        if (stopped || boundGen === null) {
            painted = null;
            return;
        }
        try {
            env.renderer.clearPresentation();
        } catch (error) {
            void error;
        }
        painted = null;
        inFlight = null;
        cancelWatchdog();
        dirty = true;
        pump();
    };

    const stop = (): void => {
        stopped = true;
        inFlight = null;
        dirty = false;
        cancelWatchdog();
        clearRendered();
    };

    return { notifyGeneration, notifyInvalidated, notifyEffectReset, stop };
}
