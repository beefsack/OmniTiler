import type { WorldObservation } from "./world-observe";

export const MAX_FOCUS_CHANGES = 64;

export type FocusChange =
    | { readonly kind: "foreground"; readonly win: string | null }
    | { readonly kind: "view"; readonly out: string; readonly native: string };

export interface FocusChanges {
    readonly after: string | null;
    readonly events: ReadonlyArray<FocusChange> | null;
}

export interface FocusChangesCut {
    readonly through: number;
    readonly interval: FocusChanges;
}

interface FocusRow { serial: number; event: FocusChange; proved: boolean }
export interface FrozenFocusPrefix {
    through: number;
    gapAt: number;
    interrupted: boolean;
    rows: ReadonlyArray<FocusRow>;
}

// Local serials delimit report prefixes, not native time or broker ordering.
export class FocusChangesTracker {
    private serial = 0;
    private incorporated = 0;
    private after: string | null = null;
    private gapAt = 0;
    private interrupted = false;
    private events: FocusRow[] = [];
    private provisional = new Map<number, FocusRow>();
    private foreground: string | null | undefined;
    private views = new Map<string, string>();

    reset(): void {
        // Keep serials monotone across a reentrant reset.
        this.incorporated = ++this.serial;
        this.after = null;
        this.gapAt = 0;
        this.events = [];
        this.provisional.clear();
        this.foreground = undefined;
        this.views.clear();
    }

    mark(): number { return this.serial; }

    interrupt(): void { this.interrupted = true; }

    pending(): boolean { return this.events.length > 0 || this.gapAt > this.incorporated; }

    gap(): void { this.gapAt = ++this.serial; }

    record(event: FocusChange, provisional = false): number {
        if (event.kind === "foreground") {
            this.foreground = event.win;
        } else if (this.views.has(event.out) || this.views.size < MAX_FOCUS_CHANGES) {
            this.views.set(event.out, event.native);
        } else {
            this.gap();
        }
        if (this.events.length === MAX_FOCUS_CHANGES) {
            // Discard the bounded prefix, retaining its loss marker. A
            // flight that already froze it can still retire its own cut.
            this.gapAt = Math.max(this.gapAt, this.events[MAX_FOCUS_CHANGES - 1]!.serial);
            this.events = [];
        }
        const row = { serial: ++this.serial, event, proved: false };
        this.events.push(row);
        if (provisional) {
            this.provisional.set(row.serial, row);
        }
        return row.serial;
    }

    // An in-call echo occupies its actual position until post-call proof
    // succeeds. Failure leaves the original event, including when later
    // input arrived while the call was still on the stack.
    prove(serial: number): void {
        const row = this.provisional.get(serial);
        if (row !== undefined) {
            row.proved = true;
            this.provisional.delete(serial);
        }
        const index = this.events.findIndex((entry) => entry.serial === serial);
        if (index !== -1) {
            this.events.splice(index, 1);
        }
    }

    leaveUnproved(serial: number): void { this.provisional.delete(serial); }

    checkView(out: string, native: string | null): boolean {
        if (this.views.get(out) === (native ?? "")) {
            return true;
        }
        this.views.set(out, native ?? "");
        this.gap();
        return false;
    }

    checkForeground(win: string | null): boolean {
        if (win === this.foreground) {
            return true;
        }
        this.foreground = win;
        this.gap();
        return false;
    }

    snapshot(observation: WorldObservation, start: number): FrozenFocusPrefix {
        const foreground = observation.foregroundKnown ? observation.focusedGlobal : null;
        let gap = this.serial !== start ||
            (this.foreground !== undefined && this.foreground !== foreground);
        this.foreground = foreground;
        const views = new Map<string, string>();
        for (const { out, current } of observation.topology) {
            const prior = this.views.get(out);
            if (current === "" || (prior !== undefined && prior !== current)) {
                gap = true;
            }
            views.set(out, current);
        }
        this.views = views;
        if (gap) {
            this.gap();
        }
        return {
            through: this.serial, gapAt: this.gapAt, interrupted: this.interrupted,
            rows: [...this.events],
        };
    }

    cut(prefix: FrozenFocusPrefix): FocusChangesCut {
        return {
            through: prefix.through,
            interval: {
                after: this.after,
                events: prefix.interrupted || prefix.gapAt > this.incorporated
                    ? null : prefix.rows.filter((row) => row.serial > this.incorporated && !row.proved)
                        .map(({ event }) => event),
            },
        };
    }

    freeze(observation: WorldObservation, start: number): FocusChangesCut {
        return this.cut(this.snapshot(observation, start));
    }

    incorporate(cut: FocusChangesCut, rev: string): void {
        this.after = rev;
        this.incorporated = Math.max(this.incorporated, cut.through);
        this.events = this.events.filter(({ serial }) => serial > this.incorporated);
    }
}
