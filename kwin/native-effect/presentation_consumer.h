// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
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
//   {"v":1,"gen":"<opaque>","rev":"<canonical-u64>","obs":"<canonical-u64>",
//    "foreground":"<id-or-empty>",
//    "border":{"visible":0/1,"inner":[x,y,w,h]|null,"outer":[x,y,w,h]|null},
//    "underlay":{"status":"found","win":"<id>","outer":[x,y,w,h],
//                "members":["<id>",...]}|{"status":"hidden"},
//    "preview":{"rect":[x,y,w,h],"sourceOut":"<id>","sourceWs":"<id>",
//               "win":"<id>"}|null}
// - `v` is exactly the integer 1 (value-based: integral finite double).
// - `rev`/`obs` are canonical decimal u64 strings (no leading zeros, no
//   sign). The `(rev, obs)` pair is the order key: different observations
//   under the same state revision can change presentation, so ordering uses
//   both, never the revision alone.
// - `foreground` is empty (no eligible target) or an opaque window id.
// - Rect value check (Qt parses every JSON number as double, so lexical
//   spelling is unobservable here): finite, integral, in int range, then
//   the carried-geometry bound (x/y in -16384..16384, w/h in 1..16384,
//   matching the retired group/preview gates and the core GEOMETRY_BOUND).
//   Out of bound, fractional, non-finite, or out-of-int-range numbers fail
//   closed (clear).
// - `members` is a non-empty unique opaque-id list (the renderer anchors
//   the already-verbatim outer rect at the lowest-stacked renderable
//   member; member rects never cross). Empty/duplicate/non-id members fail
//   closed.
// - `preview.win` is the insertion neighbor, never the mover identity: the
//   native show gate uses only gesture liveness (a live interactive move),
//   never an identity match against the mover.
// - The read correlation `req` is subscriber-side only (nonce plus
//   generation fencing at the read site); native orders paints on
//   `(gen, rev, obs)` alone, which is sufficient on the ordered channel
//   with those fences.
// - No styling inputs cross here: native styling stays native
//   (independent per-surface configured fills with the live system accent
//   default, never core-derived).
//
// Ordering and freshness model (all inside this Qt-only header so the
// offline consumer test pins it without KWin):
// - `fence` is the ordering token, independent of visible state: display
//   clears (focus activation, geometry dirt, malformed payloads) retain it;
//   only the stream-resetting `ClearPresentation` path (generation
//   rotation, unbind, stop) resets it. Same-generation older `(rev, obs)`
//   never applies; a different generation with a set fence never applies
//   (late old generation must not replace an accepted new one). Newer
//   `(rev, obs)` applies; equal `(rev, obs)` re-applies idempotently.
// - `borderFloor`/`underlayFloor` snapshot the fence when native frame
//   geometry changes on the border target / any held underlay member. The
//   affected shape stays hidden until a strictly newer paint arrives; an
//   equal-redelivery never resurrects it, and geometry coincidence (a move
//   that returns to its start) never clears the floor without a fresh
//   paint. The underlay depends on ALL held members, not just the anchor.

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

// Ordering token: serving generation plus the (revision, observation)
// pair. Opaque generations never compare ordinally; same-generation order
// uses `(rev, obs)` lexicographically because different observations under
// the same state revision can change presentation.
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

struct PresentationPaint {
    bool has = false;
    QString gen;
    quint64 rev = 0;
    quint64 obs = 0;
    QByteArray foreground;
    bool borderVisible = false;
    QRect borderInner;
    QRect borderOuter;
    bool underlayFound = false;
    QByteArray underlayWin;
    QRect underlayOuter;
    QStringList underlayMembers;
    bool previewHas = false;
    QRect previewRect;
    QByteArray previewSourceOut;
    QByteArray previewSourceWs;
    QByteArray previewWin;
    // Ordering token, independent of visible state (finding G): retained
    // across display-only hides, reset only by the stream-resetting clear.
    PaintOrder fence;
    // Geometry-dirt floors (finding F): snapshot of the fence when native
    // frame geometry changed on the dependent windows. A set floor hides
    // its shape until a strictly newer paint arrives.
    PaintOrder borderFloor;
    PaintOrder underlayFloor;
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

// Display-freshness for the affected shape: a set floor hides until the
// held fence advances strictly past the dirt point. A generation mismatch
// between floor and fence fails closed (hidden); floors snapshot the live
// fence at dirt time so they always share its stream in practice.
inline bool presentationBorderFresh(const PresentationPaint *state)
{
    if (state == nullptr) {
        return false;
    }
    if (!state->borderFloor.set) {
        return true;
    }
    return paintOrderGreater(state->fence, state->borderFloor);
}

inline bool presentationUnderlayFresh(const PresentationPaint *state)
{
    if (state == nullptr) {
        return false;
    }
    if (!state->underlayFloor.set) {
        return true;
    }
    return paintOrderGreater(state->fence, state->underlayFloor);
}

// Native geometry-dirt marks (finding F): snapshot the current fence so a
// strictly newer paint alone clears the floor. Never compares live frames,
// so same-window moves and A->B->A races stay hidden without a fresh paint.
inline void markBorderGeometryDirty(PresentationPaint *state)
{
    if (state == nullptr) {
        return;
    }
    state->borderFloor = state->fence;
    state->borderFloor.set = true;
}

inline void markUnderlayGeometryDirty(PresentationPaint *state)
{
    if (state == nullptr) {
        return;
    }
    state->underlayFloor = state->fence;
    state->underlayFloor.set = true;
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

// Applies one `SetPresentation` payload to the held paint. Returns Accepted
// (state replaced; a set geometry floor still hides the affected shape
// until strictly newer content arrives), StaleIgnored (older `(rev, obs)`
// or a mismatched generation with a set fence; display, fence, and floors
// preserved), or Malformed (display cleared only; fence and floors
// retained so the stream continues and stale retransmits still cannot
// resurrect).
inline PresentationApply applyPresentationPaint(PresentationPaint *state, const QString &payload)
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
    if (o.size() != 8 || !o.contains(QStringLiteral("v")) || !o.contains(QStringLiteral("gen")) || !o.contains(QStringLiteral("rev"))
        || !o.contains(QStringLiteral("obs")) || !o.contains(QStringLiteral("foreground")) || !o.contains(QStringLiteral("border"))
        || !o.contains(QStringLiteral("underlay")) || !o.contains(QStringLiteral("preview"))) {
        return failClosed();
    }
    if (!isJsonIntValue(o.value(QStringLiteral("v")), nullptr) || o.value(QStringLiteral("v")).toInt(-1) != 1) {
        return failClosed();
    }
    const QString gen = o.value(QStringLiteral("gen")).toString();
    quint64 rev = 0;
    quint64 obs = 0;
    if (!isOpaqueId(gen) || !isCanonicalU64(o.value(QStringLiteral("rev")).toString(), &rev)
        || !isCanonicalU64(o.value(QStringLiteral("obs")).toString(), &obs)) {
        return failClosed();
    }
    const QString foreground = o.value(QStringLiteral("foreground")).toString(QStringLiteral("\u0000"));
    if (foreground != QStringLiteral("") && !isOpaqueId(foreground)) {
        return failClosed();
    }
    const QJsonValue borderValue = o.value(QStringLiteral("border"));
    if (!borderValue.isObject()) {
        return failClosed();
    }
    const QJsonObject border = borderValue.toObject();
    if (border.size() != 3 || !border.contains(QStringLiteral("visible")) || !border.contains(QStringLiteral("inner"))
        || !border.contains(QStringLiteral("outer"))) {
        return failClosed();
    }
    const QJsonValue visibleValue = border.value(QStringLiteral("visible"));
    if (!isJsonIntValue(visibleValue, nullptr)) {
        return failClosed();
    }
    const int visibleInt = visibleValue.toInt(-1);
    if (visibleInt != 0 && visibleInt != 1) {
        return failClosed();
    }
    QRect inner;
    QRect outer;
    bool hasInner = false;
    bool hasOuter = false;
    const QJsonValue innerValue = border.value(QStringLiteral("inner"));
    const QJsonValue outerValue = border.value(QStringLiteral("outer"));
    if (!innerValue.isNull()) {
        if (!parseRect(innerValue, &inner)) {
            return failClosed();
        }
        hasInner = true;
    }
    if (!outerValue.isNull()) {
        if (!parseRect(outerValue, &outer)) {
            return failClosed();
        }
        hasOuter = true;
    }
    if (visibleInt == 1 && (!hasInner || !hasOuter)) {
        return failClosed();
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
    if (!previewValue.isNull()) {
        if (!previewValue.isObject()) {
            return failClosed();
        }
        const QJsonObject preview = previewValue.toObject();
        if (preview.size() != 4 || !preview.contains(QStringLiteral("rect")) || !preview.contains(QStringLiteral("sourceOut"))
            || !preview.contains(QStringLiteral("sourceWs")) || !preview.contains(QStringLiteral("win"))) {
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
        previewHas = true;
        previewSourceOut = sourceOut.toUtf8();
        previewSourceWs = sourceWs.toUtf8();
        previewWin = win.toUtf8();
    }
    // Order fence (findings F/G): same-generation older `(rev, obs)`
    // never applies (in-transit paints crossing a native invalidation are
    // the reachable stale case). A mismatched generation with a set fence
    // is accepted as a new stream: the rotation path always precedes a new
    // stream with the stream-resetting clear on the same ordered channel,
    // so a mismatch means the clear was lost while the endpoint was down
    // and only accepting recovers (rejecting would wedge the fresh stream
    // forever). A late old generation after an accepted new one cannot
    // occur on that ordered channel under subscriber-side generation
    // fencing, so there is no reachable old-replaces-new path to refuse.
    // Equal `(rev, obs)` re-applies idempotently but never clears a set
    // geometry floor; only strictly newer content does.
    const PaintOrder incoming{gen, rev, obs, true};
    if (state->fence.set && gen == state->fence.gen && paintOrderLess(incoming, state->fence)) {
        return PresentationApply::StaleIgnored;
    }
    const bool strictlyNewer = !state->fence.set || paintOrderGreater(incoming, state->fence);
    state->has = true;
    state->gen = gen;
    state->rev = rev;
    state->obs = obs;
    state->foreground = foreground.toUtf8();
    state->borderVisible = visibleInt == 1;
    state->borderInner = inner;
    state->borderOuter = outer;
    state->underlayFound = found;
    state->underlayWin = underlayWin;
    state->underlayOuter = underlayOuter;
    state->underlayMembers = members;
    state->previewHas = previewHas;
    state->previewRect = previewRect;
    state->previewSourceOut = previewSourceOut;
    state->previewSourceWs = previewSourceWs;
    state->previewWin = previewWin;
    state->fence = incoming;
    if (strictlyNewer) {
        state->borderFloor = PaintOrder{};
        state->underlayFloor = PaintOrder{};
    }
    return PresentationApply::Accepted;
}

// Display-only hide (finding G): clears the visible paint while retaining
// the ordering fence and geometry floors, so a stale retransmit still
// cannot resurrect. Used by focus activation, geometry-dirt hides, and
// malformed payloads: the stream continues with its order intact.
// (Forward-declared above for the malformed path in applyPresentationPaint.)
inline void clearPaintDisplay(PresentationPaint *state)
{
    if (state == nullptr) {
        return;
    }
    const PaintOrder fence = state->fence;
    const PaintOrder borderFloor = state->borderFloor;
    const PaintOrder underlayFloor = state->underlayFloor;
    *state = PresentationPaint{};
    state->fence = fence;
    state->borderFloor = borderFloor;
    state->underlayFloor = underlayFloor;
}

// Stream-resetting clear: display, ordering fence, and geometry floors all
// reset. Only the subscriber-driven `ClearPresentation` path (generation
// rotation, unbind, stop) uses this; every legitimate new stream starts
// from an empty fence.
inline void clearPresentationPaint(PresentationPaint *state)
{
    if (state != nullptr) {
        *state = PresentationPaint{};
    }
}

} // namespace KWin
