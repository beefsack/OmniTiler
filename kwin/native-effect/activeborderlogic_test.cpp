// Versioned presentation-facts helpers offline test (Qt only, no KWin
// scene, no session bus, no Rust staticlib): the Stage 3 ingress wire
// shape the KWin script route parses. Geometry/eligibility derivation is
// deleted in Stage 5; the renderer contract is pinned by
// presentationconsumer_test.cpp instead.

#include "activeborderlogic.h"

#include <QString>

#include <cstdint>
#include <cstdio>
#include <cstdlib>

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

void presentationFactsJsonIsVersionedAndComplete()
{
    // Exact wire shape the KWin script route parses (v:1, opaque instance
    // and rev, causal invalidation witness, 0/1 chord lanes,
    // full-precision doubles, changed lane).
    KWin::PresentationFactsSnapshot facts;
    facts.instance = QStringLiteral("instance-1");
    facts.rev = 7;
    facts.invalidation = 9;
    facts.chord = true;
    facts.chordKnown = true;
    facts.gap = 0.0;
    facts.width = 3.5;
    facts.extension = -1.0;
    facts.changed = true;
    CHECK(KWin::presentationFactsJson(facts) == QStringLiteral("{\"v\":1,\"instance\":\"instance-1\",\"rev\":\"7\",\"invalidation\":\"9\",\"chord\":1,\"chordKnown\":1,\"gap\":0,\"width\":3.5,\"extension\":-1,\"changed\":1,\"superseded\":0,\"available\":1}"));
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

void presentationBorderColorPrefersValidNontransparentThemeOnlyWhenRequested()
{
    // Exact predicate the native ring artwork calls: a valid nontransparent
    // theme color wins only when theming is requested, including barely
    // visible (alpha 1) tints. Disabled, invalid, and fully transparent
    // themes fall back verbatim; an invalid fallback rides through (the
    // caller owns it), never a guessed color.
    const QColor theme(42, 130, 218, 255);
    const QColor fallback(255, 0, 0, 255);
    CHECK(KWin::presentationBorderColor(theme, fallback, true) == theme);
    CHECK(KWin::presentationBorderColor(theme, fallback, false) == fallback);
    CHECK(KWin::presentationBorderColor(QColor(), fallback, true) == fallback);
    CHECK(KWin::presentationBorderColor(QColor(), fallback, false) == fallback);
    const QColor faint(42, 130, 218, 1);
    CHECK(faint.isValid());
    CHECK(KWin::presentationBorderColor(faint, fallback, true) == faint);
    const QColor transparent(42, 130, 218, 0);
    CHECK(transparent.isValid());
    CHECK(KWin::presentationBorderColor(transparent, fallback, true) == fallback);
    CHECK(!KWin::presentationBorderColor(theme, QColor(), false).isValid());
}

} // namespace

int main()
{
    presentationFactsJsonIsVersionedAndComplete();
    presentationFactsStaleDetectsInstanceAndRevision();
    presentationFactsLeaseIsBounded();
    presentationBorderColorPrefersValidNontransparentThemeOnlyWhenRequested();

    if (failures != 0) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("all checks passed\n");
    return EXIT_SUCCESS;
}
