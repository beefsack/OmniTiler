// Read-only presentation consumer offline tests (Stage 5): strict parse,
// generation/(revision, observation) fencing with fence retention across
// display-only hides, geometry-dirt floors with strictly-newer clearing,
// underlay anchor inputs, preview publication/clear plus gesture-latch
// clearing, no one-shot authority, value-based integer checks, the exact
// border stroke derivation, and native styling independence. Pure Qt, no
// KWin scene, no session bus, no Rust staticlib: policy lives in core,
// geometry derivation is deleted, and this test pins the renderer-only
// contract.

#include "presentation_consumer.h"
#include "presentation_router.h"

#include <QCoreApplication>
#include <QString>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>

namespace
{

int failures = 0;

void check(bool condition, const char *expression, const char *file, int line)
{
    if (!condition) {
        std::fprintf(stderr, "FAIL: %s (%s:%d)\n", expression, file, line);
        ++failures;
    }
}

#define CHECK(expression) check(expression, #expression, __FILE__, __LINE__)

std::string foundPayload(const std::string &gen, const std::string &rev, const std::string &obs = "4",
    const std::string &covered = "5")
{
    return "{\"v\":1,\"gen\":\"" + gen + "\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"coveredInstance\":\"effect-1\","
        "\"coveredInvalid\":\"" + covered + "\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"found\",\"win\":\"win-2\",\"outer\":[0,0,1200,800],"
        "\"members\":[\"win-1\",\"win-2\"]},\"preview\":null}";
}

std::string previewPayload(const std::string &gen, const std::string &rev, const std::string &obs = "7",
    const std::string &covered = "5", const char *producerInstance = "gesture-1", const char *producerSerial = "1")
{
    return "{\"v\":1,\"gen\":\"" + gen + "\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"coveredInstance\":\"effect-1\","
        "\"coveredInvalid\":\"" + covered + "\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"hidden\"},"
        "\"preview\":{\"rect\":[0,0,396,600],\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\"win\":\"win-a\","
        "\"producerInstance\":\"" + std::string(producerInstance) + "\",\"producerSerial\":\"" + std::string(producerSerial) + "\"}}";
}

std::string unprovenPreviewPayload(const std::string &gen, const std::string &rev, const std::string &obs = "7",
    const std::string &covered = "5")
{
    return "{\"v\":1,\"gen\":\"" + gen + "\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"coveredInstance\":\"effect-1\","
        "\"coveredInvalid\":\"" + covered + "\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"hidden\"},"
        "\"preview\":{\"rect\":[0,0,396,600],\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\"win\":\"win-a\","
        "\"producerInstance\":null,\"producerSerial\":null}}";
}

KWin::PresentationApply applyStr(KWin::PresentationPaint *state, const std::string &payload,
    const char *liveInstance = "", const char *liveSerial = "0")
{
    return KWin::applyPresentationPaint(state, QString::fromStdString(payload),
        QString::fromUtf8(liveInstance), QString::fromUtf8(liveSerial));
}

// Seed effect-authority truth for direct-apply tests (mirrors the
// production endpoint dispatch, which re-seeds latest from the router
// before parsing): no arriving paint may establish latest.
void seedLive(KWin::PresentationPaint *state, const char *instance, quint64 counter)
{
    KWin::noteNativeInvalidation(state, QString::fromUtf8(instance), counter, KWin::InvalidationScope::None);
}

void validFoundAppliesCompleteShapes()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "4", "9")) == KWin::PresentationApply::Accepted);
    CHECK(state.has);
    CHECK(state.gen == QStringLiteral("gen-1"));
    CHECK(state.rev == 4);
    CHECK(state.obs == 9);
    CHECK(state.foreground == QByteArray("win-2"));
    CHECK(state.borderVisible);
    CHECK(state.borderInner == QRect(10, 10, 300, 300));
    CHECK(state.borderOuter == QRect(8, 8, 304, 304));
    CHECK(state.underlayFound);
    CHECK(state.underlayWin == QByteArray("win-2"));
    CHECK(state.underlayOuter == QRect(0, 0, 1200, 800));
    CHECK(state.underlayMembers.size() == 2);
    CHECK(state.underlayMembers.at(0) == QStringLiteral("win-1"));
    CHECK(state.underlayMembers.at(1) == QStringLiteral("win-2"));
    CHECK(!state.previewHas);
    CHECK(state.fence.set);
    CHECK(state.fence.gen == QStringLiteral("gen-1"));
    CHECK(state.fence.rev == 4);
    CHECK(state.fence.obs == 9);
    CHECK(KWin::presentationBorderFresh(&state));
    CHECK(KWin::presentationUnderlayFresh(&state));
}

void previewPublishesTrimmedGeometryOnly()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "7"), "gesture-1", "1") == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    CHECK(state.previewRect == QRect(0, 0, 396, 600));
    CHECK(state.previewSourceOut == QByteArray("out-1"));
    CHECK(state.previewSourceWs == QByteArray("ws-1"));
    CHECK(state.previewWin == QByteArray("win-a"));
    // The producing native gesture rides alongside: the latch arms only
    // for a matching live open gesture, never for the insertion neighbor.
    CHECK(state.previewProducerInstance == QByteArray("gesture-1"));
    CHECK(state.previewProducerSerial == QByteArray("1"));
    // A null preview clears the drop target without touching the rest.
    CHECK(applyStr(&state, foundPayload("gen-1", "8", "8")) == KWin::PresentationApply::Accepted);
    CHECK(!state.previewHas);
}

void newerRevisionReplacesOlderIgnored()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "10", "10")) == KWin::PresentationApply::Accepted);
    const QRect shown = state.underlayOuter;
    // Strictly older (rev, obs) in the same generation: ignored, display
    // preserved, never resurrected or cleared.
    CHECK(applyStr(&state, foundPayload("gen-1", "9", "11")) == KWin::PresentationApply::StaleIgnored);
    CHECK(state.has);
    CHECK(state.rev == 10);
    CHECK(state.underlayOuter == shown);
    // Same revision with an older observation is still older: ignored.
    CHECK(applyStr(&state, foundPayload("gen-1", "10", "9")) == KWin::PresentationApply::StaleIgnored);
    CHECK(state.rev == 10);
    // Same (rev, obs) redelivers idempotently (reconnect seed repaint).
    CHECK(applyStr(&state, foundPayload("gen-1", "10", "10")) == KWin::PresentationApply::Accepted);
    CHECK(state.rev == 10);
    // Same revision with a newer observation applies: different
    // observations under the same state revision can change presentation.
    CHECK(applyStr(&state, previewPayload("gen-1", "10", "11"), "gesture-1", "1") == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    CHECK(state.rev == 10);
    CHECK(state.obs == 11);
    // Newer revision replaces.
    CHECK(applyStr(&state, previewPayload("gen-1", "11", "11"), "gesture-1", "1") == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    CHECK(state.rev == 11);
}

void generationRotationAlwaysReplaces()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "99", "99")) == KWin::PresentationApply::Accepted);
    // A new generation replaces even with a numerically older revision:
    // generations are separate order namespaces, never compared.
    CHECK(applyStr(&state, foundPayload("gen-2", "1", "1")) == KWin::PresentationApply::Accepted);
    CHECK(state.gen == QStringLiteral("gen-2"));
    CHECK(state.rev == 1);
    CHECK(state.obs == 1);
    CHECK(state.underlayFound);
}

void generationStreamsResetThroughTheSlotClear()
{
    // Finding G: the rotation path resets the stream with the
    // stream-resetting clear before any new-stream paint. A late
    // same-generation retransmit never resurrects, and a fresh stream
    // starts clean.
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    KWin::clearPresentationPaint(&state);
    CHECK(!state.fence.set);
    // The reset wipes order state but not source truth: re-seed from
    // effect authority (as the production endpoint dispatch does) before
    // the fresh stream paints.
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-2", "1", "1")) == KWin::PresentationApply::Accepted);
    CHECK(state.gen == QStringLiteral("gen-2"));
    CHECK(state.fence.gen == QStringLiteral("gen-2"));
}

void lostClearRecoveryAcceptsTheFreshStream()
{
    // Finding G edge case (actual ordered-channel semantics): the rotation
    // clear and the new-stream seed travel the same ordered D-Bus channel,
    // so a late old generation can never arrive after an accepted new one
    // under subscriber-side generation fencing. The one mismatch that can
    // arrive is a fresh stream whose clear was lost while the endpoint was
    // down: only accepting recovers (rejecting would wedge the fresh
    // stream forever, with no later clear coming).
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    CHECK(applyStr(&state, foundPayload("gen-2", "1", "1")) == KWin::PresentationApply::Accepted);
    CHECK(state.gen == QStringLiteral("gen-2"));
    CHECK(state.fence.gen == QStringLiteral("gen-2"));
    CHECK(state.has);
}

void displayHideRetainsFenceSoStaleCannotResurrect()
{
    // Finding G: a display-only clear (focus activation, malformed
    // payload, geometry hide) retains the ordering token, so an older
    // retransmit crossing the invalidation still cannot resurrect.
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    KWin::clearPaintDisplay(&state);
    CHECK(!state.has);
    CHECK(state.fence.set);
    CHECK(state.fence.rev == 5);
    // Older content is refused after the hide.
    CHECK(applyStr(&state, foundPayload("gen-1", "4", "9")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!state.has);
    // Equal content re-applies idempotently (same accepted projection).
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    CHECK(state.has);
    // The stream-resetting clear drops the fence: the next paint of any
    // stream is accepted as new (source truth re-seeds at dispatch).
    KWin::clearPresentationPaint(&state);
    CHECK(!state.fence.set);
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "4", "9")) == KWin::PresentationApply::Accepted);
    CHECK(state.has);
    CHECK(state.rev == 4);
}

void geometryDirtHidesUntilCoveringPaint()
{
    // Finding F (causal witness form): a frame change on the border target
    // or any held member bumps the native invalidation witness and floors
    // the affected shape at the pre-bump value. The shape stays hidden
    // until a paint covers strictly past the dirt point in the same
    // instance; equal-or-older coverage never resurrects it, and a move
    // that returns to its start (A->B->A) stays hidden without a fresh
    // covering paint. No live-frame comparison anywhere.
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    CHECK(KWin::presentationBorderFresh(&state));
    CHECK(KWin::presentationUnderlayFresh(&state));
    // Production dirt order: the tracked frame moves (border floor at 5,
    // latest 6), then a held member frame moves (underlay floor at 6,
    // latest 7).
    KWin::markBorderGeometryDirty(&state, QStringLiteral("effect-1"), 6);
    KWin::markUnderlayGeometryDirty(&state, QStringLiteral("effect-1"), 7);
    CHECK(!KWin::presentationBorderFresh(&state));
    CHECK(!KWin::presentationUnderlayFresh(&state));
    // Equal redelivery from before the move is known-stale coverage: it
    // never applies, let alone clears the floors.
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!KWin::presentationBorderFresh(&state));
    CHECK(!KWin::presentationUnderlayFresh(&state));
    // A newer (rev, obs) paint that still covers only the pre-move value
    // is stale coverage too: the in-flight P6 captured before the move
    // never paints over the dirt.
    CHECK(applyStr(&state, foundPayload("gen-1", "6", "6")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!KWin::presentationBorderFresh(&state));
    CHECK(!KWin::presentationUnderlayFresh(&state));
    // The post-move covering paint clears both floors at once.
    CHECK(applyStr(&state, foundPayload("gen-1", "7", "7", "7")) == KWin::PresentationApply::Accepted);
    CHECK(KWin::presentationBorderFresh(&state));
    CHECK(KWin::presentationUnderlayFresh(&state));
    // Re-dirt, then a same-coverage round trip that returns to its start:
    // still hidden until strictly covering content arrives.
    KWin::markBorderGeometryDirty(&state, QStringLiteral("effect-1"), 8);
    KWin::markUnderlayGeometryDirty(&state, QStringLiteral("effect-1"), 9);
    CHECK(applyStr(&state, foundPayload("gen-1", "7", "7", "7")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!KWin::presentationBorderFresh(&state));
    CHECK(!KWin::presentationUnderlayFresh(&state));
    // A paint covering between the two dirt points is still older than the
    // latest witness: refused as stale coverage, never applied.
    CHECK(applyStr(&state, foundPayload("gen-1", "8", "8", "8")) == KWin::PresentationApply::StaleIgnored);
    // Covering the latest clears both floors at once.
    CHECK(applyStr(&state, foundPayload("gen-1", "9", "9", "9")) == KWin::PresentationApply::Accepted);
    CHECK(KWin::presentationBorderFresh(&state));
    CHECK(KWin::presentationUnderlayFresh(&state));
}

void previewLatchClearsOnGestureBoundaries()
{
    // Finding D: the preview latch belongs to exactly one live gesture.
    // Drag start and finish drop it immediately, independent of the
    // ordering fence (which the subscriber separately maintains).
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "7"), "gesture-1", "1") == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    KWin::clearPaintPreview(&state);
    CHECK(!state.previewHas);
    CHECK(state.previewRect == QRect());
    // The fence and display survive the latch clear.
    CHECK(state.has);
    CHECK(state.fence.set);
    CHECK(state.fence.rev == 7);
    // A late same-generation paint re-arms the latch only through the
    // normal order gate (older still refused).
    CHECK(applyStr(&state, previewPayload("gen-1", "6", "9"), "gesture-1", "1") == KWin::PresentationApply::StaleIgnored);
    CHECK(!state.previewHas);
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "8"), "gesture-1", "1") == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
}

void outOfOrderCallbacksNeverResurrectCleared()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    KWin::clearPaintDisplay(&state);
    CHECK(!state.has);
    // A late older callback after the clear must not resurrect: the
    // retained fence refuses it.
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "4")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!state.has);
    CHECK(applyStr(&state, foundPayload("gen-1", "6", "6")) == KWin::PresentationApply::Accepted);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "9")) == KWin::PresentationApply::StaleIgnored);
    CHECK(state.rev == 6);
}

void malformedShapesFailClosedToDisplayClear()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "4", "4")) == KWin::PresentationApply::Accepted);
    const char *cases[] = {
        "not-json",
        "",
        "{\"v\":1}",
        // Unknown keys refuse (no one-shot, write, or result lanes).
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null,\"result\":null}",
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null,\"transfers\":[]}",
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null,\"warpCursor\":null}",
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null,\"frame_op\":\"x\"}",
        // Missing obs refuses (the order key is incomplete).
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}",
        // Schema version and revision gates.
        "{\"v\":2,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}",
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"04\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}",
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"-1\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}",
        nullptr,
    };
    for (size_t i = 0; cases[i] != nullptr; ++i) {
        CHECK(applyStr(&state, cases[i]) == KWin::PresentationApply::Malformed);
        CHECK(!state.has);
        // The fence survives a malformed payload: the stream continues and
        // a stale retransmit still cannot resurrect.
        CHECK(state.fence.set);
        CHECK(state.fence.rev == 4);
        // The next good read repaints (no wedged clear).
        CHECK(applyStr(&state, foundPayload("gen-1", "4", "4")) == KWin::PresentationApply::Accepted);
        CHECK(state.has);
    }
}

void intValuesRejectHugeFractionalAndOutOfRange()
{
    // Finding B: value-based integer check. Qt parses every JSON number
    // as double, so lexical spelling is unobservable; what matters is
    // finite, integral, and in int range, with the bounds/finiteness check
    // strictly before the cast (casting 1e30 to int first is UB).
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    // Huge finite doubles reject (would overflow int).
    CHECK(applyStr(&state,
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[1e30,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}")
        == KWin::PresentationApply::Malformed);
    CHECK(!state.has);
    CHECK(applyStr(&state,
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[-1e30,8,304,304]},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}")
        == KWin::PresentationApply::Malformed);
    // Fractions reject even when in bound.
    CHECK(applyStr(&state,
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10.5,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}")
        == KWin::PresentationApply::Malformed);
    // Integral-valued exponents are integral values: 1e3 is the value
    // 1000, in bound, so it paints exactly that rect.
    {
        KWin::PresentationPaint exp;
        seedLive(&exp, "effect-1", 5);
        CHECK(applyStr(&exp,
            "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\","
            "\"coveredInstance\":\"effect-1\",\"coveredInvalid\":\"5\",\"foreground\":\"win-2\","
            "\"border\":{\"visible\":1,\"inner\":[1e3,10,300,300],\"outer\":[8,8,1312,304]},"
            "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}")
            == KWin::PresentationApply::Accepted);
        CHECK(exp.borderInner == QRect(1000, 10, 300, 300));
    }
    // Out-of-bound geometry refuses.
    CHECK(applyStr(&state,
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"found\",\"win\":\"win-2\",\"outer\":[0,0,99999,800],"
        "\"members\":[\"win-2\"]},\"preview\":null}")
        == KWin::PresentationApply::Malformed);
    // Empty/duplicate member lists refuse (no anchor, no paint).
    CHECK(applyStr(&state,
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"found\",\"win\":\"win-2\",\"outer\":[0,0,100,100],"
        "\"members\":[]},\"preview\":null}")
        == KWin::PresentationApply::Malformed);
    CHECK(applyStr(&state,
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"found\",\"win\":\"win-2\",\"outer\":[0,0,100,100],"
        "\"members\":[\"win-2\",\"win-2\"]},\"preview\":null}")
        == KWin::PresentationApply::Malformed);
    // Preview with a missing subject identity refuses.
    CHECK(applyStr(&state,
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"hidden\"},"
        "\"preview\":{\"rect\":[0,0,10,10],\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\"win\":\"\"}}")
        == KWin::PresentationApply::Malformed);
    // Visible border without complete geometry refuses.
    CHECK(applyStr(&state,
        "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":null,\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}")
        == KWin::PresentationApply::Malformed);
    CHECK(applyStr(&state, foundPayload("gen-1", "4", "4")) == KWin::PresentationApply::Accepted);
    CHECK(state.has);
}

void borderStrokeReproducesPublishedOuterExactly()
{
    // Finding C: the native OutlinedBorderItem draws geometry from
    // inflate(innerRect) (pinned v6.7.4 borderoutline.h), i.e. every side
    // expanded by exactly the outline thickness. The core publishes
    // outer = inner expanded by the capture-time width on every side, so
    // the half-difference recovers that exact width without consulting
    // live config. Drawing with the derived stroke reproduces the
    // published outer rect exactly (proves the effective drawn bounds).
    const auto stroke = KWin::borderStrokeFromPublished(QRect(10, 10, 300, 300), QRect(8, 8, 304, 304));
    CHECK(stroke.has_value());
    CHECK(*stroke == 2.0);
    const QRect inner(10, 10, 300, 300);
    const QRect drawn(inner.x() - static_cast<int>(*stroke), inner.y() - static_cast<int>(*stroke),
        inner.width() + 2 * static_cast<int>(*stroke), inner.height() + 2 * static_cast<int>(*stroke));
    CHECK(drawn == QRect(8, 8, 304, 304));
    // Degenerate and non-uniform pairs fail closed (hidden, never
    // approximated): zero/negative insets, mismatched sides, empty rects.
    CHECK(!KWin::borderStrokeFromPublished(QRect(10, 10, 300, 300), QRect(10, 10, 300, 300)).has_value());
    CHECK(!KWin::borderStrokeFromPublished(QRect(8, 8, 304, 304), QRect(10, 10, 300, 300)).has_value());
    CHECK(!KWin::borderStrokeFromPublished(QRect(10, 10, 300, 300), QRect(8, 8, 306, 304)).has_value());
    CHECK(!KWin::borderStrokeFromPublished(QRect(10, 10, 300, 300), QRect(8, 10, 304, 304)).has_value());
    CHECK(!KWin::borderStrokeFromPublished(QRect(), QRect(8, 8, 304, 304)).has_value());
    CHECK(!KWin::borderStrokeFromPublished(QRect(10, 10, 0, 300), QRect(8, 8, 304, 304)).has_value());
    // Capture-vs-live config drift is ignored by construction: the helper
    // takes no config input, only the published pair.
    const auto drifted = KWin::borderStrokeFromPublished(QRect(0, 0, 100, 100), QRect(-5, -5, 110, 110));
    CHECK(drifted.has_value());
    CHECK(*drifted == 5.0);
}

void noStylingInputsInPayload()
{
    // Native styling independence: the renderer payload carries geometry
    // plus order fencing only. Those tokens must not appear anywhere in
    // the consumer header (which defines the whole accepted vocabulary).
    const std::string payload = foundPayload("gen-1", "4", "4");
    CHECK(payload.find("color") == std::string::npos);
    CHECK(payload.find("Color") == std::string::npos);
    CHECK(payload.find("width") == std::string::npos);
    CHECK(payload.find("Width") == std::string::npos);
    CHECK(payload.find("gap") == std::string::npos);
    CHECK(payload.find("Gap") == std::string::npos);
    CHECK(payload.find("theme") == std::string::npos);
    CHECK(payload.find("Theme") == std::string::npos);
    CHECK(payload.find("radius") == std::string::npos);
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, payload) == KWin::PresentationApply::Accepted);
}

void nullPreviewWithOpenGestureClearsLatchOnly()
{
    // Must-cover null preview: a previously shown preview followed by an
    // accepted null preview at a strictly newer order key drops the latch
    // while the rest of the paint rides intact (underlay found, border
    // fresh, fence advanced). The live-gesture show gate itself is
    // effect-side liveness and out of scope for this Qt-only consumer;
    // this pins the paint half.
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "7"), "gesture-1", "1") == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    CHECK(applyStr(&state, foundPayload("gen-1", "8", "8")) == KWin::PresentationApply::Accepted);
    CHECK(!state.previewHas);
    CHECK(state.previewRect == QRect());
    CHECK(state.previewSourceOut.isEmpty());
    CHECK(state.previewSourceWs.isEmpty());
    CHECK(state.previewWin.isEmpty());
    CHECK(state.has);
    CHECK(state.underlayFound);
    CHECK(state.borderVisible);
    CHECK(state.fence.set);
    CHECK(state.fence.rev == 8);
    CHECK(state.fence.obs == 8);
    CHECK(KWin::presentationBorderFresh(&state));
    CHECK(KWin::presentationUnderlayFresh(&state));
}

void nullStateIsUsageError()
{
    CHECK(KWin::applyPresentationPaint(nullptr, QStringLiteral("{}"), QString(), QStringLiteral("0"))
        == KWin::PresentationApply::Malformed);
    KWin::clearPaintDisplay(nullptr);
    KWin::clearPresentationPaint(nullptr);
    KWin::clearPaintPreview(nullptr);
    CHECK(!KWin::presentationBorderFresh(nullptr));
    CHECK(!KWin::presentationUnderlayFresh(nullptr));
    CHECK(!KWin::previewProducerMatchesLive(nullptr, QStringLiteral("gesture-1"), QStringLiteral("1")));
}

// Stage 5 causal-freshness consumer contracts over the production
// ordering/witness/floor/latch/clear logic in this header (the exact code
// the effect's SetPresentation endpoint dispatches through
// `applyPresentationPaint`, the exact bump the effect's frame/member
// signals and drag start/finish paths call, and the exact activation-hide
// route the effect's windowActivated handler invokes). They pin the
// approved causal semantics: floors key on the covered witness, previews
// need a producing identity matching the live open gesture, and
// display-only hides retain the fence. The render-hiding interleavings on
// real production event routing (signals/captures/reads/renders) are
// proven at the entry in kwin/tests/world-entry-freshness.test.ts.
// Stage 5: geometry dirt versus in-flight newer snapshot (production
// header routing: applyPresentationPaint + markBorder/UnderlayGeometryDirty
// with the effect's causal counter, the exact functions the effect's
// SetPresentation endpoint and frame/member-changed signals drive).
void stage5GeometryDirtRequiresCoveredWitness()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    // Production dirt order after the paint: the tracked frame moves, then
    // a held member frame moves. Both floors key on the pre-bump witness.
    KWin::markBorderGeometryDirty(&state, QStringLiteral("effect-1"), 6);
    KWin::markUnderlayGeometryDirty(&state, QStringLiteral("effect-1"), 7);
    // The held pre-move read completes with P6 (newer rev/obs, captured
    // before the move) before the post-move capture is accepted: known-
    // stale coverage never applies, let alone shows.
    CHECK(applyStr(&state, foundPayload("gen-1", "6", "6")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!KWin::presentationBorderFresh(&state));
    CHECK(!KWin::presentationUnderlayFresh(&state));
    // The post-move covering paint clears and shows.
    CHECK(applyStr(&state, foundPayload("gen-1", "7", "7", "7")) == KWin::PresentationApply::Accepted);
    CHECK(KWin::presentationBorderFresh(&state));
    CHECK(KWin::presentationUnderlayFresh(&state));
}

// Stage 5: preview publication versus gesture boundary (production
// header routing: applyPresentationPaint with the endpoint's exact live
// open identity, plus clearPaintPreview, the exact functions the effect's
// SetPresentation endpoint and drag start/finish paths call).
void stage5PreviewRequiresProducingIdentity()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    // Gesture A is live: its producing preview paints while A is open.
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "7"), "gesture-1", "1")
        == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    // Production boundary: A finishes and B starts. The latch drops first
    // (exact start path), so no previous-gesture paint reseeds the new one.
    KWin::clearPaintPreview(&state);
    CHECK(!state.previewHas);
    // Late already-sent A paint arrives after B started (newer obs): the
    // producer no longer matches the live open gesture, so the latch never
    // re-arms even though the order key is newer.
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "8"), "gesture-1", "2")
        == KWin::PresentationApply::Accepted);
    CHECK(!state.previewHas);
    CHECK(KWin::previewProducerMatchesLive(&state, QStringLiteral("gesture-1"), QStringLiteral("2")) == false);
    CHECK(KWin::previewProducerMatchesLive(&state, QStringLiteral("gesture-1"), QStringLiteral("1")) == false);
    // B's own producing preview paints while B is live. The production
    // start path bumps the native witness with the latch clear; the
    // covering paint names exactly that witness.
    KWin::noteNativeInvalidation(&state, QStringLiteral("effect-1"), 7, KWin::InvalidationScope::None);
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "9", "7", "gesture-1", "2"), "gesture-1", "2")
        == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    CHECK(state.previewProducerInstance == QByteArray("gesture-1"));
    CHECK(state.previewProducerSerial == QByteArray("2"));
    CHECK(KWin::previewProducerMatchesLive(&state, QStringLiteral("gesture-1"), QStringLiteral("2")));
    // An unproven preview (no producing identity) never arms the latch,
    // even with a live gesture and newer order key: fail closed.
    CHECK(applyStr(&state, unprovenPreviewPayload("gen-1", "7", "10", "7"), "gesture-1", "2")
        == KWin::PresentationApply::Accepted);
    CHECK(!state.previewHas);
}

// Stage 5: focus clear versus in-transit paint. This drives the routing
// contract the production activation path implements (display-only hide
// plus the causal bump, as the effect's windowActivated handler routes
// through the production router) in production call order with the
// endpoint parse. It pins that a display-only hide retains the fence and
// witness evidence instead of resetting the stream, so the in-transit old
// paint delivered after the clear cannot resurrect, and that only the
// explicitly replaced stream (clearPresentationPaint) resets them.
//
// Native harness scope: this drives the header functions in production
// call order but not the real effect event wiring (no offline
// ActiveWindowBorderEffect construction exists). The activation
// interleavings on real production event routing (activation signal,
// in-transit reads, malformed error-hide, SetPresentation renders) are
// proven at the entry layer in kwin/tests/world-entry-freshness.test.ts;
// a native end-to-end pin through the effect awaits a fixture-scoped live
// leg. The metadata validator additionally requires the implementation to
// delegate activation to the router, so a regression to the
// stream-resetting clear on the activation path fails the native gate.
void stage5FocusClearPreservesFence()
{
    KWin::PresentationPaint state;
    seedLive(&state, "effect-1", 5);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    // Production activation today: display-only hide plus the causal bump
    // (latest 5 -> 6), never the stream-resetting clear.
    KWin::hideForActivation(&state, QStringLiteral("effect-1"), 6);
    CHECK(!state.has);
    // The ordering token survives the hide: the fence still names (5, 5)
    // and the latest names the post-activation witness.
    CHECK(state.fence.set);
    CHECK(state.fence.rev == 5);
    CHECK(state.latest.set);
    CHECK(state.latest.instance == QStringLiteral("effect-1"));
    CHECK(state.latest.counter == 6);
    // In-transit old A paint delivered after the clear, before return-to-A:
    // same-instance older coverage never applies, even with an equal
    // (rev, obs) key.
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!state.has);
    // A late same-generation paint with an equal (rev, obs) but no causal
    // coverage stays out too: the fence was preserved, not reset.
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!state.has);
    // Return-to-A: the fresh covering paint applies and shows.
    CHECK(applyStr(&state, foundPayload("gen-1", "8", "8", "6")) == KWin::PresentationApply::Accepted);
    CHECK(state.has);
}

// Finding 1: member-frame dirt must advance the causal counter even when
// no paint is held (hidden/unpainted). The old handler gated on held
// paint plus member list, so error-hide cleared the evidence and a held
// pre-move paint (coverage equal to latest) resurrected stale underlay.
// Pre-first-paint member changes never counted at all.
void routerMemberDirtCountsWhileHidden()
{
    KWin::PresentationRouter router;
    router.setSourceInstance(QStringLiteral("effect-1"));
    for (int i = 0; i < 5; ++i) {
        router.onMemberFrame(QStringLiteral("effect-1"));
    }
    CHECK(router.counter == 5);
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "5", "5")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::Accepted);
    CHECK(router.paint.has);
    // Display-hide clears the visible list (subscriber error path): the
    // next member frame change must still bump, or the held paint would
    // still prove against a stale latest.
    router.onDisplayHide();
    CHECK(!router.paint.has);
    router.onMemberFrame(QStringLiteral("effect-1"));
    CHECK(router.counter == 6);
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "5", "5")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::StaleIgnored);
    CHECK(!router.paint.has);
    CHECK(!KWin::presentationCoveredCurrent(&router.paint));
    // The covering paint recovers.
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "6", "6", "6")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::Accepted);
    CHECK(router.paint.has);
    CHECK(KWin::presentationCoveredCurrent(&router.paint));
}

void routerPreFirstPaintMemberDirtCounts()
{
    // Finding 1, pre-first-paint: geometry changes in a member with no
    // held paint at all must still advance the counter.
    KWin::PresentationRouter router;
    router.setSourceInstance(QStringLiteral("effect-1"));
    router.onMemberFrame(QStringLiteral("effect-1"));
    CHECK(router.counter == 1);
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "4", "4", "0")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::StaleIgnored);
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "4", "4", "1")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::Accepted);
}

// Finding 2: the live source (effect counter plus facts instance) owns
// latest, never arriving paint. Foreign or future coverage hides even on
// a fresh stream or right after a stream reset.
void routerLatestIsSourceOwnedAcrossReset()
{
    KWin::PresentationRouter router;
    router.setSourceInstance(QStringLiteral("effect-1"));
    // Future coverage on a fresh stream: refused, latest stays source-owned.
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "4", "4", "9")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::StaleIgnored);
    CHECK(router.paint.latest.set);
    CHECK(router.paint.latest.counter == 0);
    // Foreign instance: refused, never adopted.
    std::string foreign = foundPayload("gen-1", "4", "4", "1");
    const std::string::size_type at = foreign.find("effect-1");
    CHECK(at != std::string::npos);
    foreign.replace(at, 8, "other-1");
    CHECK(router.applyEndpointPaint(QString::fromStdString(foreign), QString(), QStringLiteral("0"))
        == KWin::PresentationApply::StaleIgnored);
    // The covering paint (counter 0, the live source value) applies.
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "4", "4", "0")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::Accepted);
    // Stream reset wipes paint order but not source truth: the same
    // covering paint re-proves; future coverage still hides.
    router.onStreamReset();
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "4", "4", "0")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::Accepted);
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "4", "4", "9")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::StaleIgnored);
}

// Finding 3: every drawn surface requires held coverage equal to the
// actual latest, not just an unset floor. A bump with no floor for a
// shape (gesture None scope) still hides that shape until fresh coverage.
void routerDrawGatesRequireCoveredCurrent()
{
    KWin::PresentationRouter router;
    router.setSourceInstance(QStringLiteral("effect-1"));
    router.onTrackedFrame(QStringLiteral("effect-1"));
    CHECK(router.applyEndpointPaint(QString::fromStdString(foundPayload("gen-1", "5", "5", "1")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::Accepted);
    CHECK(KWin::presentationCoveredCurrent(&router.paint));
    CHECK(KWin::presentationBorderFresh(&router.paint));
    CHECK(KWin::presentationUnderlayFresh(&router.paint));
    // Gesture-boundary bump advances the witness with no floors: every
    // surface must hide even though no floor is set.
    router.onGestureBoundary(QStringLiteral("effect-1"));
    CHECK(!KWin::presentationCoveredCurrent(&router.paint));
    CHECK(KWin::presentationBorderFresh(&router.paint));
    CHECK(KWin::presentationUnderlayFresh(&router.paint));
    // A proven preview armed before the bump no longer proves current.
    KWin::PresentationRouter preview;
    preview.setSourceInstance(QStringLiteral("effect-1"));
    preview.onTrackedFrame(QStringLiteral("effect-1"));
    CHECK(preview.applyEndpointPaint(
        QString::fromStdString(previewPayload("gen-1", "7", "7", "1", "gesture-1", "1")),
        QStringLiteral("gesture-1"), QStringLiteral("1"))
        == KWin::PresentationApply::Accepted);
    CHECK(preview.paint.previewHas);
    preview.onTrackedFrame(QStringLiteral("effect-1"));
    CHECK(!KWin::presentationCoveredCurrent(&preview.paint));
}

// Finding 4: UINT64_MAX saturation never wraps and records no pretend
// proof; only an explicit fresh source instance recovers.
void routerExhaustionFailsClosedUntilFreshSource()
{
    KWin::PresentationRouter router;
    router.setSourceInstance(QStringLiteral("effect-1"));
    router.counter = UINT64_MAX - 1;
    router.onTrackedFrame(QStringLiteral("effect-1"));
    CHECK(router.counter == UINT64_MAX);
    CHECK(!router.exhausted);
    router.onTrackedFrame(QStringLiteral("effect-1"));
    CHECK(router.counter == UINT64_MAX);
    CHECK(router.exhausted);
    // No new proof may apply or show while exhausted.
    CHECK(router.applyEndpointPaint(
        QString::fromStdString(foundPayload("gen-1", "9", "9", "18446744073709551615")),
        QString(), QStringLiteral("0"))
        == KWin::PresentationApply::StaleIgnored);
    // An explicit fresh source instance recovers the namespace.
    router.setSourceInstance(QStringLiteral("effect-2"));
    CHECK(!router.exhausted);
    CHECK(router.counter == 0);
    std::string fresh = foundPayload("gen-1", "9", "9", "0");
    const std::string::size_type at = fresh.find("effect-1");
    CHECK(at != std::string::npos);
    fresh.replace(at, 8, "effect-2");
    CHECK(router.applyEndpointPaint(QString::fromStdString(fresh), QString(), QStringLiteral("0"))
        == KWin::PresentationApply::Accepted);
    CHECK(KWin::presentationCoveredCurrent(&router.paint));
}

// Finding 5: the counter is native facts lifecycle, not the renderer move
// latch. Every gesture boundary (start, resize finish with no move latch,
// abort/drop) advances it exactly once; pending cancellation semantics
// are untouched (no terminal fabricated here - transitions only).
void routerGestureBoundaryAlwaysCounts()
{
    KWin::PresentationRouter router;
    router.setSourceInstance(QStringLiteral("effect-1"));
    router.onGestureBoundary(QStringLiteral("effect-1"));
    CHECK(router.counter == 1);
    router.onGestureBoundary(QStringLiteral("effect-1"));
    CHECK(router.counter == 2);
    // A proven preview armed while live drops on the next boundary.
    CHECK(router.applyEndpointPaint(
        QString::fromStdString(previewPayload("gen-1", "7", "7", "2", "gesture-1", "1")),
        QStringLiteral("gesture-1"), QStringLiteral("1"))
        == KWin::PresentationApply::Accepted);
    CHECK(router.paint.previewHas);
    router.onGestureBoundary(QStringLiteral("effect-1"));
    CHECK(router.counter == 3);
    CHECK(!router.paint.previewHas);
    CHECK(KWin::previewProducerMatchesLive(&router.paint, QStringLiteral("gesture-1"), QStringLiteral("1")) == false);
}


} // namespace

int main(int argc, char **argv)
{
    // Offline hard gate: isolate from the live session bus before any Qt
    // D-Bus initialization.
    qputenv("DBUS_SESSION_BUS_ADDRESS", QByteArray("unix:path=/dev/null/omnitiler-presentation-test-isolated-bus"));
    QCoreApplication app(argc, argv);

    validFoundAppliesCompleteShapes();
    previewPublishesTrimmedGeometryOnly();
    newerRevisionReplacesOlderIgnored();
    generationRotationAlwaysReplaces();
    generationStreamsResetThroughTheSlotClear();
    lostClearRecoveryAcceptsTheFreshStream();
    displayHideRetainsFenceSoStaleCannotResurrect();
    geometryDirtHidesUntilCoveringPaint();
    previewLatchClearsOnGestureBoundaries();
    nullPreviewWithOpenGestureClearsLatchOnly();
    outOfOrderCallbacksNeverResurrectCleared();
    malformedShapesFailClosedToDisplayClear();
    intValuesRejectHugeFractionalAndOutOfRange();
    borderStrokeReproducesPublishedOuterExactly();
    noStylingInputsInPayload();
    nullStateIsUsageError();
    stage5GeometryDirtRequiresCoveredWitness();
    stage5PreviewRequiresProducingIdentity();
    stage5FocusClearPreservesFence();
    routerMemberDirtCountsWhileHidden();
    routerPreFirstPaintMemberDirtCounts();
    routerLatestIsSourceOwnedAcrossReset();
    routerDrawGatesRequireCoveredCurrent();
    routerExhaustionFailsClosedUntilFreshSource();
    routerGestureBoundaryAlwaysCounts();

    if (failures != 0) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("all checks passed\n");
    return EXIT_SUCCESS;
}
