import { startWorldAdapterEntry, mintAdapterIncarnation } from "./world-entry";
import { TrayPublisher } from "./tray-publisher";

// Production entry: the single world-state adapter owns all KWin observation
// and actuation over DiscoverWorld/DescribeWorld. Only normal windows are
// observed with stable opaque ids, frame rectangles, output, workspace, and
// focus; Rust owns every tiling, order, membership, and rejection decision
// through the full world report/reply boundary and the adapter applies the
// complete desired snapshot per property in the shared canonical order.
// Directional focus/move and direction-plus-mode resize shortcuts issue
// parameterized world operation commands, workspace selection/send/migration
// shortcuts issue Planner-resolved world commands, and window/scope signals
// feed one debounced fresh-snapshot resync. The legacy plan route is
// removed; only the world route exists on this path.

const trayTimers = new Set<QTimer>();

// The world adapter owns no tiling state locally; the tray snapshot projects
// the live scope from the latest accepted Planner mapping. The tray invokes
// the toggle over KGlobalAccel; the menu waits for the next published state.
const trayHolder: { current: TrayPublisher | null } = { current: null };
const planHandle = startWorldAdapterEntry({
    owner: "kwin-world-adapter",
    // Fresh incarnation per evaluation: reset counters open a fresh sequence
    // namespace against the retained Planner instead of fencing as
    // superseded/consumed. The owner stays stable so sessions survive.
    adapter: mintAdapterIncarnation(),
    // Accepted and ignored: the core generation binds through authenticated
    // discovery, and float membership rides the full observation.
    generation: "world-1",
    floatIntent: true,
    onWorkspaceTilingChanged: () => {
        try {
            trayHolder.current?.notifyWorkspaceChanged();
        } catch (error) {
            void error;
        }
    },
});

const trayPublisher = new TrayPublisher({
    isEnabled: () => true,
    log: (message) => console.log(message),
    getScope: () => planHandle?.getWorkspaceTilingSnapshot().scope ?? "",
    isTiled: () => planHandle?.getWorkspaceTilingSnapshot().tiled ?? true,
    getDefaultTiled: () => planHandle?.getWorkspaceTilingSnapshot().defaultTiled ?? true,
    publishSnapshot: (schema, generation, revision, enabled, currentScope, tiled, defaultTiled) => {
        callDBus(
            "com.omnitiler.Tray",
            "/com/omnitiler/Tray",
            "com.omnitiler.Tray1",
            "PublishSnapshot",
            schema,
            generation,
            revision,
            enabled,
            currentScope,
            tiled,
            defaultTiled,
        );
    },
    scheduleOnce: (delayMs, callback) => {
        const timer = new QTimer();
        trayTimers.add(timer);
        timer.interval = delayMs;
        timer.singleShot = true;
        timer.timeout.connect(() => {
            try {
                callback();
            } finally {
                trayTimers.delete(timer);
            }
        });
        timer.start();
        return () => {
            timer.stop();
            trayTimers.delete(timer);
        };
    },
});

trayPublisher.start();
trayHolder.current = trayPublisher;

void planHandle;
