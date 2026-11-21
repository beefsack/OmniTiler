import assert from "node:assert/strict";
import { it } from "node:test";
import { awaitQueueCount, deskById, discover, lastDescribeReply, makeHarness, winById, type FakeDesktop } from "./world-producer-harness";

it("KWin flag off keeps the requested mode and executes a core-degraded shared selection", async () => {
    const options = Object.freeze({ perOutputVirtualDesktops: false });
    const h = await makeHarness("win-a", ["win-a", "win-b"], {
        options,
        desktops: ["ws1", "ws2"],
        outputs: [
            { name: "left", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "right", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ],
        homes: { "win-b": "right" },
    });
    try {
        const handshake = JSON.parse(h.calls[0]!.payload);
        winById(h, "win-b").desktops = [deskById(h, "ws2")];
        assert.equal(handshake.capabilities.nativeSelectorScope, "all-outputs");
        await discover(h);
        const settings = JSON.parse(h.calls[1]!.payload);
        assert.equal(settings.values.mode, "per-output-local");
        const settingsReply = JSON.parse(h.replies.get(1)!);
        assert.deepEqual(settingsReply.diagnostics.find((d: { key: string }) => d.key === "mode"), {
            key: "mode", kind: "unsupported-mode",
        });
        const targetRows = (lastDescribeReply(h)["workspaces"] as Array<Record<string, unknown>>)
            .filter((row) => row["bind"] === "ws2");
        assert.deepEqual(targetRows.map((row) => row["out"]).sort(), ["left", "right"]);
        assert.equal(targetRows[0]?.["id"], targetRows[1]?.["id"], "both outputs share one logical workspace");

        h.handle.requestWorkspaceSelect(2);
        await awaitQueueCount(h, "select-numbered", 1);
        await h.flush();
        await h.settle();
        assert.deepEqual(h.viewWrites, [], "the coupled per-output setter must not be called");
        assert.deepEqual(h.sharedViewWrites, ["ws2"]);
        const current = h.workspace["currentDesktopForScreen"] as (output: { name: string }) => FakeDesktop;
        assert.equal(current({ name: "left" }).id, "ws2");
        assert.equal(current({ name: "right" }).id, "ws2");
    } finally {
        await h.stop();
    }
});
