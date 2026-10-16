use super::*;

/// One effective (validated) binding: the persisted state plus the resolved
/// live chords and whether the owner can intercept them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveBinding {
    pub id: &'static str,
    pub active: bool,
    pub chords: Vec<String>,
    pub effective: bool,
    pub reason: &'static str,
    /// Live OS conflict for the UI: the catalog conflict for kept bindings,
    /// the same OS-owner text for disabled bindings (the OS action is live
    /// there), and the rebound chord's own conflict (see [`chord_conflict`])
    /// for rebinds. `None` means not applicable (unparsable or unbound);
    /// every Win-family chord carries either its documented owner or the
    /// honest unverified note (resize Alt chords included).
    pub conflict: Option<&'static str>,
}

/// Resolve the effective bindings for live use. Assumes `settings` already
/// passed [`validate_settings`]; unknown ids are skipped defensively.
#[must_use]
pub fn effective_bindings(settings: &Settings) -> Vec<EffectiveBinding> {
    let catalog = binding_catalog();
    let mut out = Vec::with_capacity(catalog.len());
    for def in catalog {
        let setting = settings.bindings.get(def.id);
        let (active, chords, effective, reason, conflict) = match setting {
            None
            | Some(BindingSetting {
                state: BindingState::Keep,
                ..
            }) => {
                let chords: Vec<String> = def.defaults.iter().map(|s| (*s).to_owned()).collect();
                if def.defaults.is_empty() {
                    // Unbound bindable row (item 2 stay, item 5 output stay,
                    // item 14 migration): Keep means unbound,
                    // never Disabled/effective interception. The chord stays
                    // pass-through until rebound; the UI shows it unbound,
                    // distinctly from disabled.
                    (true, chords, false, "unbound: bindable", None)
                } else if def.implemented {
                    (true, chords, true, "", def.conflict)
                } else {
                    (
                        true,
                        chords,
                        false,
                        "not intercepted: Alt chords pass through",
                        def.conflict,
                    )
                }
            }
            Some(BindingSetting {
                state: BindingState::Disabled,
                ..
            }) => (
                false,
                Vec::new(),
                false,
                "disabled: passes through natively",
                def.conflict,
            ),
            Some(BindingSetting {
                state: BindingState::Rebind,
                chord,
            }) => match chord.as_deref().and_then(|text| parse_chord(text).ok()) {
                Some(parsed) if def.implemented => {
                    let rendered = render_chord(&parsed);
                    let conflict = chord_conflict(&rendered);
                    (true, vec![rendered], true, "", conflict)
                }
                Some(_) => (
                    true,
                    Vec::new(),
                    false,
                    "not intercepted: Alt chords pass through",
                    def.conflict,
                ),
                None => (
                    false,
                    Vec::new(),
                    false,
                    "malformed chord: passes through",
                    None,
                ),
            },
        };
        out.push(EffectiveBinding {
            id: def.id,
            active,
            chords,
            effective,
            reason,
            conflict,
        });
    }
    out
}

/// Build the live classifier remap from validated settings: one full-modifier
/// entry per effective rebind (Shift/Ctrl/Alt matching the binding's native
/// arm, resize included, float/fullscreen Ctrl alternates included). The rebound chord alone routes; the binding's old
/// default chords pass through via [`build_disabled`]. A keep-equivalent
/// rebind (custom chord exactly equal to one of the row's own defaults)
/// claims nothing new and emits no entry; a same-key modifier-only rebind
/// (Compatible Win+Ctrl+G over Win+G, Win+Ctrl+F11 over Win+F11) still emits
/// its full-modifier entry so the new arm routes. Unbound stay rows
/// route into their canonical slot (shared with the follow arm on the same
/// key) through the explicit stay action; item 14 migration rows route into
/// their canonical letter slot through the explicit migrate action; rebound
/// resize rows route into their letter canonical slot through the
/// mode-carrying resize action.
#[must_use]
pub fn build_remap(settings: &Settings) -> Vec<ChordRemap> {
    let mut defs = BTreeMap::new();
    for def in binding_catalog() {
        defs.insert(def.id, def);
    }
    let mut out = Vec::new();
    for (id, setting) in &settings.bindings {
        if setting.state != BindingState::Rebind {
            continue;
        }
        let Some(def) = defs.get(id.as_str()) else {
            continue;
        };
        if !def.implemented {
            continue;
        }
        let Some(text) = setting.chord.as_deref() else {
            continue;
        };
        let Ok(parsed) = parse_chord(text) else {
            continue;
        };
        if !binding_modifiers_ok(def, parsed.shift, parsed.ctrl, parsed.alt) {
            continue;
        }
        let action = binding_action(def);
        let Some(to_vk) = binding_canonical_vk(def) else {
            continue;
        };
        // Keep-equivalent rebinds (custom chord exactly equals one of the
        // row's own defaults) route natively with no entry. Same-key
        // modifier-only rebinds (Compatible Ctrl+G/F11 over Win+G/Win+F11)
        // still route: only the exact chord match skips.
        let own_default = def
            .defaults
            .iter()
            .any(|default| parse_chord(default).ok() == Some(parsed));
        if own_default {
            continue;
        }
        out.push(ChordRemap {
            from_vk: parsed.vk,
            from_shift: parsed.shift,
            from_ctrl: parsed.ctrl,
            from_alt: parsed.alt,
            action,
            to_vk,
        });
    }
    out.sort_by_key(|entry| {
        (
            entry.from_vk,
            entry.from_shift,
            entry.from_ctrl,
            entry.from_alt,
        )
    });
    out
}

/// Build the live classifier suppression table from validated settings: every
/// disabled binding's default chord passes through, and every rebind's old
/// default chord passes through (only the custom chord routes, via
/// [`build_remap`]). Entries are full-modifier physical chords (resize rows
/// contribute both their letter and arrow defaults). The rebound table wins
/// over suppression for fresh downs, so a key claimed by a rebind still
/// routes even when another row's old default names it.
#[must_use]
pub fn build_disabled(settings: &Settings) -> Vec<ChordDisable> {
    let mut defs = BTreeMap::new();
    for def in binding_catalog() {
        defs.insert(def.id, def);
    }
    let mut out = Vec::new();
    for (id, setting) in &settings.bindings {
        let Some(def) = defs.get(id.as_str()) else {
            continue;
        };
        if !def.implemented {
            continue;
        }
        match setting.state {
            BindingState::Keep => {}
            BindingState::Disabled => {
                for default in def.defaults {
                    if let Ok(parsed) = parse_chord(default) {
                        out.push(ChordDisable {
                            vk: parsed.vk,
                            shift: parsed.shift,
                            ctrl: parsed.ctrl,
                            alt: parsed.alt,
                        });
                    }
                }
            }
            BindingState::Rebind => {
                // Keep-equivalent rebind (custom chord equals the row's own
                // default): the chord still routes natively with no remap
                // entry, so it must not be suppressed either.
                let custom = setting
                    .chord
                    .as_deref()
                    .and_then(|text| parse_chord(text).ok());
                for default in def.defaults {
                    if let Ok(parsed) = parse_chord(default) {
                        if custom == Some(parsed) {
                            continue;
                        }
                        out.push(ChordDisable {
                            vk: parsed.vk,
                            shift: parsed.shift,
                            ctrl: parsed.ctrl,
                            alt: parsed.alt,
                        });
                    }
                }
            }
        }
    }
    out.sort_by_key(|entry| (entry.vk, entry.shift, entry.ctrl, entry.alt));
    out.dedup_by_key(|entry| (entry.vk, entry.shift, entry.ctrl, entry.alt));
    out
}
