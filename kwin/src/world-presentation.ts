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
//     completion invalidated during flight still sends (continuity across
//     all-motion invalidation: the native renderer decides every surface
//     on exact causal proof instead of hiding the valid same-gesture
//     preview), as does a clean read with stale monotonic cover but a
//     live-producer preview; both refetch to converge on the covering
//     projection. A structurally-stale send without a live producer is
//     rejected natively with no display touch.
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
// state), and identical or older (gen, rev, obs) repeats never re-send:
// the last painted (or hidden) point is retained across error hides, so a
// stale retransmit can never resurrect. A read paints when its causal
// coverage equals the renderer's latest known witness (same instance,
// equal counter), or when a live-producer preview rides along despite a
// monotonic mismatch (clean or dirty: the mismatch is expected all-motion
// supersession while the native gates decide). Reads with stale coverage
// and no live-producer preview skip and wait for the covering capture's
// own notification; without a covering capture nothing new can paint. A
// preview paints only when its producing native identity matches the live
// open gesture; otherwise the subscriber paints border/underlay only
// (fail-closed strip). Malformed replies hide; transport silence (lost
// callback) hides via the watchdog. The renderer remains the native
// effect; this module never derives geometry, eligibility, or policy.

import {
    buildPresentationReadRequest,
    isWorldPresentationReply,
    parseWorldReply,
    type WorldPresentationReply,
} from "./world-report";
import { isCanonicalU64, isOpaqueId, utf8ByteLength, WORLD_MAX_REPLY_BYTES } from "./world-seq";

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
export const PRESENTATION_EFFECT_HIDE_METHOD = "HidePresentation";

export interface WorldPresentationRenderer {
    // Paint one accepted projection. Returns false when the bus is
    // unavailable (fail closed); the subscriber retries on the next event.
    readonly renderPresentation: (payload: string) => boolean;
    // Display-only hide: clears the visible paint while retaining
    // order/invalidation evidence. Used by the error/watchdog path and
    // same-generation reconnect seeds; stream replacement (rotation,
    // unbind, stop, effect-instance replacement) uses the resetting clear.
    readonly hidePresentation: () => void;
    // Stream-resetting clear: display, ordering fence, and witness floors
    // all reset. Only generation rotation, unbind, stop, and explicit
    // stream replacement use this.
    readonly clearPresentation: () => void;
}

export interface WorldPresentationSubscriberEnv {
    readonly callDbus: (
        service: string, path: string, iface: string, method: string,
        payload: string, callback: (reply: unknown) => void,
    ) => void;
    readonly getGeneration: () => string | null;
    // Latest known causal witness (Stage 5): the native-invalidation
    // source instance plus counter from the latest fetched effect facts,
    // or null when unknown (effect absent/stale/malformed). A clean read
    // paints when its coverage equals this value, or when a live-producer
    // preview rides along despite a monotonic mismatch (clean continuity:
    // the native renderer decides every surface on exact causal proof).
    // A reply invalidated during flight still sends (dirty continuity)
    // under the same native decision.
    readonly getInvalidation: () => { instance: string; counter: string } | null;
    // Live open native gesture identity: the mapped native lifecycle
    // identity behind the current snapshot with its proven core id, or
    // null when no gesture is open (a terminal is not open). A served
    // preview paints only when its producer matches this identity.
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
    // Unknown facts (unavailable/malformed/timeout, not a proven stream
    // replacement): hide display-only, retain order/invalidation evidence
    // and the painted point, and re-read. Only a proven instance or
    // generation replacement resets the stream.
    readonly notifyEffectUnknown: () => void;
    readonly stop: () => void;
}

// Strict effect payload formatter: the exact JSON the native `SetPresentation`
// endpoint parses. Carries only accepted core shapes plus the serving
// generation/revision/observation for native order fencing and the causal
// coverage witness:
//   {v:1,gen,rev,obs,coveredInstance,coveredInvalid,foreground,
//    border:{visible:0/1,inner:[x,y,w,h]|null,outer:[x,y,w,h]|null},
//    underlay:{status:"found",win,outer:[x,y,w,h],members:[win,...]}|
//             {status:"hidden"},
//    preview:{rect,sourceOut,sourceWs,win,producerInstance,producerSerial}|null}
// Member rects never cross: anchoring matches member window ids in native
// stacking order against the already-verbatim core outer rect. `preview.win`
// is the insertion neighbor, never the mover identity; the producer
// identity names the actual producing native gesture. All coordinates ride
// through verbatim in KWin logical units: no rounding, outset, DPI, or
// device scaling at this site (fractional rounding happened once at
// capture with TS Math.round, halves toward +inf; style units converted
// once at ingress). `includePreview=false` paints border/underlay only
// (cross-gesture or unproven-producer strip): hiding, never fabrication.
// Missing or invalid coverage fails closed to null (never sent: no
// drawable coverage without proof).
// Returns null on any unexpected shape (the caller hides instead of
// sending).
export function formatPresentationPayload(reply: WorldPresentationReply, includePreview = true): string | null {
    try {
        if (!isOpaqueId(reply.gen)) {
            return null;
        }
        if (reply.coveredInstance === null || reply.coveredInvalid === null) {
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
                producerInstance: reply.preview.producerInstance,
                producerSerial: reply.preview.producerSerial,
            };
        if (preview !== null && (!ints(preview.rect) || !isOpaqueId(preview.sourceOut) || !isOpaqueId(preview.sourceWs) || !isOpaqueId(preview.win))) {
            return null;
        }
        if (
            preview !== null &&
            ((preview.producerInstance !== null && !isOpaqueId(preview.producerInstance)) ||
                (preview.producerSerial !== null &&
                    (typeof preview.producerSerial !== "string" || !isCanonicalU64(preview.producerSerial))))
        ) {
            return null;
        }
        return JSON.stringify({
            v: 1,
            gen: reply.gen,
            rev: reply.rev,
            obs: reply.obs,
            coveredInstance: reply.coveredInstance,
            coveredInvalid: reply.coveredInvalid,
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
    let inFlight: { gen: string; req: string; nonce: number } | null = null;
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
    // Last painted-or-hidden projection point per generation: equal or
    // older (rev, obs) repeats never re-send, so a stale retransmit can
    // never resurrect after a display-only hide. Retained across
    // error/watchdog hides; nulled only on stream replacement (generation
    // rotation, unbind, effect-instance replacement, reconnect seed, stop)
    // so the next good read repaints.
    let painted: { rev: string; obs: string } | null = null;

    // Canonical decimal u64 ordering without precision loss: longer is
    // bigger, equal length compares lexicographically.
    const orderCovered = (a: string, b: string): number => {
        if (a.length !== b.length) {
            return a.length < b.length ? -1 : 1;
        }
        if (a === b) {
            return 0;
        }
        return a < b ? -1 : 1;
    };
    const paintedCovers = (rev: string, obs: string): boolean => {
        if (painted === null) {
            return false;
        }
        const revOrder = orderCovered(painted.rev, rev);
        if (revOrder !== 0) {
            return revOrder > 0;
        }
        return orderCovered(painted.obs, obs) >= 0;
    };

    // Stream-resetting clear: display, ordering fence, and witness floors
    // all reset. Only generation rotation, unbind, stop, and explicit
    // stream replacement use this; every legitimate new stream starts
    // from an empty fence.
    const clearRendered = (): void => {
        painted = null;
        try {
            env.renderer.clearPresentation();
        } catch (error) {
            void error;
        }
    };

    // Display-only hide: clears the visible paint while retaining the
    // ordering fence, the causal witness, and the geometry floors, so a
    // stale retransmit still cannot resurrect. Used by the error/watchdog
    // path and same-generation reconnect seeds: the last painted-or-hidden
    // point is retained, so only strictly newer (rev, obs) repaints.
    const hideRendered = (): void => {
        try {
            env.renderer.hidePresentation();
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
        inFlight = { gen, req, nonce };
        dirty = false;
        try {
            env.callDbus(
                PRESENTATION_SERVICE, PRESENTATION_OBJECT, PRESENTATION_INTERFACE,
                PRESENTATION_READ_METHOD, payload, (reply: unknown) => {
                    onReadReply(nonce, gen, req, reply);
                },
            );
        } catch (error) {
            void error;
            // Synchronous transport throw: drop the flight, hide any
            // possibly-stale paint (display-only: order/invalidation
            // evidence is preserved), and stay dirty so the next event
            // retries. Never a hot loop: no watchdog is armed on a
            // synchronous throw; recovery rides the next event or the
            // entry's existing host-deadline chains.
            if (inFlight !== null && inFlight.nonce === nonce) {
                inFlight = null;
            }
            cancelWatchdog();
            hideRendered();
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
    // wedged flight, hide the possibly-stale paint display-only (never
    // retain visible stale shapes after transport failure, never reset
    // the ordering evidence), and re-drive exactly one read.
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
        hideRendered();
        dirty = true;
        pump();
    };

    const onReadReply = (nonce: number, gen: string, req: string, reply: unknown): void => {
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
            // correlation: hide display-only instead of painting anything
            // stale, then stand down to the next event (no self re-arm,
            // no polling; the armed watchdog was already cancelled above).
            // The last painted-or-hidden point is retained, so a stale
            // retransmit of the same bytes can never resurrect.
            env.log(`omnitiler:world:presentation-malformed req=${token(req)} recovery=hide`);
            hideRendered();
            dirty = true;
            return;
        }
        // Continuity across all-motion invalidation (finding E refined): a
        // reply invalidated while in flight still sends instead of hiding
        // the valid same-gesture preview. The native renderer decides every
        // surface on exact causal proof (monotonic cover for
        // borders/underlay, the structural floor plus live producer for the
        // preview), so a structurally-stale send is rejected natively with
        // no display touch. Consume the dirt; the refetch after the send
        // converges on the covering projection.
        let relaxed = dirty;
        if (relaxed) {
            env.log(`omnitiler:world:presentation-continuity-during-flight req=${token(req)} recovery=paint-then-refetch`);
            dirty = false;
        }
        if (paintedCovers(parsed.rev, parsed.obs)) {
            // Equal or older (rev, obs): the last painted-or-hidden point
            // already covers this content (including a display-only hide
            // after an error). Never re-sends, never resurrects. A
            // relaxed read that lands here refetches to converge on the
            // covering projection; a strict read stands down.
            if (relaxed) {
                dirty = true;
                pump();
            }
            return;
        }
        // Causal coverage gate (Stage 5): this read paints only when it
        // covers the renderer's latest known witness exactly, unless a
        // live-producer preview rides along (relaxed below). A strict read
        // that predates a native invalidation skips (its paint could not
        // cover current) and waits for the covering capture's own
        // notification; without a covering capture nothing new can paint.
        // Relaxed reads bypass this gate (their mismatch is expected
        // all-motion supersession) while the native gates decide. Missing
        // proof on either side hides: no drawable coverage is ever
        // guessed.
        let latest: { instance: string; counter: string } | null = null;
        try {
            latest = env.getInvalidation();
        } catch (error) {
            void error;
            latest = null;
        }
        const coverageCurrent = latest !== null && parsed.coveredInstance !== null && parsed.coveredInvalid !== null &&
            parsed.coveredInstance === latest.instance && parsed.coveredInvalid === latest.counter;
        // Producing-gesture match (Stage 5): a preview paints only when
        // its producing native identity is the live open gesture. A null
        // producer (unproven), a missing live identity, or any mismatch
        // paints border/underlay only (fail-closed strip); the live
        // gesture's own preview arrives with its own covering projection.
        // Hoisted above the coverage gate: a clean read with stale
        // monotonic cover but a live-producer preview still sends (clean
        // continuity), delegating the coverage decision to the native
        // endpoint instead of hiding a valid same-gesture preview over a
        // mere motion mismatch.
        let live: { instance: string; serial: string; coreId: string } | null = null;
        try {
            live = env.getGestureIdentity?.() ?? null;
        } catch (error) {
            void error;
            live = null;
        }
        const previewOk = parsed.preview === null || (
            parsed.preview.producerInstance !== null &&
            parsed.preview.producerSerial !== null &&
            live !== null &&
            parsed.preview.producerInstance === live.instance &&
            parsed.preview.producerSerial === live.serial
        );
        if (!relaxed && !coverageCurrent) {
            if (parsed.preview !== null && previewOk) {
                relaxed = true;
                env.log(`omnitiler:world:presentation-continuity-stale-coverage req=${token(req)} recovery=paint-then-refetch`);
            } else {
                env.log(`omnitiler:world:presentation-stale-coverage req=${token(req)} recovery=refetch-or-next-event`);
                return;
            }
        }
        const payload = formatPresentationPayload(parsed, previewOk);
        if (payload === null) {
            env.log(`omnitiler:world:presentation-malformed req=${token(req)} recovery=hide`);
            hideRendered();
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
            // retries; the last painted-or-hidden point is untouched so no
            // stale state is asserted.
            dirty = true;
            env.log(`omnitiler:world:presentation-unavailable req=${token(req)} recovery=on-next-event`);
            return;
        }
        painted = { rev: parsed.rev, obs: parsed.obs };
        // Submitted (not proven-painted: the endpoint is fire-and-forget)
        // preview bounds with producer proof for drop correlation.
        let previewLog = "preview=null";
        if (parsed.preview !== null && previewOk) {
            const producerInstance = parsed.preview.producerInstance;
            const producerSerial = parsed.preview.producerSerial;
            if (producerInstance !== null && producerSerial !== null) {
                previewLog = `preview=rect previewRect=${parsed.preview.rect[0]},${parsed.preview.rect[1]},${parsed.preview.rect[2]},${parsed.preview.rect[3]} producer=${token(producerInstance)}/${token(producerSerial)}`;
            }
        }
        env.log(`omnitiler:world:presentation-painted rev=${token(parsed.rev)} obs=${token(parsed.obs)} ${previewLog}`);
        if (relaxed) {
            // Converge on the covering projection: the invalidation that
            // relaxed this flight means a newer capture may already be
            // cached. One bounded refetch (no loop: the refetch paints or
            // skips on its own gates and stands down without new events).
            dirty = true;
            pump();
        }
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
            // instead of holding a paint from before the restart. The
            // hide is display-only (order/invalidation evidence is
            // preserved); the painted point resets so the fresh read
            // re-sends even for an identical projection.
            painted = null;
            try {
                env.renderer.hidePresentation();
            } catch (error) {
                void error;
            }
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
        // Finding H: the raw presentation-facts instance was replaced by a
        // proven new one (effect restart with a new instance). The
        // fire-and-forget renderer call succeeds vacuously while the
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

    const notifyEffectUnknown = (): void => {
        // Unknown facts (unavailable, malformed, timeout, or teardown):
        // NOT an explicit stream replacement. Hide display-only, retain
        // order/invalidation evidence and the painted point (a stale
        // retransmit of hidden bytes still cannot resurrect), then re-read;
        // the next read skips until facts return, with no reset anywhere.
        if (stopped || boundGen === null) {
            return;
        }
        hideRendered();
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

    return { notifyGeneration, notifyInvalidated, notifyEffectReset, notifyEffectUnknown, stop };
}
