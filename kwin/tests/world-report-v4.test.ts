// v4 native-fact report producer (K1b1): exact canonical bytes, nullable
// honesty, broker-namespace omission, and fail-closed encoding. Offline
// only: scripted in-memory workspace fakes with native KWin property
// spellings, no D-Bus, no KWin.
//
// NOTE on string escaping: the producer never relies on the KWin
// JSON.stringify for string bytes. The KWin short escapes (\b \t \n \f
// \r) are LIVE-unproved and would differ from the canonical writer, so
// encodeV4String spells every C0 control as lowercase \u00xx and the
// tests below lock that divergence.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { captureWorldObservation } from "../src/world-observe";
import { buildWorldReportV4InnerJson, encodeV4String } from "../src/world-report";
import { mapNativePid } from "../src/world-connection";

function makeDesk(id: string) {
    return { id };
}

function makeWin(over: Record<string, unknown>) {
    return {
        internalId: "win-a",
        normalWindow: true,
        dialog: false,
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        modal: false,
        wantsInput: true,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minimized: false,
        hidden: false,
        frameGeometry: { x: 8.4, y: 8.6, width: 800, height: 600 },
        clientGeometry: { x: 10, y: 30, width: 796, height: 578 },
        minSize: { width: 100, height: 100 },
        // Native unbounded maximum: KWin reports no cap as INT_MAX on
        // every backend (X11 fallback in src/utils/xcbutils.h, Wayland
        // 0-to-INT_MAX map in src/xdgshellwindow.cpp, base
        // Window::maxSize), never (0,0). Rides verbatim; the core
        // sanitizes out-of-bound values to absent at use.
        maxSize: { width: 2147483647, height: 2147483647 },
        output: { name: "out-a" },
        desktops: [makeDesk("native-a")],
        pid: 1234,
        resourceClass: "Example",
        caption: "Window",
        windowRole: "normal",
        desktopFileName: "example.app",
        resizeable: true,
        keepAbove: false,
        keepBelow: false,
        ...over,
    };
}

function makeSurface(wins: Array<Record<string, unknown>>) {
    const desktops = [makeDesk("native-a")];
    const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    return {
        windowList: () => wins,
        screens,
        desktops,
        activeScreen: { name: "out-a" },
        activeWindow: wins[0] ?? null,
        cursorPos: { x: 20, y: 30 },
        windowAt: () => (wins.length > 0 ? [wins[0]] : []),
        stackingOrder: [...wins].reverse(),
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktopForScreen: () => desktops[0],
    };
}

function observe(surface: object) {
    const obs = captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-a", pin: false, disp: false, managed: true }),
        isFloating: () => false,
    });
    assert.ok(obs !== null);
    return obs;
}

function buildInner(surface: object) {
    const obs = observe(surface);
    const json = buildWorldReportV4InnerJson({
        observation: obs,
        creationAttempts: [],
        frameAttempts: [],
        memberAttempts: [],
        viewAttempts: [],
        clearAttempts: [],
        intentRecords: [],
    });
    assert.ok(json !== null);
    return { obs, json, value: JSON.parse(json) as Record<string, unknown> };
}

describe("v4 canonical string writer", () => {
    it("escapes quote and backslash short, every C0 control as lowercase \\u00xx", () => {
        assert.equal(encodeV4String('a"b\\c'), '"a\\"b\\\\c"');
        let all = "";
        let want = '"';
        for (let c = 0; c < 32; c += 1) {
            all += String.fromCharCode(c);
            want += `\\u00${c.toString(16).padStart(2, "0")}`;
        }
        want += '"';
        assert.equal(encodeV4String(all), want);
        // Non-ASCII passes through verbatim (no \\uXXXX escaping).
        assert.equal(encodeV4String("h\u00e9llo\u4e2d"), '"h\u00e9llo\u4e2d"');
    });

    it("never uses the JSON.stringify short escapes for \\b \\t \\n \\f \\r", () => {
        const encoded = encodeV4String("\b\t\n\f\r");
        assert.equal(encoded, '"\\u0008\\u0009\\u000a\\u000c\\u000d"');
        assert.ok(!encoded.includes("\\b") && !encoded.includes("\\t") &&
            !encoded.includes("\\n") && !encoded.includes("\\f") && !encoded.includes("\\r"));
    });
});

describe("v4 pid mapping", () => {
    it("maps -1 to the own token, positives to decimal text, others to null", () => {
        assert.equal(mapNativePid(-1), "kwin-internal");
        assert.equal(mapNativePid(1234), "1234");
        assert.equal(mapNativePid(0), null);
        assert.equal(mapNativePid(-2), null);
        assert.equal(mapNativePid(Number.NaN), null);
        assert.equal(mapNativePid("1234"), null);
        assert.equal(mapNativePid(undefined), null);
    });
});

describe("v4 inner report shape", () => {
    it("emits the exact top-level key order without the broker namespace", () => {
        const { value } = buildInner(makeSurface([makeWin({})]));
        assert.deepEqual(Object.keys(value), [
            "v", "screens", "windows", "closed", "native", "focused",
            "complete", "gesture", "cursor", "actionForeground",
            "actionCached", "actionOutputs", "presentation",
            "intentRecords", "stateAttempts", "focusFeedback",
            "rendererInput", "nativeGesture", "cursorWindow",
            "nativeInvalidWitness",
        ]);
        assert.equal(value["v"], 4);
        for (const brokerKey of ["adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]) {
            assert.ok(!(brokerKey in value), `broker-owned ${brokerKey} must be absent`);
        }
        for (const retired of ["owner", "mode", "views", "hostOwned", "equalCount", "birthClear", "predicate",
            "refill", "defaultTiled", "switchTarget", "freshPlacement", "focusFollowsCursor", "cursorWarp",
            "anchorFallback", "noSpaceResult", "bornMaximized", "floatIntent", "tileIntent",
            "primary", "cursorHit", "monotonicMs", "gestureIngress",
            "frameAttempts", "memberAttempts", "viewAttempts", "clearAttempts", "creationAttempts"]) {
            assert.ok(!(retired in value), `retired/optional ${retired} must be absent`);
        }
    });

    it("emits the exact window row order with nativeWs and honest nulls", () => {
        const { value } = buildInner(makeSurface([makeWin({})]));
        const windows = value["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.length, 1);
        const row = windows[0] as Record<string, unknown>;
        assert.deepEqual(Object.keys(row), [
            "win", "out", "rect", "fullscreen", "maximized", "sticky",
            "minimized", "hints", "nativeWs", "hostHidden", "privileged",
            "managed", "acceptsInput", "activationInhibited", "toolWindow",
            "owned", "transient", "modal", "resizable", "above", "below",
            "owner", "parent", "role", "appId", "executable", "processId",
            "class", "title", "type", "activationPolicy", "layer",
            "fullscreenOwner", "fullscreenExitable", "suppressionAvailable",
            "visibility", "lastWrite",
        ]);
        assert.deepEqual(row["rect"], [8, 9, 800, 600]);
        assert.deepEqual(row["hints"], { minW: 104, minH: 122, maxW: 2147483647, maxH: 2147483647 });
        assert.equal(row["nativeWs"], "native-a");
        assert.equal(row["role"], "normal");
        assert.equal(row["appId"], "example.app");
        assert.equal(row["processId"], "1234");
        assert.equal(row["class"], "Example");
        assert.equal(row["title"], "Window");
        assert.equal(row["type"], "normal");
        assert.equal(row["managed"], true);
        assert.equal(row["resizable"], true);
        for (const key of ["privileged", "activationInhibited", "toolWindow", "owned",
            "executable", "activationPolicy", "layer", "fullscreenOwner",
            "fullscreenExitable", "owner", "parent", "lastWrite"]) {
            assert.equal(row[key], null, `${key} must ride null`);
        }
        assert.equal(row["suppressionAvailable"], false);
        assert.deepEqual(row["visibility"], { visible: null, suppressionOwned: false });
    });

    it("omits nativeWs when membership is not exactly one desktop", () => {
        const multi = makeWin({ internalId: "win-a", desktops: [makeDesk("native-a"), makeDesk("native-b")] });
        const { value } = buildInner(makeSurface([multi]));
        const row = (value["windows"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>;
        assert.ok(!("nativeWs" in row), "multi-homed nativeWs must omit, never guess");
    });

    it("maps native minimized/hostHidden to visibility without inventing primary", () => {
        const { value } = buildInner(makeSurface([makeWin({ minimized: true })]));
        const row = (value["windows"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>;
        assert.equal(row["minimized"], true);
        assert.equal(row["hostHidden"], false);
        assert.deepEqual(row["visibility"], { visible: false, suppressionOwned: false });
        assert.ok(!("primary" in (value as Record<string, unknown>)), "active screen is not primary proof");
    });

    it("maps dialog candidates to the dialog type with pid token for own windows", () => {
        const dialog = makeWin({
            internalId: "win-d", normalWindow: false, dialog: true, pid: -1,
            transient: true, modal: true, transientFor: null,
        });
        const { value } = buildInner(makeSurface([dialog]));
        const row = (value["windows"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>;
        assert.equal(row["type"], "dialog");
        assert.equal(row["processId"], "kwin-internal");
        assert.equal(row["owner"], null);
    });

    it("fails closed on a closed id that overlaps a live lifetime", () => {
        const obs = observe(makeSurface([makeWin({})]));
        const json = buildWorldReportV4InnerJson({
            observation: { ...obs, closed: ["win-a"] },
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
            intentRecords: [],
        });
        assert.equal(json, null);
    });

    it("spells scale 1 without an exponent and keeps cursor/action facts", () => {
        const { json, value } = buildInner(makeSurface([makeWin({})]));
        assert.ok(json.includes('"scale":1'), "KWin logical scale spells exactly 1");
        const screens = value["screens"] as Array<Record<string, unknown>>;
        assert.deepEqual(Object.keys(screens[0] as Record<string, unknown>),
            ["out", "full", "work", "complete", "scale", "identity"]);
        assert.deepEqual(screens[0]?.["identity"],
            { manufacturer: null, model: null, serial: null, connector: null });
        assert.deepEqual(value["cursor"], [20, 30]);
        assert.equal(value["focused"], "win-a");
        assert.equal(value["actionForeground"], "out-a");
        assert.equal(value["actionCached"], null);
        assert.deepEqual(value["actionOutputs"], ["out-a"]);
        const presentation = value["presentation"] as Record<string, unknown>;
        assert.deepEqual(Object.keys(presentation), [
            "foreground", "minimized", "maximized", "fullscreen",
            "hostHidden", "hiddenWorkspace", "frame", "chordHeld",
            "moveWindow", "pagerOrder", "pagerFrames", "witness", "foregroundOut",
        ]);
        assert.equal(presentation["witness"], null);
    });
});

describe("v4 frame attempt lane", () => {
    function buildWithFrames(frames: Array<Record<string, unknown>>) {
        const obs = observe(makeSurface([makeWin({})]));
        const json = buildWorldReportV4InnerJson({
            observation: obs,
            creationAttempts: [],
            frameAttempts: frames as never,
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
            intentRecords: [],
        });
        return { json, value: json === null ? null : JSON.parse(json) as Record<string, unknown> };
    }

    function ordinary(over: Record<string, unknown> = {}) {
        return {
            win: "win-a",
            target: [0, 0, 200, 200],
            setter: "accepted",
            readback: [0, 0, 200, 200],
            dpiBefore: null,
            dpiAfter: null,
            attemptMs: "7",
            op: "",
            pre: null,
            ...over,
        };
    }

    it("encodes an ordinary accepted write with empty op and present readback", () => {
        const { json, value } = buildWithFrames([ordinary()]);
        assert.ok(json !== null, "ordinary accepted row must encode");
        const rows = (value as Record<string, unknown>)["frameAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows.length, 1);
        assert.deepEqual(Object.keys(rows[0] as Record<string, unknown>), [
            "win", "target", "setter", "readback",
            "dpiBefore", "dpiAfter", "attemptMs", "op", "pre",
        ]);
        assert.equal(rows[0]?.["op"], "");
        assert.deepEqual(rows[0]?.["readback"], [0, 0, 200, 200]);
        assert.equal(rows[0]?.["pre"], null);
    });

    it("encodes an ordinary refused write with null readback and null pre", () => {
        const { json, value } = buildWithFrames([ordinary({ setter: "refused", readback: null })]);
        assert.ok(json !== null, "ordinary refused row with unreadable readback must encode");
        const rows = (value as Record<string, unknown>)["frameAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows[0]?.["readback"], null);
        assert.equal(rows[0]?.["op"], "");
        assert.equal(rows[0]?.["pre"], null);
    });

    it("encodes an explicit float placement under its retained operation id", () => {
        const { json, value } = buildWithFrames([ordinary({ op: "op-1", pre: [3, 3, 11, 11] })]);
        assert.ok(json !== null, "explicit placement row must encode");
        const rows = (value as Record<string, unknown>)["frameAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows[0]?.["op"], "op-1");
        assert.deepEqual(rows[0]?.["pre"], [3, 3, 11, 11]);
    });

    it("fails closed on a bad nonempty operation id", () => {
        assert.equal(buildWithFrames([ordinary({ op: "not an id!" })]).json, null);
    });

    it("fails closed on a bad window id", () => {
        assert.equal(buildWithFrames([ordinary({ win: "" })]).json, null);
    });

    it("fails closed on an invalid pre rect", () => {
        assert.equal(buildWithFrames([ordinary({ op: "op-1", pre: [0, 0, 1.5, 2] })]).json, null);
    });

    it("fails closed on a pre-write frame without an operation", () => {
        assert.equal(buildWithFrames([ordinary({ pre: [3, 3, 11, 11] })]).json, null);
    });

    it("fails closed on an invalid target rect", () => {
        assert.equal(buildWithFrames([ordinary({ target: [0, 0, Number.NaN, 2] })]).json, null);
    });
});

describe("v4 member/view attempt lanes", () => {
    function buildWithAttempts(
        members: Array<Record<string, unknown>>,
        views: Array<Record<string, unknown>>,
    ) {
        const obs = observe(makeSurface([makeWin({})]));
        const json = buildWorldReportV4InnerJson({
            observation: obs,
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: members as never,
            viewAttempts: views as never,
            clearAttempts: [],
            intentRecords: [],
        });
        return { json, value: json === null ? null : JSON.parse(json) as Record<string, unknown> };
    }

    function member(over: Record<string, unknown> = {}) {
        return {
            op: "op-9",
            win: "win-a",
            out: "out-2",
            ws: "w2",
            setter: "accepted",
            pre: { kind: "native", out: "out-1", ws: "d1" },
            post: { kind: "native", out: "out-2", ws: "d2" },
            attemptMs: "7",
            ...over,
        };
    }

    function view(over: Record<string, unknown> = {}) {
        return {
            op: "op-9",
            out: "out-2",
            ws: "w2",
            setter: "accepted",
            pre: { kind: "native", out: "out-2", ws: "d1" },
            post: { kind: "native", out: "out-2", ws: "d2" },
            base: { kind: "native", out: "out-2", ws: "d1" },
            attemptMs: "7",
            ...over,
        };
    }

    it("encodes native member evidence in canonical key order without sidecars", () => {
        const { json, value } = buildWithAttempts([member()], []);
        assert.ok(json !== null, "native member row must encode");
        const rows = (value as Record<string, unknown>)["memberAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows.length, 1);
        assert.deepEqual(Object.keys(rows[0] as Record<string, unknown>), [
            "op", "win", "out", "ws", "setter", "pre", "post", "attemptMs",
        ]);
        // The cross-output preimage keeps its source output, never the target.
        assert.ok(json.includes('"pre":{"kind":"native","out":"out-1","ws":"d1"}'), "pre ref spells kind/out/ws");
        assert.ok(json.includes('"post":{"kind":"native","out":"out-2","ws":"d2"}'), "post ref spells kind/out/ws");
        for (const sidecar of ["nativePre", "nativePost", "nativeBase"]) {
            assert.ok(!json.includes(`"${sidecar}"`), `retired ${sidecar} must be absent`);
        }
        assert.ok(json.includes('"attemptMs":"7"'), "original write stamp preserved");
    });

    it("keeps logical member evidence tagged logical", () => {
        const { json } = buildWithAttempts([member({
            pre: { kind: "logical", out: "out-1", ws: "w9" },
            post: { kind: "logical", out: "out-1", ws: "w1" },
        })], []);
        assert.ok(json !== null, "logical member row must encode");
        assert.ok(json.includes('"pre":{"kind":"logical","out":"out-1","ws":"w9"}'), "logical pre stays tagged");
        assert.ok(json.includes('"post":{"kind":"logical","out":"out-1","ws":"w1"}'), "logical post stays tagged");
    });

    it("encodes a native view row with pre/post/base and no sidecars", () => {
        const { json, value } = buildWithAttempts([], [view()]);
        assert.ok(json !== null, "native view row must encode");
        const rows = (value as Record<string, unknown>)["viewAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows.length, 1);
        assert.deepEqual(Object.keys(rows[0] as Record<string, unknown>), [
            "op", "out", "ws", "setter", "pre", "post", "base", "attemptMs",
        ]);
        assert.ok(json.includes('"base":{"kind":"native","out":"out-2","ws":"d1"}'), "baseline spells kind/out/ws");
        for (const sidecar of ["nativePre", "nativePost", "nativeBase"]) {
            assert.ok(!json.includes(`"${sidecar}"`), `retired ${sidecar} must be absent`);
        }
    });

    it("keeps unreadable evidence null on both lanes", () => {
        const { json, value } = buildWithAttempts(
            [member({ pre: null, post: null })],
            [view({ pre: null, post: null, base: null })],
        );
        assert.ok(json !== null, "null-evidence rows must encode");
        const members = (value as Record<string, unknown>)["memberAttempts"] as Array<Record<string, unknown>>;
        const views = (value as Record<string, unknown>)["viewAttempts"] as Array<Record<string, unknown>>;
        assert.equal(members[0]?.["pre"], null);
        assert.equal(members[0]?.["post"], null);
        assert.equal(views[0]?.["pre"], null);
        assert.equal(views[0]?.["post"], null);
        assert.equal(views[0]?.["base"], null);
        assert.ok(json.includes('"pre":null'), "null evidence stays null");
    });

    it("fails closed on a malformed ref id on either lane", () => {
        assert.equal(buildWithAttempts([member({ pre: { kind: "native", out: "out-1", ws: "bad id" } })], []).json, null);
        assert.equal(buildWithAttempts([member({ post: { kind: "native", out: "", ws: "d2" } })], []).json, null);
        assert.equal(buildWithAttempts([], [view({ base: { kind: "native", out: "out-2", ws: "" } })]).json, null);
    });

    it("fails closed on a logical view ref, which has no wire spelling", () => {
        assert.equal(buildWithAttempts([], [view({ pre: { kind: "logical", out: "out-2", ws: "w2" } })]).json, null);
    });

    it("fails closed on a bad operation id on either lane", () => {
        assert.equal(buildWithAttempts([member({ op: "not an id!" })], []).json, null);
        assert.equal(buildWithAttempts([], [view({ op: "" })]).json, null);
    });
});
