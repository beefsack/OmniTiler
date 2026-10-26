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

std::string foundPayload(const std::string &gen, const std::string &rev, const std::string &obs = "4")
{
    return "{\"v\":1,\"gen\":\"" + gen + "\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
        "\"underlay\":{\"status\":\"found\",\"win\":\"win-2\",\"outer\":[0,0,1200,800],"
        "\"members\":[\"win-1\",\"win-2\"]},\"preview\":null}";
}

std::string previewPayload(const std::string &gen, const std::string &rev, const std::string &obs = "7")
{
    return "{\"v\":1,\"gen\":\"" + gen + "\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"foreground\":\"win-2\","
        "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
        "\"underlay\":{\"status\":\"hidden\"},"
        "\"preview\":{\"rect\":[0,0,396,600],\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\"win\":\"win-a\"}}";
}

KWin::PresentationApply applyStr(KWin::PresentationPaint *state, const std::string &payload)
{
    return KWin::applyPresentationPaint(state, QString::fromStdString(payload));
}

void validFoundAppliesCompleteShapes()
{
    KWin::PresentationPaint state;
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
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "7")) == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    CHECK(state.previewRect == QRect(0, 0, 396, 600));
    CHECK(state.previewSourceOut == QByteArray("out-1"));
    CHECK(state.previewSourceWs == QByteArray("ws-1"));
    CHECK(state.previewWin == QByteArray("win-a"));
    // The insertion neighbor is not the mover: the payload carries no
    // mover identity, hover chain, revision tag, or domain resolution.
    CHECK(state.foreground == QByteArray("win-2"));
    // A null preview clears the drop target without touching the rest.
    CHECK(applyStr(&state, foundPayload("gen-1", "8", "8")) == KWin::PresentationApply::Accepted);
    CHECK(!state.previewHas);
}

void newerRevisionReplacesOlderIgnored()
{
    KWin::PresentationPaint state;
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
    CHECK(applyStr(&state, previewPayload("gen-1", "10", "11")) == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    CHECK(state.rev == 10);
    CHECK(state.obs == 11);
    // Newer revision replaces.
    CHECK(applyStr(&state, previewPayload("gen-1", "11", "11")) == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
    CHECK(state.rev == 11);
}

void generationRotationAlwaysReplaces()
{
    KWin::PresentationPaint state;
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
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    KWin::clearPresentationPaint(&state);
    CHECK(!state.fence.set);
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
    // stream is accepted as new.
    KWin::clearPresentationPaint(&state);
    CHECK(!state.fence.set);
    CHECK(applyStr(&state, foundPayload("gen-1", "4", "9")) == KWin::PresentationApply::Accepted);
    CHECK(state.has);
    CHECK(state.rev == 4);
}

void geometryDirtHidesUntilStrictlyNewerPaint()
{
    // Finding F: a frame change on the border target or any held member
    // hides the affected shape until a strictly newer paint arrives. No
    // live-frame comparison: a move that returns to its start (A->B->A)
    // stays hidden without a fresh paint.
    KWin::PresentationPaint state;
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    KWin::markBorderGeometryDirty(&state);
    KWin::markUnderlayGeometryDirty(&state);
    CHECK(!KWin::presentationBorderFresh(&state));
    CHECK(!KWin::presentationUnderlayFresh(&state));
    // Equal redelivery does not clear the floors: no resurrection.
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "5")) == KWin::PresentationApply::Accepted);
    CHECK(!KWin::presentationBorderFresh(&state));
    CHECK(!KWin::presentationUnderlayFresh(&state));
    // Same revision with a newer observation clears: obs orders within
    // the state revision.
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "6")) == KWin::PresentationApply::Accepted);
    CHECK(KWin::presentationBorderFresh(&state));
    CHECK(KWin::presentationUnderlayFresh(&state));
    // Re-dirt, then a same-rev round trip that returns to its start:
    // still hidden until strictly newer content arrives.
    KWin::markBorderGeometryDirty(&state);
    KWin::markUnderlayGeometryDirty(&state);
    CHECK(applyStr(&state, foundPayload("gen-1", "5", "6")) == KWin::PresentationApply::Accepted);
    CHECK(!KWin::presentationBorderFresh(&state));
    CHECK(!KWin::presentationUnderlayFresh(&state));
    // Newer revision clears both floors at once.
    CHECK(applyStr(&state, foundPayload("gen-1", "6", "6")) == KWin::PresentationApply::Accepted);
    CHECK(KWin::presentationBorderFresh(&state));
    CHECK(KWin::presentationUnderlayFresh(&state));
}

void previewLatchClearsOnGestureBoundaries()
{
    // Finding D: the preview latch belongs to exactly one live gesture.
    // Drag start and finish drop it immediately, independent of the
    // ordering fence (which the subscriber separately maintains).
    KWin::PresentationPaint state;
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "7")) == KWin::PresentationApply::Accepted);
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
    CHECK(applyStr(&state, previewPayload("gen-1", "6", "9")) == KWin::PresentationApply::StaleIgnored);
    CHECK(!state.previewHas);
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "8")) == KWin::PresentationApply::Accepted);
    CHECK(state.previewHas);
}

void outOfOrderCallbacksNeverResurrectCleared()
{
    KWin::PresentationPaint state;
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
        CHECK(applyStr(&exp,
            "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"4\",\"obs\":\"4\",\"foreground\":\"win-2\","
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
    CHECK(applyStr(&state, previewPayload("gen-1", "7", "7")) == KWin::PresentationApply::Accepted);
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
    CHECK(KWin::applyPresentationPaint(nullptr, QStringLiteral("{}")) == KWin::PresentationApply::Malformed);
    KWin::clearPaintDisplay(nullptr);
    KWin::clearPresentationPaint(nullptr);
    KWin::clearPaintPreview(nullptr);
    CHECK(!KWin::presentationBorderFresh(nullptr));
    CHECK(!KWin::presentationUnderlayFresh(nullptr));
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
    geometryDirtHidesUntilStrictlyNewerPaint();
    previewLatchClearsOnGestureBoundaries();
    nullPreviewWithOpenGestureClearsLatchOnly();
    outOfOrderCallbacksNeverResurrectCleared();
    malformedShapesFailClosedToDisplayClear();
    intValuesRejectHugeFractionalAndOutOfRange();
    borderStrokeReproducesPublishedOuterExactly();
    noStylingInputsInPayload();
    nullStateIsUsageError();

    if (failures != 0) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("all checks passed\n");
    return EXIT_SUCCESS;
}
