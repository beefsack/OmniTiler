// World explicit-close tracker (D5): canonical live-id cache plus
// event-gated difference. The removal payload is never read (its QObject
// may be dead); a real windowRemoved event only arms a pending flag, and
// the next provably complete full windowList read promotes cached ids
// absent from that fresh inventory to explicit closes. Minimized/hidden
// windows stay in the full inventory so they never infer close.
// Unreadable/over-cap inventories defer (retain live plus pending) and
// never fabricate. Bounded 10000 on both live and closed.
import { decodeList } from "./qml-list";
import { normalizeNativeId } from "./native-id";

const MAX_TRACKED_WINDOWS = 10000;
const MAX_TRACKED_CLOSED = 10000;

function readFullInventory(surface: object): { complete: boolean; ids: string[] } {
    const incomplete: { complete: boolean; ids: string[] } = { complete: false, ids: [] };
    try {
        let lister: unknown = undefined;
        try {
            lister = Reflect.get(surface, "windowList");
        } catch (error) {
            void error;
            return incomplete;
        }
        if (typeof lister !== "function") {
            return incomplete;
        }
        let raw: unknown = undefined;
        try {
            raw = Reflect.apply(lister as (...a: ReadonlyArray<never>) => unknown, surface, []);
        } catch (error) {
            void error;
            return incomplete;
        }
        const list = decodeList(raw, MAX_TRACKED_WINDOWS);
        if (list === null) {
            return incomplete;
        }
        const seen = new Set<string>();
        const ids: string[] = [];
        for (const item of list) {
            if (typeof item !== "object" || item === null) {
                return incomplete;
            }
            let rawId: unknown = undefined;
            try {
                rawId = Reflect.get(item as object, "internalId");
            } catch (error) {
                void error;
                return incomplete;
            }
            const id = normalizeNativeId(rawId);
            if (id === null) {
                return incomplete;
            }
            if (!seen.has(id)) {
                seen.add(id);
                ids.push(id);
            }
        }
        return { complete: true, ids };
    } catch (error) {
        void error;
        return incomplete;
    }
}

export interface WorldCloseTracker {
    readonly closed: string[];
    readonly noteRemovedEvent: () => void;
    readonly syncLive: (surface: object) => void;
    readonly snapshotClosed: () => string[];
    readonly ack: () => void;
}

export function createWorldCloseTracker(): WorldCloseTracker {
    const live = new Set<string>();
    const closed: string[] = [];
    let pending = false;
    return {
        closed,
        noteRemovedEvent: (): void => {
            pending = true;
        },
        syncLive: (surface: object): void => {
            const fresh = readFullInventory(surface);
            if (!fresh.complete) {
                return;
            }
            if (pending) {
                const present = new Set(fresh.ids);
                for (const id of live) {
                    if (!present.has(id) && !closed.includes(id) && closed.length < MAX_TRACKED_CLOSED) {
                        closed.push(id);
                    }
                }
                pending = false;
            }
            live.clear();
            for (const id of fresh.ids) {
                live.add(id);
            }
        },
        snapshotClosed: (): string[] => [...closed],
        ack: (): void => {
            closed.length = 0;
        },
    };
}
