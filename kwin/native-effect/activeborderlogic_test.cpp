#include "activeborderlogic.h"
#include "oraclepress.h"

#include <QColor>
#include <QRectF>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

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

void eligibleWindowUsesFrameGeometryAsInnerRect()
{
    const QRectF frame(10.0, 20.0, 320.0, 200.0);
    const KWin::ActiveBorderState state = KWin::activeBorderState(true, frame, false, false, false, false, false);
    CHECK(state.visible);
    CHECK(state.innerRect == frame);
}

void missingWindowIsNotVisible()
{
    const QRectF frame(0.0, 0.0, 100.0, 100.0);
    const KWin::ActiveBorderState state = KWin::activeBorderState(false, frame, false, false, false, false, false);
    CHECK(!state.visible);
    CHECK(state.innerRect == QRectF());
}

void deletedWindowIsNotVisible()
{
    const QRectF frame(0.0, 0.0, 100.0, 100.0);
    const KWin::ActiveBorderState state = KWin::activeBorderState(true, frame, true, false, false, false, false);
    CHECK(!state.visible);
}

void minimizedWindowIsNotVisible()
{
    const QRectF frame(0.0, 0.0, 100.0, 100.0);
    const KWin::ActiveBorderState state = KWin::activeBorderState(true, frame, false, true, false, false, false);
    CHECK(!state.visible);
}

void fullScreenWindowIsNotVisible()
{
    const QRectF frame(0.0, 0.0, 1920.0, 1080.0);
    const KWin::ActiveBorderState state = KWin::activeBorderState(true, frame, false, false, true, false, false);
    CHECK(!state.visible);
}

void maximizedWindowIsNotVisibleAndRestores()
{
    const QRectF frame(0.0, 0.0, 1920.0, 1080.0);
    const KWin::ActiveBorderState maximized = KWin::activeBorderState(true, frame, false, false, false, true, false);
    const KWin::ActiveBorderState restored = KWin::activeBorderState(true, frame, false, false, false, false, false);
    CHECK(!maximized.visible);
    CHECK(restored.visible);
    CHECK(restored.innerRect == frame);
}

void partialMaximizeStatesAreMaximized()
{
    CHECK(!KWin::activeBorderIsMaximized(false, false));
    CHECK(KWin::activeBorderIsMaximized(true, false));
    CHECK(KWin::activeBorderIsMaximized(false, true));
    CHECK(KWin::activeBorderIsMaximized(true, true));
}

void invalidThemeColorUsesConfiguredFallback()
{
    const QColor fallback(0x2a, 0x82, 0xda);
    CHECK(KWin::activeBorderColor(QColor(), fallback, true) == fallback);
}

void transparentThemeColorUsesConfiguredFallback()
{
    const QColor fallback(0x2a, 0x82, 0xda);
    CHECK(KWin::activeBorderColor(QColor(0, 0, 0, 0), fallback, true) == fallback);
}

void usableThemeColorWinsOverConfiguredFallback()
{
    const QColor theme(0x10, 0x20, 0x30);
    CHECK(KWin::activeBorderColor(theme, QColor(0x2a, 0x82, 0xda), true) == theme);
}

void disabledThemeOverrideAlwaysUsesConfiguredFallback()
{
    const QColor fallback(0x2a, 0x82, 0xda);
    const QColor theme(0x10, 0x20, 0x30);
    CHECK(KWin::activeBorderColor(theme, fallback, false) == fallback);
    CHECK(KWin::activeBorderColor(QColor(), fallback, false) == fallback);
    CHECK(KWin::activeBorderColor(QColor(0, 0, 0, 0), fallback, false) == fallback);
}

void zeroGapKeepsFrameAsInnerRect()
{
    const QRectF frame(10.0, 20.0, 320.0, 200.0);
    CHECK(KWin::activeBorderInnerRect(frame, 0.0) == frame);
}

void positiveGapExpandsInnerRect()
{
    const QRectF frame(10.0, 20.0, 320.0, 200.0);
    const QRectF expanded = KWin::activeBorderInnerRect(frame, 5.0);
    CHECK(expanded == frame.adjusted(-5.0, -5.0, 5.0, 5.0));
    CHECK(expanded.x() == 5.0);
    CHECK(expanded.y() == 15.0);
    CHECK(expanded.width() == 330.0);
    CHECK(expanded.height() == 210.0);
}

void gapAppliesToVisibleBorderState()
{
    const QRectF frame(0.0, 0.0, 100.0, 100.0);
    const KWin::ActiveBorderState state = KWin::activeBorderState(true, frame, false, false, false, false, false);
    CHECK(state.visible);
    CHECK(KWin::activeBorderInnerRect(state.innerRect, 2.0) == QRectF(-2.0, -2.0, 104.0, 104.0));
}

void seedMaximizedMapsCommittedMaximizeMode()
{
    // Direct-read seed: committed maximizeMode() 0 restores, any nonzero
    // axis (vertical/horizontal/full) suppresses both borders.
    CHECK(!KWin::activeBorderSeedMaximized(0));
    CHECK(KWin::activeBorderSeedMaximized(1));
    CHECK(KWin::activeBorderSeedMaximized(2));
    CHECK(KWin::activeBorderSeedMaximized(3));
}

void normalObservationStartShowsThenMaximizes()
{
    // Observation start with a normal seed shows; a subsequent native
    // maximize transition hides; the restore transition shows again.
    // Transitions stay authoritative after the seed.
    CHECK(KWin::activeBorderState(true, QRectF(0.0, 0.0, 100.0, 100.0), false, false, false,
        KWin::activeBorderSeedMaximized(0), false)
            .visible);
    CHECK(!KWin::activeBorderState(true, QRectF(0.0, 0.0, 100.0, 100.0), false, false, false, true, false).visible);
    CHECK(KWin::activeBorderState(true, QRectF(0.0, 0.0, 100.0, 100.0), false, false, false, false, false).visible);
}

void maximizedObservationStartHidesUntilRestore()
{
    // A window already maximized before effect load seeds hidden and stays
    // hidden across a repeated maximized signal until restore.
    CHECK(!KWin::activeBorderState(true, QRectF(0.0, 0.0, 1920.0, 1080.0), false, false, false,
        KWin::activeBorderSeedMaximized(3), false)
            .visible);
    CHECK(!KWin::activeBorderState(true, QRectF(0.0, 0.0, 1920.0, 1080.0), false, false, false,
        KWin::activeBorderIsMaximized(true, true), false)
            .visible);
    CHECK(KWin::activeBorderState(true, QRectF(0.0, 0.0, 1920.0, 1080.0), false, false, false,
        KWin::activeBorderIsMaximized(false, false), false)
            .visible);
}

void fullscreenObservationStartHidesRegardlessOfSeed()
{
    // Fullscreen stays suppressed independently: even a normal seed hides
    // while fullscreen, and un-fullscreen restores when normal.
    CHECK(!KWin::activeBorderState(true, QRectF(0.0, 0.0, 1920.0, 1080.0), false, false, true,
        KWin::activeBorderSeedMaximized(0), false)
            .visible);
    CHECK(!KWin::activeBorderState(true, QRectF(0.0, 0.0, 1920.0, 1080.0), false, false, true,
        KWin::activeBorderSeedMaximized(3), false)
            .visible);
    CHECK(KWin::activeBorderState(true, QRectF(0.0, 0.0, 1920.0, 1080.0), false, false, false,
        KWin::activeBorderSeedMaximized(0), false)
            .visible);
}

void appletPopupIsNotVisibleAndOrdinaryRestores()
{
    // Active applet popups never carry the border; an ordinary active window
    // with the same geometry restores it.
    const QRectF frame(0.0, 0.0, 800.0, 600.0);
    const KWin::ActiveBorderState popup = KWin::activeBorderState(true, frame, false, false, false, false, true);
    const KWin::ActiveBorderState ordinary = KWin::activeBorderState(true, frame, false, false, false, false, false);
    CHECK(!popup.visible);
    CHECK(popup.innerRect == QRectF());
    CHECK(ordinary.visible);
    CHECK(ordinary.innerRect == frame);
}

void pressResizeBindingMapsSlotsInOrder()
{
    // Options::MouseUnrestrictedResize value is opaque here; the seam takes
    // it as a plain int so no KWin headers are needed.
    constexpr int resize = 14;
    CHECK(KWin::oracleResizeButton(resize, 0, 0, resize) == Qt::LeftButton);
    CHECK(KWin::oracleResizeButton(0, resize, 0, resize) == Qt::MiddleButton);
    CHECK(KWin::oracleResizeButton(0, 0, resize, resize) == Qt::RightButton);
    CHECK(KWin::oracleResizeButton(0, 0, 0, resize) == Qt::NoButton);
    CHECK(KWin::oracleResizeButton(1, 2, 3, resize) == Qt::NoButton);
    // First slot carrying the resize command wins.
    CHECK(KWin::oracleResizeButton(resize, resize, resize, resize) == Qt::LeftButton);
}

void pressDefaultBindingIsAltRight()
{
    // Verified against Options::defaultCommandAll3() (MouseUnrestrictedResize)
    // and Options::defaultKeyCmdAllModKey() (Key_Alt): fallback only while
    // the public options are unavailable.
    CHECK(KWin::oracleDefaultResizeButton() == Qt::RightButton);
    CHECK(KWin::oracleDefaultResizeModifier() == Qt::AltModifier);
}

void pressAgeGateBoundsTwoSecondsMonotonic()
{
    CHECK(KWin::oraclePressAgeOk(0));
    CHECK(KWin::oraclePressAgeOk(2000));
    CHECK(!KWin::oraclePressAgeOk(2001));
    CHECK(!KWin::oraclePressAgeOk(-1));
    CHECK(!KWin::oraclePressAgeOk(INT64_C(1000000)));
}

void pressBindingNameIsClosedVocabulary()
{
    CHECK(std::strcmp(KWin::oraclePressBindingName(true), "configured") == 0);
    CHECK(std::strcmp(KWin::oraclePressBindingName(false), "default") == 0);
}

void dragPreviewAcceptsCarriedGeometryBounds()
{
    CHECK(KWin::dragPreviewRectValid(0, 0, 1, 1));
    CHECK(KWin::dragPreviewRectValid(0, 0, 1200, 800));
    CHECK(KWin::dragPreviewRectValid(-16384, -16384, 16384, 16384));
    CHECK(KWin::dragPreviewRectValid(16384, 16384, 1, 1));
    CHECK(!KWin::dragPreviewRectValid(0, 0, 0, 1));
    CHECK(!KWin::dragPreviewRectValid(0, 0, 1, 0));
    CHECK(!KWin::dragPreviewRectValid(0, 0, 16385, 1));
    CHECK(!KWin::dragPreviewRectValid(0, 0, 1, 16385));
    CHECK(!KWin::dragPreviewRectValid(16385, 0, 10, 10));
    CHECK(!KWin::dragPreviewRectValid(0, -16385, 10, 10));
    CHECK(!KWin::dragPreviewRectValid(-16385, 0, 10, 10));
}

void groupUnderlayOuterExpandsByGapWidthAndExtension()
{
    // Outer = union + gap + border width + extension beyond the border edge.
    const QRectF unionRect(10.0, 20.0, 100.0, 80.0);
    CHECK(KWin::groupUnderlayOuterRect(unionRect, 0.0, 3.0, 3.0) == unionRect.adjusted(-6.0, -6.0, 6.0, 6.0));
    CHECK(KWin::groupUnderlayOuterRect(unionRect, 2.0, 3.0, 3.0) == unionRect.adjusted(-8.0, -8.0, 8.0, 8.0));
}

void groupUnderlayDefaultExtensionMatchesBorderWidth()
{
    // Default sentinel -1 follows the current configured border width:
    // gap 0 plus border width plus resolved extension is twice the width.
    // Explicit 0 renders as-is (outer = union + gap + border width only).
    CHECK(KWin::groupUnderlayEffectiveExtension(-1.0, 3.0) == 3.0);
    CHECK(KWin::groupUnderlayEffectiveExtension(-1.0, 5.0) == 5.0);
    CHECK(KWin::groupUnderlayEffectiveExtension(0.0, 5.0) == 0.0);
    CHECK(KWin::groupUnderlayEffectiveExtension(7.5, 5.0) == 7.5);
    const QRectF unionRect(0.0, 0.0, 100.0, 100.0);
    const QRectF outerDefault3 = KWin::groupUnderlayOuterRect(unionRect, 0.0, 3.0, KWin::groupUnderlayEffectiveExtension(-1.0, 3.0));
    CHECK(outerDefault3 == QRectF(-6.0, -6.0, 112.0, 112.0));
    const QRectF outerDefault5 = KWin::groupUnderlayOuterRect(unionRect, 0.0, 5.0, KWin::groupUnderlayEffectiveExtension(-1.0, 5.0));
    CHECK(outerDefault5 == QRectF(-10.0, -10.0, 120.0, 120.0));
    CHECK(outerDefault5.width() == unionRect.width() + 20.0);
    const QRectF outerExplicit0 = KWin::groupUnderlayOuterRect(unionRect, 0.0, 5.0, KWin::groupUnderlayEffectiveExtension(0.0, 5.0));
    CHECK(outerExplicit0 == QRectF(-5.0, -5.0, 110.0, 110.0));
}

void presentationFactsJsonIsVersionedAndComplete()
{
    // Exact wire shape the KWin script route parses (v:1, opaque instance
    // and rev, 0/1 chord lanes, full-precision doubles, changed lane).
    KWin::PresentationFactsSnapshot facts;
    facts.instance = QStringLiteral("instance-1");
    facts.rev = 7;
    facts.chord = true;
    facts.chordKnown = true;
    facts.gap = 0.0;
    facts.width = 3.5;
    facts.extension = -1.0;
    facts.changed = true;
    CHECK(KWin::presentationFactsJson(facts) == QStringLiteral("{\"v\":1,\"instance\":\"instance-1\",\"rev\":\"7\",\"chord\":1,\"chordKnown\":1,\"gap\":0,\"width\":3.5,\"extension\":-1,\"changed\":1,\"superseded\":0,\"available\":1}"));
}

void presentationFactsStaleDetectsInstanceAndRevision()
{
    CHECK(KWin::presentationFactsStale(QStringLiteral(""), QStringLiteral(""), QStringLiteral("instance-1"), 0));
    CHECK(KWin::presentationFactsStale(QStringLiteral("instance-1"), QStringLiteral(""), QStringLiteral("instance-1"), 0));
    CHECK(KWin::presentationFactsStale(QStringLiteral("instance-1"), QStringLiteral("0"), QStringLiteral("instance-2"), 0));
    CHECK(KWin::presentationFactsStale(QStringLiteral("instance-1"), QStringLiteral("0"), QStringLiteral("instance-1"), 1));
    CHECK(!KWin::presentationFactsStale(QStringLiteral("instance-1"), QStringLiteral("1"), QStringLiteral("instance-1"), 1));
}

void presentationFactsLeaseIsBounded()
{
    CHECK(KWin::presentationFactsLeaseMs(0) == 1000);
    CHECK(KWin::presentationFactsLeaseMs(500) == 1000);
    CHECK(KWin::presentationFactsLeaseMs(20000) == 20000);
    CHECK(KWin::presentationFactsLeaseMs(60000) == 60000);
    CHECK(KWin::presentationFactsLeaseMs(120000) == 60000);
}

void groupUnderlayChordNeedsBothWinAndShift()
{
    // (win, shift) -> chord. Extras and press order live outside this
    // predicate: the native fold passes only these two level bits.
    CHECK(!KWin::groupUnderlayChordHeld(false, false));
    CHECK(!KWin::groupUnderlayChordHeld(true, false));
    CHECK(!KWin::groupUnderlayChordHeld(false, true));
    CHECK(KWin::groupUnderlayChordHeld(true, true));
}

} // namespace

int main()
{
    eligibleWindowUsesFrameGeometryAsInnerRect();
    missingWindowIsNotVisible();
    deletedWindowIsNotVisible();
    minimizedWindowIsNotVisible();
    fullScreenWindowIsNotVisible();
    maximizedWindowIsNotVisibleAndRestores();
    partialMaximizeStatesAreMaximized();
    invalidThemeColorUsesConfiguredFallback();
    transparentThemeColorUsesConfiguredFallback();
    usableThemeColorWinsOverConfiguredFallback();
    disabledThemeOverrideAlwaysUsesConfiguredFallback();
    zeroGapKeepsFrameAsInnerRect();
    positiveGapExpandsInnerRect();
    gapAppliesToVisibleBorderState();
    seedMaximizedMapsCommittedMaximizeMode();
    normalObservationStartShowsThenMaximizes();
    maximizedObservationStartHidesUntilRestore();
    fullscreenObservationStartHidesRegardlessOfSeed();
    appletPopupIsNotVisibleAndOrdinaryRestores();
    pressResizeBindingMapsSlotsInOrder();
    pressDefaultBindingIsAltRight();
    pressAgeGateBoundsTwoSecondsMonotonic();
    pressBindingNameIsClosedVocabulary();
    dragPreviewAcceptsCarriedGeometryBounds();
    groupUnderlayChordNeedsBothWinAndShift();
    groupUnderlayOuterExpandsByGapWidthAndExtension();
    groupUnderlayDefaultExtensionMatchesBorderWidth();
    presentationFactsJsonIsVersionedAndComplete();
    presentationFactsStaleDetectsInstanceAndRevision();
    presentationFactsLeaseIsBounded();

    if (failures != 0) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("all checks passed\n");
    return EXIT_SUCCESS;
}
