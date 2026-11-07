#pragma once

// Read-only presentation consumer (Stage 5): strict parse plus generation
// order fencing for the native `SetPresentation(QString)` endpoint. The
// script subscriber forwards accepted core shapes only; this module never
// derives geometry, eligibility, or policy and never touches one-shot,
// write, or result lanes.
//
// Coordinate proof (KWin logical units end to end, no second transform):
// every rect below arrives in KWin logical pixels exactly as the core
// published it (the adapter rounds fractional natives once at capture with
// TS Math.round semantics, halves toward positive infinity:
// Math.round(-10.5) === -10 and Rust f64::round is never used; style units
// convert once at ingress and are never re-scaled). The renderer consumes the
// border inner/outer, underlay outer, and preview rects verbatim: item
// anchoring translates coordinate spaces (scene <-> item local via
// mapFromScene) without a second geometric outset, DPI factor, or device
// scaling. Native code never recomputes an outer rect from the union plus
// gap/width/extension (that expansion already happened core-side).
//
// Payload contract (built by the script subscriber from the accepted broker
// projection, exact keys, no unknown fields, no trailing data):
//   {"v":2,"gen":"<opaque>","rev":"<canonical-u64>","obs":"<canonical-u64>",
//    "coveredInstance":"<opaque>","coveredInvalid":"<canonical-u64>",
//    "foreground":"<id-or-empty>",
//    "border":{"visible":<bool>,"reason":"<closed>","subject":"<id>"|null,
//              "outsets":[l,t,r,b]|null,"width":<i32>=0|null,"radius":<i32>=0|null,
//              "witness":{"win":"<id>","obs":"<canonical-u64>",
//                         "frame":[x,y,w,h],"foreground":"<id>"}|null},
//    "underlay":{"status":"found","win":"<id>","outer":[x,y,w,h],
//                "members":["<id>",...]}|{"status":"hidden"},
//    "preview":{"rect":[x,y,w,h],"sourceOut":"<id>","sourceWs":"<id>",
//               "win":"<id>","producerInstance":"<id>"|null,
//               "producerSerial":"<canonical-u64>"|null}|null}
// - `v` is exactly the integer 2 (value-based: integral finite double).
// - `rev`/`obs` are canonical decimal u64 strings (no leading zeros, no
//   sign). The `(rev, obs)` pair is the order key: different observations
//   under the same state revision can change presentation, so ordering uses
//   both, never the revision alone.
// - `coveredInstance`/`coveredInvalid` are the causal coverage witness:
//   the native-invalidation source instance plus the counter value this
//   capture covered. A paint applies only when its witness is the
//   renderer's latest (same instance); same-instance older coverage never
//   resurrects, and a foreign instance with a set latest never applies.
//   Never inferred from timing, geometry, or age.
// - `foreground` is empty (no eligible target) or an opaque window id.
// - Border value check (Qt parses every JSON number as double, so lexical
//   spelling is unobservable here): outsets/width/radius are finite,
//   integral, in i32 range (width/radius additionally >= 0 when non-null;
//   no uniform, non-negative-outset, or 1024 clamp). Witness frame uses
//   the carried-geometry bound (x/y in -16384..16384, w/h in 1..16384).
//   Reason is the closed v4 set; visible=>reason=="". Nullable
//   subject/outsets/width/radius/witness pass unchanged even when null,
//   dangling, or unjoined: native border narrowly hides while
//   underlay/preview can still paint. Mismatched version or malformed DTO
//   hides all three quietly.
// - `members` is a non-empty unique opaque-id list (the renderer anchors
//   the already-verbatim outer rect at the lowest-stacked renderable
//   member; member rects never cross). Empty/duplicate/non-id members fail
//   closed.
// - `preview.win` is the insertion neighbor, never the mover identity: the
//   producer identity (`producerInstance`/`producerSerial`, null when the
//   producing native gesture is unproven) names the actual producing native
//   gesture. The native show gate matches that producer against the live
//   open gesture, never against the mover; unproven previews never arm the
//   latch (fail closed).
// - The read correlation `req` is subscriber-side only (nonce plus
//   generation fencing at the read site); native orders paints on
//   `(gen, rev, obs)` alone, which is sufficient on the ordered channel
//   with those fences. Coverage gates alongside: a paint whose witness is
//   older than the renderer's latest (or foreign to it) never applies.
// - No styling inputs cross here: native styling stays native
//   (independent per-surface configured fills with the live system accent
//   default, never core-derived).
//
// Ordering and freshness model (all inside this Qt-only header so the
// offline consumer test pins it without KWin):
// - `fence` is the ordering token, independent of visible state: display
//   clears (focus activation, geometry dirt, malformed payloads) retain it;
//   only the stream-resetting `ClearPresentation` path (generation
//   rotation, unbind, stop, explicit stream replacement) resets it.
//   Same-generation older `(rev, obs)` never applies; a different generation
//   with a set fence applies as a new stream (rotation always precedes a new
//   stream with the stream-resetting clear on the same ordered channel, so a
//   mismatch means the clear was lost while the endpoint was down and only
//   accepting recovers; a late old generation after an accepted new one
//   cannot occur on that ordered channel under subscriber-side generation
//   fencing). Newer `(rev, obs)` applies; equal `(rev, obs)` re-applies
//   idempotently.
// - `latest` is the effect-owned causal witness (advanced by
//   noteNativeInvalidation on frames including genuine mover motion,
//   activation/focus, fullscreen transitions, visibility changes, layout
//   add/remove, and gesture boundaries). `covered` is the witness the held
//   paint covers: paints apply when covered matches latest exactly (same
//   instance, equal counter), or when covered reaches the seeded
//   `structuralFloor` in the same instance (pre-motion paint stays
//   appliable while strict shapes hide by their own gates); same-instance
//   older coverage below the floor never applies (in-transit paints
//   crossing a structural invalidation are the reachable stale case), and
//   a foreign instance with a set latest never applies (reused counters
//   across instances never compare).
// - `borderFloor`/`underlayFloor` snapshot the pre-bump latest when native
//   frame geometry changes on the border target / any held underlay member.
//   A set floor hides its shape until a paint covers strictly past it in the
//   same instance; an equal-or-older covering paint never resurrects it, and
//   geometry coincidence (a move that returns to its start) never clears the
//   floor without a fresh paint. The underlay depends on ALL held members,
//   not just the anchor.

#include <QByteArray>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QRect>
#include <QString>
#include <QStringList>

#include <climits>
#include <cmath>
#include <cstdint>
#include <optional>

namespace KWin
{

// Carried-geometry bound (core GEOMETRY_BOUND, retired group/preview gate).
inline constexpr int PRESENTATION_COORD_MIN = -16384;
inline constexpr int PRESENTATION_COORD_MAX = 16384;
inline constexpr int PRESENTATION_SIZE_MAX = 16384;

// Causal coverage witness (Stage 5): the native-invalidation source
// instance plus the counter value covered. Counters never compare across
// instances.
struct CoverWitness {
    QString instance;
    quint64 counter = 0;
    bool set = false;
};

// Scope of one native invalidation bump: which shape floors snapshot the
// pre-bump latest. Gesture boundaries advance the witness with no floors
// (the preview latch clears separately and the producer match gates it).
enum class InvalidationScope : int {
    None = 0,
    Border = 1,
    Underlay = 2,
    All = 3,
};
struct PaintOrder {
    QString gen;
    quint64 rev = 0;
    quint64 obs = 0;
    bool set = false;
};

// Strictly-newer check for one ordered stream only: opaque generations
// never compare, so a generation mismatch is never "newer" here (the
// caller fences generations separately).
inline bool paintOrderGreater(const PaintOrder &a, const PaintOrder &b)
{
    if (!a.set || !b.set || a.gen != b.gen) {
        return false;
    }
    if (a.rev != b.rev) {
        return a.rev > b.rev;
    }
    return a.obs > b.obs;
}

inline bool paintOrderLess(const PaintOrder &a, const PaintOrder &b)
{
    if (!a.set || !b.set || a.gen != b.gen) {
        return false;
    }
    if (a.rev != b.rev) {
        return a.rev < b.rev;
    }
    return a.obs < b.obs;
}

// Quiet version-mismatch peek for the SetPresentation endpoint: true when
// the payload declares an integral version other than the strict v2 the
// consumer requires. Lets the endpoint hide without the repetitive
// malformed diagnostic a stale v1 producer would otherwise emit on every
// receipt; any other shape still takes the normal malformed path.
inline bool presentationPayloadVersionMismatch(const QString &payload)
{
    if (payload.isEmpty() || payload.size() > 65536) {
        return false;
    }
    QJsonParseError error{};
    const QJsonDocument document = QJsonDocument::fromJson(payload.toUtf8(), &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) {
        return false;
    }
    const QJsonObject root = document.object();
    const QJsonValue version = root.value(QStringLiteral("v"));
    if (!version.isDouble()) {
        return false;
    }
    const double raw = version.toDouble();
    if (!std::isfinite(raw) || std::trunc(raw) != raw) {
        return false;
    }
    if (raw < static_cast<double>(INT_MIN) || raw > static_cast<double>(INT_MAX)) {
        return false;
    }
    return static_cast<int>(raw) != 2;
}

struct PresentationPaint {
    bool has = false;
    QString gen;
    quint64 rev = 0;
    quint64 obs = 0;
    QByteArray foreground;
    bool borderVisible = false;
    QString borderReason;
    QByteArray borderSubject;
    bool borderHasOutsets = false;
    int borderOutsetLeft = 0;
    int borderOutsetTop = 0;
    int borderOutsetRight = 0;
    int borderOutsetBottom = 0;
    bool borderHasWidth = false;
    int borderWidth = 0;
    bool borderHasRadius = false;
    int borderRadius = 0;
    bool borderHasWitness = false;
    QByteArray witnessWin;
    QString witnessObs;
    quint64 witnessObsValue = 0;
    QRect witnessFrame;
    QByteArray witnessForeground;
    bool underlayFound = false;
    QByteArray underlayWin;
    QRect underlayOuter;
    QStringList underlayMembers;
    bool previewHas = false;
    QRect previewRect;
    QByteArray previewSourceOut;
    QByteArray previewSourceWs;
    QByteArray previewWin;
    // Producing native gesture of the held preview (effect-instance plus
    // nonzero serial); empty when unproven. The show gate matches it
    // against the live open gesture. Set only from accepted paints.
    QByteArray previewProducerInstance;
    QByteArray previewProducerSerial;
    // Ordering token, independent of visible state (finding G): retained
    // across display-only hides, reset only by the stream-resetting clear.
    PaintOrder fence;
    // Causal witness: the renderer's latest native invalidation plus the
    // witness the held paint covers. A paint applies only when covered
    // matches latest exactly (same instance), or when it reaches the
    // structural floor below (same instance, covered at or past the
    // floor): motion advances the monotonic witness without touching the
    // floor, so a newer-order pre-motion paint is accepted on the
    // structural path while strict shapes stay hidden by their own gates
    // until exactly covering paint arrives. Older or foreign coverage
    // never applies.
    CoverWitness latest;
    CoverWitness covered;
    // Structural floor copy, re-seeded from effect authority at every
    // endpoint dispatch (never established by arriving paint, like
    // latest): the monotonic counter value as of the last structural
    // invalidation in this instance. Unset outside the router-seeded route
    // (direct consumer use keeps the exact-only gate).
    CoverWitness structuralFloor;
    // Geometry-dirt floors (finding F, causal witness form): the pre-bump
    // latest snapshotted when native frame geometry changed on the
    // dependent windows. A set floor hides its shape until a paint covers
    // strictly past it in the same instance.
    CoverWitness borderFloor;
    CoverWitness underlayFloor;
};

enum class PresentationApply : int {
    // Accepted: the display state was replaced (idempotent re-apply on
    // equal `(rev, obs)` redelivery, e.g. a reconnect seed; a set
    // geometry floor still hides the affected shape until strictly newer
    // content arrives).
    Accepted = 1,
    // Ignored stale/out-of-order: the display, fence, and floors are
    // preserved, never resurrected or cleared.
    StaleIgnored = 2,
    // Parse-rejected (malformed shape, unknown keys, out-of-bound
    // geometry): the display clears, never a partial paint. The fence and
    // floors are retained: the stream continues and a stale retransmit
    // still cannot resurrect.
    Malformed = 0,
};

// A paint covers its floor when both name the same instance and the
// covered counter advances strictly past the dirt point.
inline bool coverClearsFloor(const CoverWitness &cover, const CoverWitness &floor)
{
    if (!cover.set || !floor.set) {
        return false;
    }
    if (cover.instance != floor.instance) {
        return false;
    }
    return cover.counter > floor.counter;
}

// Draw-gate coverage check: the held paint covers exactly the renderer's
// actual latest witness (same instance, equal counter). Every drawn
// surface requires this in addition to its floor: a bump with no floor
// for a shape (or none at all) still hides that shape until fresh
// covering paint arrives. Never compares reused counters across
// instances; missing proof on either side hides.
inline bool presentationCoveredCurrent(const PresentationPaint *state)
{
    if (state == nullptr || !state->has) {
        return false;
    }
    if (!state->latest.set || !state->covered.set) {
        return false;
    }
    return state->covered.instance == state->latest.instance
        && state->covered.counter == state->latest.counter;
}

// Display-freshness for the affected shape: an unset floor is fresh; a set
// floor hides until the held paint covers strictly past the dirt point in
// the same instance.
inline bool presentationBorderFresh(const PresentationPaint *state)
{
    if (state == nullptr) {
        return false;
    }
    if (!state->borderFloor.set) {
        return true;
    }
    return coverClearsFloor(state->covered, state->borderFloor);
}

inline bool presentationUnderlayFresh(const PresentationPaint *state)
{
    if (state == nullptr) {
        return false;
    }
    if (!state->underlayFloor.set) {
        return true;
    }
    return coverClearsFloor(state->covered, state->underlayFloor);
}

// Native invalidation bump (Stage 5, causal witness): advances the
// renderer's latest witness and snapshots the pre-bump value as the dirt
// floor for the affected shapes. The effect calls this on geometry,
// activation/focus, and gesture boundaries (gesture scope advances the
// witness with no floors; the preview latch clears separately and the
// producer match gates it). Never compares live frames, so same-window
// moves and A->B->A races stay hidden without a fresh paint. Never
// compares reused counters across instances: a foreign instance clears
// the floors (the caller hides the display on replacement; new-instance
// paints apply fresh below).
inline void noteNativeInvalidation(
    PresentationPaint *state, const QString &instance, quint64 counter, InvalidationScope scope)
{
    if (state == nullptr || instance.isEmpty()) {
        return;
    }
    CoverWitness prev;
    if (state->latest.set && state->latest.instance == instance) {
        prev = state->latest;
    } else {
        if (state->latest.set) {
            state->borderFloor = CoverWitness{};
            state->underlayFloor = CoverWitness{};
        }
        prev = CoverWitness{instance, 0, true};
    }
    if (scope == InvalidationScope::Border || scope == InvalidationScope::All) {
        state->borderFloor = prev;
    }
    if (scope == InvalidationScope::Underlay || scope == InvalidationScope::All) {
        state->underlayFloor = prev;
    }
    state->latest = CoverWitness{instance, counter, true};
}

// Native geometry-dirt marks (finding F, causal witness form): snapshot
// the pre-bump latest as the floor and advance the witness, so only a
// paint covering strictly past the dirt point clears it.
inline void markBorderGeometryDirty(PresentationPaint *state, const QString &instance, quint64 counter)
{
    noteNativeInvalidation(state, instance, counter, InvalidationScope::Border);
}

inline void markUnderlayGeometryDirty(PresentationPaint *state, const QString &instance, quint64 counter)
{
    noteNativeInvalidation(state, instance, counter, InvalidationScope::Underlay);
}

// Native gesture-boundary preview latch clear (finding D): the held
// drop-preview rect belongs to exactly one live gesture. Drag start and
// finish drop it immediately (liveness alone would hide it, but the latch
// must not survive into the next gesture even if no fresh paint arrives
// first). The fence is untouched: a late same-generation paint still
// fences on `(rev, obs)`.
inline void clearPaintPreview(PresentationPaint *state)
{
    if (state == nullptr) {
        return;
    }
    state->previewHas = false;
    state->previewRect = QRect();
    state->previewSourceOut.clear();
    state->previewSourceWs.clear();
    state->previewWin.clear();
    state->previewProducerInstance.clear();
    state->previewProducerSerial.clear();
}

// Live producer match for the held preview: the held producing identity
// equals the live open native identity. The effect's show gate calls this
// with the exact live values; the parse-time gate below uses the same
// values the SetPresentation endpoint received.
inline bool previewProducerMatchesLive(
    const PresentationPaint *state, const QString &liveInstance, const QString &liveSerial)
{
    if (state == nullptr || !state->previewHas) {
        return false;
    }
    if (liveInstance.isEmpty() || liveSerial.isEmpty() || liveSerial == QStringLiteral("0")) {
        return false;
    }
    return state->previewProducerInstance == liveInstance.toUtf8()
        && state->previewProducerSerial == liveSerial.toUtf8();
}

// Exact-reproduction stroke for the native OutlinedBorderItem (finding C):
// pinned KWin v6.7.4 `src/scene/borderoutline.h` inflates every side by
// exactly the outline thickness, and `src/scene/outlinedborderitem.cpp`
// sets the item geometry from `inflate(innerRect)`. The core publishes
// `outer = inner` expanded by the capture-time width on every side
// (`tiler-core/src/visual.rs::border_geometry`), so each uniform inset
// already equals that exact width: drawing with it reproduces the
// published outer rect exactly. Live config width is never consulted.
// All four insets must be equal and positive, else nullopt (render hidden,
// never approximated).
inline std::optional<double> borderStrokeFromPublished(const QRect &inner, const QRect &outer)
{
    if (inner.width() <= 0 || inner.height() <= 0 || outer.width() <= 0 || outer.height() <= 0) {
        return std::nullopt;
    }
    const int left = inner.x() - outer.x();
    const int top = inner.y() - outer.y();
    const int right = (outer.x() + outer.width()) - (inner.x() + inner.width());
    const int bottom = (outer.y() + outer.height()) - (inner.y() + inner.height());
    if (left <= 0 || top <= 0 || right <= 0 || bottom <= 0) {
        return std::nullopt;
    }
    if (left != top || top != right || right != bottom) {
        return std::nullopt;
    }
    return static_cast<double>(left);
}

// TS Math.round quantization for the live native frame (halves toward
// +infinity): std::floor(v + 0.5). Differs from std::lround on negative
// halves (Math.round(-10.5) == -10, lround gives -11). The adapter rounds
// fractional natives once at capture with this semantics; native
// re-quantizes the live frame the same way before joining to the witness.
inline int quantizeMathRound(double value)
{
    return static_cast<int>(std::floor(value + 0.5));
}

inline QRect quantizeFrameForWitness(double x, double y, double width, double height)
{
    return QRect(quantizeMathRound(x), quantizeMathRound(y), quantizeMathRound(width), quantizeMathRound(height));
}

// Live border geometry from the quantized live frame plus the held
// outsets/width verbatim (no config, no second scale): outer is the live
// frame expanded by each outset, inner is outer inset by width once.
// Returns false on width 0 or invalid metrics (narrow hide, never a parser
// rejection: the accepted shape stays for underlay/preview).
inline bool borderGeometryFromLiveFrame(
    const QRect &liveFrame, int outsetLeft, int outsetTop, int outsetRight, int outsetBottom, int width,
    QRect *outer, QRect *inner)
{
    if (liveFrame.width() <= 0 || liveFrame.height() <= 0 || width <= 0) {
        return false;
    }
    // Promoted bounds before any arithmetic: int outset/width extremes
    // (INT_MIN negation, INT_MAX sums) must fail closed instead of
    // wrapping through QRect::adjusted int math to a nonsense rect.
    const qint64 x1 = static_cast<qint64>(liveFrame.x()) - static_cast<qint64>(outsetLeft);
    const qint64 y1 = static_cast<qint64>(liveFrame.y()) - static_cast<qint64>(outsetTop);
    const qint64 x2 = static_cast<qint64>(liveFrame.x()) + static_cast<qint64>(liveFrame.width()) - 1
        + static_cast<qint64>(outsetRight);
    const qint64 y2 = static_cast<qint64>(liveFrame.y()) + static_cast<qint64>(liveFrame.height()) - 1
        + static_cast<qint64>(outsetBottom);
    if (x1 < INT_MIN || x1 > INT_MAX || y1 < INT_MIN || y1 > INT_MAX || x2 < INT_MIN || x2 > INT_MAX
        || y2 < INT_MIN || y2 > INT_MAX || x2 < x1 || y2 < y1) {
        return false;
    }
    const qint64 outerW = x2 - x1 + 1;
    const qint64 outerH = y2 - y1 + 1;
    if (outerW <= 0 || outerW > INT_MAX || outerH <= 0 || outerH > INT_MAX) {
        return false;
    }
    const qint64 ix1 = x1 + static_cast<qint64>(width);
    const qint64 iy1 = y1 + static_cast<qint64>(width);
    const qint64 ix2 = x2 - static_cast<qint64>(width);
    const qint64 iy2 = y2 - static_cast<qint64>(width);
    if (ix1 < INT_MIN || ix1 > INT_MAX || iy1 < INT_MIN || iy1 > INT_MAX || ix2 < INT_MIN || ix2 > INT_MAX
        || iy2 < INT_MIN || iy2 > INT_MAX || ix2 < ix1 || iy2 < iy1) {
        return false;
    }
    const qint64 innerW = ix2 - ix1 + 1;
    const qint64 innerH = iy2 - iy1 + 1;
    if (innerW <= 0 || innerW > INT_MAX || innerH <= 0 || innerH > INT_MAX) {
        return false;
    }
    // Corner construction: QRect(x, y, w, h) derives the corner as
    // left + width - 1, whose intermediate sum can overflow (and abort a
    // debug Qt build) when the mathematically right corner is exactly
    // INT_MAX. The corners above are already validated, so build from
    // them directly.
    if (outer != nullptr) {
        *outer = QRect(QPoint(static_cast<int>(x1), static_cast<int>(y1)),
            QPoint(static_cast<int>(x2), static_cast<int>(y2)));
    }
    if (inner != nullptr) {
        *inner = QRect(QPoint(static_cast<int>(ix1), static_cast<int>(iy1)),
            QPoint(static_cast<int>(ix2), static_cast<int>(iy2)));
    }
    return true;
}

namespace PresentationConsumerDetail
{

inline bool isOpaqueId(const QString &value)
{
    if (value.isEmpty() || value.size() > 128) {
        return false;
    }
    for (const QChar c : value) {
        const ushort u = c.unicode();
        const bool alnum = (u >= u'0' && u <= u'9') || (u >= u'A' && u <= u'Z') || (u >= u'a' && u <= u'z');
        if (!(alnum || u == u'-' || u == u'_' || u == u'.')) {
            return false;
        }
    }
    return true;
}

inline bool isCanonicalU64(const QString &value, quint64 *out)
{
    if (value.isEmpty() || value.size() > 20) {
        return false;
    }
    if (value.size() > 1 && value.startsWith(u'0')) {
        return false;
    }
    quint64 acc = 0;
    for (const QChar c : value) {
        const ushort u = c.unicode();
        if (u < u'0' || u > u'9') {
            return false;
        }
        const quint64 digit = static_cast<quint64>(u - u'0');
        if (acc > (UINT64_MAX - digit) / 10) {
            return false;
        }
        acc = acc * 10 + digit;
    }
    if (out != nullptr) {
        *out = acc;
    }
    return true;
}

// Value-based JSON integer check (finding B): Qt parses every JSON number
// as double, so lexical spelling (fraction/exponent form) is unobservable
// here and must not be claimed. What this proves instead: finite,
// integral, and in int range, with the bounds/finiteness check strictly
// before the cast (casting 1e30 to int first is undefined behavior).
inline bool isJsonIntValue(const QJsonValue &value, int *out)
{
    if (!value.isDouble()) {
        return false;
    }
    const double d = value.toDouble();
    if (!std::isfinite(d) || std::trunc(d) != d) {
        return false;
    }
    if (d < static_cast<double>(INT_MIN) || d > static_cast<double>(INT_MAX)) {
        return false;
    }
    const int v = static_cast<int>(d);
    if (out != nullptr) {
        *out = v;
    }
    return true;
}

inline bool parseRect(const QJsonValue &value, QRect *out)
{
    if (!value.isArray()) {
        return false;
    }
    const QJsonArray a = value.toArray();
    if (a.size() != 4) {
        return false;
    }
    int x = 0;
    int y = 0;
    int w = 0;
    int h = 0;
    if (!isJsonIntValue(a.at(0), &x) || !isJsonIntValue(a.at(1), &y) || !isJsonIntValue(a.at(2), &w) || !isJsonIntValue(a.at(3), &h)) {
        return false;
    }
    if (x < PRESENTATION_COORD_MIN || x > PRESENTATION_COORD_MAX || y < PRESENTATION_COORD_MIN || y > PRESENTATION_COORD_MAX
        || w <= 0 || w > PRESENTATION_SIZE_MAX || h <= 0 || h > PRESENTATION_SIZE_MAX) {
        return false;
    }
    if (out != nullptr) {
        *out = QRect(x, y, w, h);
    }
    return true;
}

} // namespace PresentationConsumerDetail

// Display-only hide (finding G): clears the visible paint while retaining
// the ordering fence and geometry floors, so a stale retransmit still
// cannot resurrect. Used by focus activation, geometry-dirt hides, and
// malformed payloads: the stream continues with its order intact.
inline void clearPaintDisplay(PresentationPaint *state);

// Applies one `SetPresentation` payload to the held paint. `liveInstance`
// and `liveSerial` are the exact live open native gesture identity at the
// endpoint (instance plus serial, "0" when no gesture is open): a proven
// preview arms the latch only when its producer matches them. Returns
// Accepted (state replaced; a set geometry floor still hides the affected
// shape until a paint covers strictly past it), StaleIgnored (older
// `(rev, obs)`; no live source witness; or coverage that is not exactly
// the live source plus counter - older is known-stale, foreign is
// incomparable, future is unproven; display, fence, witness, and floors
// preserved), or Malformed (display cleared only; fence, witness, and
// floors retained so the stream continues and stale retransmits still
// cannot resurrect).
inline PresentationApply applyPresentationPaint(
    PresentationPaint *state, const QString &payload, const QString &liveInstance, const QString &liveSerial)
{
    using namespace PresentationConsumerDetail;
    if (state == nullptr) {
        return PresentationApply::Malformed;
    }
    auto failClosed = [&]() {
        clearPaintDisplay(state);
        return PresentationApply::Malformed;
    };
    if (payload.isEmpty() || payload.size() > 65536) {
        return failClosed();
    }
    QJsonParseError error{};
    const QJsonDocument document = QJsonDocument::fromJson(payload.toUtf8(), &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) {
        return failClosed();
    }
    const QJsonObject o = document.object();
    if (o.size() != 10 || !o.contains(QStringLiteral("v")) || !o.contains(QStringLiteral("gen")) || !o.contains(QStringLiteral("rev"))
        || !o.contains(QStringLiteral("obs")) || !o.contains(QStringLiteral("coveredInstance"))
        || !o.contains(QStringLiteral("coveredInvalid")) || !o.contains(QStringLiteral("foreground"))
        || !o.contains(QStringLiteral("border")) || !o.contains(QStringLiteral("underlay"))
        || !o.contains(QStringLiteral("preview"))) {
        return failClosed();
    }
    if (!isJsonIntValue(o.value(QStringLiteral("v")), nullptr) || o.value(QStringLiteral("v")).toInt(-1) != 2) {
        return failClosed();
    }
    const QString gen = o.value(QStringLiteral("gen")).toString();
    quint64 rev = 0;
    quint64 obs = 0;
    if (!isOpaqueId(gen) || !isCanonicalU64(o.value(QStringLiteral("rev")).toString(), &rev)
        || !isCanonicalU64(o.value(QStringLiteral("obs")).toString(), &obs)) {
        return failClosed();
    }
    // Causal coverage witness: the capture this paint was read from
    // covered exactly this native-invalidation value. Never compared
    // across instances; never inferred from timing or geometry.
    const QString coveredInstance = o.value(QStringLiteral("coveredInstance")).toString();
    quint64 coveredInvalid = 0;
    if (!isOpaqueId(coveredInstance)
        || !isCanonicalU64(o.value(QStringLiteral("coveredInvalid")).toString(), &coveredInvalid)) {
        return failClosed();
    }
    const CoverWitness incomingCover{coveredInstance, coveredInvalid, true};
    const QString foreground = o.value(QStringLiteral("foreground")).toString(QStringLiteral("\u0000"));
    if (foreground != QStringLiteral("") && !isOpaqueId(foreground)) {
        return failClosed();
    }
    const QJsonValue borderValue = o.value(QStringLiteral("border"));
    if (!borderValue.isObject()) {
        return failClosed();
    }
    const QJsonObject border = borderValue.toObject();
    if (border.size() != 7 || !border.contains(QStringLiteral("visible")) || !border.contains(QStringLiteral("reason"))
        || !border.contains(QStringLiteral("subject")) || !border.contains(QStringLiteral("outsets"))
        || !border.contains(QStringLiteral("width")) || !border.contains(QStringLiteral("radius"))
        || !border.contains(QStringLiteral("witness"))) {
        return failClosed();
    }
    const QJsonValue visibleValue = border.value(QStringLiteral("visible"));
    if (!visibleValue.isBool()) {
        return failClosed();
    }
    const bool borderVisible = visibleValue.toBool(false);
    const QString borderReason = border.value(QStringLiteral("reason")).toString(QStringLiteral("\u0000"));
    const bool reasonOk = borderReason == QStringLiteral("") || borderReason == QStringLiteral("disabled")
        || borderReason == QStringLiteral("no-target") || borderReason == QStringLiteral("minimized")
        || borderReason == QStringLiteral("maximized") || borderReason == QStringLiteral("fullscreen")
        || borderReason == QStringLiteral("host-hidden") || borderReason == QStringLiteral("hidden-workspace")
        || borderReason == QStringLiteral("shell") || borderReason == QStringLiteral("suspended");
    if (!reasonOk) {
        return failClosed();
    }
    if (borderVisible && borderReason != QStringLiteral("")) {
        return failClosed();
    }
    const QJsonValue subjectValue = border.value(QStringLiteral("subject"));
    QByteArray borderSubject;
    if (!subjectValue.isNull()) {
        if (!subjectValue.isString() || !isOpaqueId(subjectValue.toString())) {
            return failClosed();
        }
        borderSubject = subjectValue.toString().toUtf8();
    }
    const QJsonValue outsetsValue = border.value(QStringLiteral("outsets"));
    bool borderHasOutsets = false;
    int outsetLeft = 0;
    int outsetTop = 0;
    int outsetRight = 0;
    int outsetBottom = 0;
    if (!outsetsValue.isNull()) {
        if (!outsetsValue.isArray() || outsetsValue.toArray().size() != 4) {
            return failClosed();
        }
        const QJsonArray outsetsArray = outsetsValue.toArray();
        int parsed[4] = {0, 0, 0, 0};
        for (int i = 0; i < 4; ++i) {
            if (!isJsonIntValue(outsetsArray.at(i), &parsed[i])) {
                return failClosed();
            }
        }
        borderHasOutsets = true;
        outsetLeft = parsed[0];
        outsetTop = parsed[1];
        outsetRight = parsed[2];
        outsetBottom = parsed[3];
    }
    const QJsonValue widthValue = border.value(QStringLiteral("width"));
    bool borderHasWidth = false;
    int borderWidth = 0;
    if (!widthValue.isNull()) {
        int parsedWidth = 0;
        if (!isJsonIntValue(widthValue, &parsedWidth) || parsedWidth < 0) {
            return failClosed();
        }
        borderHasWidth = true;
        borderWidth = parsedWidth;
    }
    const QJsonValue radiusValue = border.value(QStringLiteral("radius"));
    bool borderHasRadius = false;
    int borderRadius = 0;
    if (!radiusValue.isNull()) {
        int parsedRadius = 0;
        if (!isJsonIntValue(radiusValue, &parsedRadius) || parsedRadius < 0) {
            return failClosed();
        }
        borderHasRadius = true;
        borderRadius = parsedRadius;
    }
    const QJsonValue witnessValue = border.value(QStringLiteral("witness"));
    bool borderHasWitness = false;
    QByteArray witnessWin;
    QString witnessObs;
    quint64 witnessObsValue = 0;
    QRect witnessFrame;
    QByteArray witnessForeground;
    if (!witnessValue.isNull()) {
        if (!witnessValue.isObject()) {
            return failClosed();
        }
        const QJsonObject witness = witnessValue.toObject();
        if (witness.size() != 4 || !witness.contains(QStringLiteral("win")) || !witness.contains(QStringLiteral("obs"))
            || !witness.contains(QStringLiteral("frame")) || !witness.contains(QStringLiteral("foreground"))) {
            return failClosed();
        }
        const QString winStr = witness.value(QStringLiteral("win")).toString();
        quint64 obsVal = 0;
        if (!isOpaqueId(winStr) || !isCanonicalU64(witness.value(QStringLiteral("obs")).toString(), &obsVal)) {
            return failClosed();
        }
        QRect frameRect;
        if (!parseRect(witness.value(QStringLiteral("frame")), &frameRect)) {
            return failClosed();
        }
        const QString fgStr = witness.value(QStringLiteral("foreground")).toString();
        if (!isOpaqueId(fgStr)) {
            return failClosed();
        }
        borderHasWitness = true;
        witnessWin = winStr.toUtf8();
        witnessObs = witness.value(QStringLiteral("obs")).toString();
        witnessObsValue = obsVal;
        witnessFrame = frameRect;
        witnessForeground = fgStr.toUtf8();
    }
    const QJsonValue underlayValue = o.value(QStringLiteral("underlay"));
    if (!underlayValue.isObject()) {
        return failClosed();
    }
    const QJsonObject underlay = underlayValue.toObject();
    if (!underlay.contains(QStringLiteral("status"))) {
        return failClosed();
    }
    const QString status = underlay.value(QStringLiteral("status")).toString();
    bool found = false;
    QByteArray underlayWin;
    QRect underlayOuter;
    QStringList members;
    if (status == QStringLiteral("found")) {
        if (underlay.size() != 4 || !underlay.contains(QStringLiteral("win")) || !underlay.contains(QStringLiteral("outer"))
            || !underlay.contains(QStringLiteral("members"))) {
            return failClosed();
        }
        const QString win = underlay.value(QStringLiteral("win")).toString();
        if (!isOpaqueId(win)) {
            return failClosed();
        }
        if (!parseRect(underlay.value(QStringLiteral("outer")), &underlayOuter)) {
            return failClosed();
        }
        const QJsonValue membersValue = underlay.value(QStringLiteral("members"));
        if (!membersValue.isArray() || membersValue.toArray().isEmpty()) {
            return failClosed();
        }
        for (const QJsonValue entry : membersValue.toArray()) {
            if (!entry.isString()) {
                return failClosed();
            }
            const QString member = entry.toString();
            if (!isOpaqueId(member) || members.contains(member)) {
                return failClosed();
            }
            members.append(member);
        }
        found = true;
        underlayWin = win.toUtf8();
    } else if (status == QStringLiteral("hidden")) {
        if (underlay.size() != 1) {
            return failClosed();
        }
    } else {
        return failClosed();
    }
    const QJsonValue previewValue = o.value(QStringLiteral("preview"));
    bool previewHas = false;
    QRect previewRect;
    QByteArray previewSourceOut;
    QByteArray previewSourceWs;
    QByteArray previewWin;
    QByteArray previewProducerInstance;
    QByteArray previewProducerSerial;
    if (!previewValue.isNull()) {
        if (!previewValue.isObject()) {
            return failClosed();
        }
        const QJsonObject preview = previewValue.toObject();
        if (preview.size() != 6 || !preview.contains(QStringLiteral("rect")) || !preview.contains(QStringLiteral("sourceOut"))
            || !preview.contains(QStringLiteral("sourceWs")) || !preview.contains(QStringLiteral("win"))
            || !preview.contains(QStringLiteral("producerInstance"))
            || !preview.contains(QStringLiteral("producerSerial"))) {
            return failClosed();
        }
        if (!parseRect(preview.value(QStringLiteral("rect")), &previewRect)) {
            return failClosed();
        }
        const QString sourceOut = preview.value(QStringLiteral("sourceOut")).toString();
        const QString sourceWs = preview.value(QStringLiteral("sourceWs")).toString();
        const QString win = preview.value(QStringLiteral("win")).toString();
        if (!isOpaqueId(sourceOut) || !isOpaqueId(sourceWs) || !isOpaqueId(win)) {
            return failClosed();
        }
        // Producing native gesture: null when unproven (fail closed: never
        // arms the latch). A proven producer arms only when it matches the
        // live open gesture the endpoint received alongside the payload.
        const QJsonValue producerInstanceValue = preview.value(QStringLiteral("producerInstance"));
        const QJsonValue producerSerialValue = preview.value(QStringLiteral("producerSerial"));
        const bool proven = producerInstanceValue.isString() && producerSerialValue.isString()
            && isOpaqueId(producerInstanceValue.toString())
            && isCanonicalU64(producerSerialValue.toString(), nullptr)
            && producerSerialValue.toString() != QStringLiteral("0");
        if (!producerInstanceValue.isNull() || !producerSerialValue.isNull()) {
            if (!proven) {
                return failClosed();
            }
            previewProducerInstance = producerInstanceValue.toString().toUtf8();
            previewProducerSerial = producerSerialValue.toString().toUtf8();
            const bool live = !liveInstance.isEmpty() && liveSerial != QStringLiteral("0")
                && previewProducerInstance == liveInstance.toUtf8()
                && previewProducerSerial == liveSerial.toUtf8();
            previewHas = live;
        }
        previewSourceOut = sourceOut.toUtf8();
        previewSourceWs = sourceWs.toUtf8();
        previewWin = win.toUtf8();
    }
    // Order fence (findings F/G) plus the causal witness: same-generation
    // older `(rev, obs)` never applies (in-transit paints crossing a native
    // invalidation are the reachable stale case). Coverage must prove
    // against the true live source plus counter: an unset latest (no live
    // source ever seeded by effect authority) never applies, a foreign
    // instance never applies (reused counters across instances never
    // compare), and only exactly equal same-instance coverage applies -
    // older coverage is known-stale and future coverage is unproven -
    // unless it reaches the seeded structural floor in the same instance:
    // a newer-order pre-motion paint is accepted on that structural path
    // (motion leaves the floor untouched) while its strict shapes stay
    // hidden by their own gates until exactly covering paint arrives. No
    // arriving paint may establish latest or the floor: the effect-owned
    // router re-seeds both from authority at every endpoint dispatch.
    // A mismatched generation with a set fence is accepted as a new stream:
    // the rotation path always precedes a new stream with the
    // stream-resetting clear on the same ordered channel, so a mismatch
    // means the clear was lost while the endpoint was down and only
    // accepting recovers (rejecting would wedge the fresh stream forever).
    // A late old generation after an accepted new one cannot occur on that
    // ordered channel under subscriber-side generation fencing, so there is
    // no reachable old-replaces-new path to refuse. Equal `(rev, obs)`
    // re-applies idempotently; covering past a set floor clears it, while
    // equal-or-older coverage never does.
    const PaintOrder incoming{gen, rev, obs, true};
    if (state->fence.set && gen == state->fence.gen && paintOrderLess(incoming, state->fence)) {
        return PresentationApply::StaleIgnored;
    }
    if (!state->latest.set) {
        return PresentationApply::StaleIgnored;
    }
    const bool exactCurrent = incomingCover.instance == state->latest.instance
        && incomingCover.counter == state->latest.counter;
    // Structural path: same instance, covered at or past the seeded floor
    // (an unset floor proves nothing: direct consumer use keeps the
    // exact-only gate), never past latest (future coverage is unproven).
    const bool structuralCurrent = state->structuralFloor.set
        && incomingCover.instance == state->structuralFloor.instance
        && incomingCover.counter >= state->structuralFloor.counter
        && incomingCover.instance == state->latest.instance
        && incomingCover.counter <= state->latest.counter;
    if (!exactCurrent && !structuralCurrent) {
        return PresentationApply::StaleIgnored;
    }
    state->has = true;
    state->gen = gen;
    state->rev = rev;
    state->obs = obs;
    state->covered = incomingCover;
    state->foreground = foreground.toUtf8();
    state->borderVisible = borderVisible;
    state->borderReason = borderReason;
    state->borderSubject = borderSubject;
    state->borderHasOutsets = borderHasOutsets;
    state->borderOutsetLeft = outsetLeft;
    state->borderOutsetTop = outsetTop;
    state->borderOutsetRight = outsetRight;
    state->borderOutsetBottom = outsetBottom;
    state->borderHasWidth = borderHasWidth;
    state->borderWidth = borderWidth;
    state->borderHasRadius = borderHasRadius;
    state->borderRadius = borderRadius;
    state->borderHasWitness = borderHasWitness;
    state->witnessWin = witnessWin;
    state->witnessObs = witnessObs;
    state->witnessObsValue = witnessObsValue;
    state->witnessFrame = witnessFrame;
    state->witnessForeground = witnessForeground;
    state->underlayFound = found;
    state->underlayWin = underlayWin;
    state->underlayOuter = underlayOuter;
    state->underlayMembers = members;
    state->previewHas = previewHas;
    state->previewRect = previewRect;
    state->previewSourceOut = previewSourceOut;
    state->previewSourceWs = previewSourceWs;
    state->previewWin = previewWin;
    state->previewProducerInstance = previewProducerInstance;
    state->previewProducerSerial = previewProducerSerial;
    state->fence = incoming;
    // Covering strictly past a set floor clears it; equal-or-older coverage
    // never does (an equal redelivery cannot resurrect a dirtied shape).
    if (coverClearsFloor(incomingCover, state->borderFloor)) {
        state->borderFloor = CoverWitness{};
    }
    if (coverClearsFloor(incomingCover, state->underlayFloor)) {
        state->underlayFloor = CoverWitness{};
    }
    return PresentationApply::Accepted;
}

// Display-only hide (finding G): clears the visible paint while retaining
// the ordering fence, the causal witness, and the geometry floors, so a
// stale retransmit still cannot resurrect. Used by focus activation,
// geometry-dirt hides, and malformed payloads: the stream continues with
// its order intact.
// (Forward-declared above for the malformed path in applyPresentationPaint.)
inline void clearPaintDisplay(PresentationPaint *state)
{
    if (state == nullptr) {
        return;
    }
    const PaintOrder fence = state->fence;
    const CoverWitness latest = state->latest;
    const CoverWitness covered = state->covered;
    const CoverWitness structuralFloor = state->structuralFloor;
    const CoverWitness borderFloor = state->borderFloor;
    const CoverWitness underlayFloor = state->underlayFloor;
    *state = PresentationPaint{};
    state->fence = fence;
    state->latest = latest;
    state->covered = covered;
    state->structuralFloor = structuralFloor;
    state->borderFloor = borderFloor;
    state->underlayFloor = underlayFloor;
}

// Production activation/error-hide routing (finding G): display-only hide
// preserving order/invalidation evidence, then the causal bump. The
// effect's windowActivated handler invokes exactly this (never the
// stream-resetting clear), and the offline red pins this exact function in
// production call order.
inline void hideForActivation(PresentationPaint *state, const QString &instance, quint64 counter)
{
    clearPaintDisplay(state);
    noteNativeInvalidation(state, instance, counter, InvalidationScope::All);
}

// Stream-resetting clear: display, ordering fence, and geometry floors
// all reset. Only the subscriber-driven `ClearPresentation` path
// (generation rotation, unbind, stop, explicit stream replacement) uses
// this; every legitimate new stream starts from an empty fence. The live
// source truth is NOT forgotten here: the effect-owned router re-seeds
// latest from authority at the next endpoint dispatch, so no arriving
// paint can establish it.
inline void clearPresentationPaint(PresentationPaint *state)
{
    if (state != nullptr) {
        *state = PresentationPaint{};
    }
}

} // namespace KWin
