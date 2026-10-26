// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

// Shared native-effect logic (Stage 5): only the versioned raw presentation
// facts helpers for the KWin script route (Stage 3 ingress, read-only, no
// policy) remain here. Geometry, eligibility, and visibility derivation are
// deleted: the effect renders accepted core shapes verbatim (see
// presentation_consumer.h), native styling stays in the effect, and trigger
// state (chord/move) is observed natively without a shared predicate.

#include <QColor>
#include <QRectF>
#include <QString>

#include <cstdint>

namespace KWin
{

// Versioned raw presentation facts for the KWin script route (Stage 3
// ingress, read-only, no policy): the facts the script cannot observe
// itself (native modifier chord level, actual effect style configuration)
// plus effect instance/revision identity for stale-snapshot fencing.
// The script holds one WaitPresentationFacts call per adapter generation;
// the effect completes it immediately when the known identity mismatches
// and otherwise holds it until a fact changes or the bounded lease
// expires. Wire JSON (v:1):
// {"v":1,"instance":"<uuid>","rev":"<n>","chord":0/1,"chordKnown":0/1,
//  "gap":<f64>,"width":<f64>,"extension":<f64>,"changed":0/1,
//  "superseded":0/1,"available":0/1}
// Doubles ride full round-trip precision; the adapter clamps to its own
// style bound. The extension sentinel (< 0) resolves adapter-side.
struct PresentationFactsSnapshot
{
    QString instance;
    quint64 rev = 0;
    bool chord = false;
    bool chordKnown = false;
    double gap = 0.0;
    double width = 0.0;
    double extension = 0.0;
    bool changed = false;
    // Explicit supersession lane: a newer Wait replaces a pending older one.
    // The replaced waiter completes unchanged with superseded set, so the
    // script stands down to its host deadline instead of ping-ponging an
    // immediate re-arm against the replacement.
    bool superseded = false;
    // Explicit availability lane: the destructor completes a pending waiter
    // with available cleared, so the script hides immediately on endpoint
    // unload instead of holding a stale snapshot through its lease.
    bool available = true;
};

inline QString presentationFactsJson(const PresentationFactsSnapshot &facts)
{
    return QStringLiteral("{\"v\":1,\"instance\":\"%1\",\"rev\":\"%2\",\"chord\":%3,\"chordKnown\":%4,\"gap\":%5,\"width\":%6,\"extension\":%7,\"changed\":%8,\"superseded\":%9,\"available\":%10}")
        .arg(facts.instance)
        .arg(QString::number(facts.rev))
        .arg(facts.chord ? 1 : 0)
        .arg(facts.chordKnown ? 1 : 0)
        .arg(QString::number(facts.gap, 'g', 17))
        .arg(QString::number(facts.width, 'g', 17))
        .arg(QString::number(facts.extension, 'g', 17))
        .arg(facts.changed ? 1 : 0)
        .arg(facts.superseded ? 1 : 0)
        .arg(facts.available ? 1 : 0);
}

// True when the script-known identity no longer matches the live facts:
// a new effect instance, or a newer revision. Either case completes a held
// waiter immediately with the fresh snapshot (changed:1) instead of
// holding a stale identity through the lease.
inline bool presentationFactsStale(
    const QString &knownInstance, const QString &knownRev, const QString &instance, quint64 rev)
{
    return knownInstance != instance || knownRev != QString::number(rev);
}

// Bounded waiter lease, milliseconds: the script asks ~20 s (the service
// wake lease scale); the effect clamps below to 1 s (never a hot loop)
// and above to 60 s (a wedged waiter still recycles inside the script
// host deadline).
inline int presentationFactsLeaseMs(int requestedMs)
{
    if (requestedMs < 1000) {
        return 1000;
    }
    if (requestedMs > 60000) {
        return 60000;
    }
    return requestedMs;
}

} // namespace KWin
