//! Pure per-output-local ordinal/trailing/cleanup policy.
//!
//! Portable core policy: no minimum-two floor. Every scope keeps exactly one
//! trailing empty spare (occupied finals supply the next spare); ordinary
//! empty rows prune once unshown, keeping the trailing spare. A host
//! native-desktop floor, if desired, lives in the adapter, never here.

pub const MIN_WORKSPACES: usize = 1;
pub const MAX_ORDERED: usize = 32;

#[must_use]
pub fn select_existing(order_len: usize, index: u8) -> Option<usize> {
    if !(1..=9).contains(&index) {
        return None;
    }
    let pos = usize::from(index - 1);
    if pos < order_len { Some(pos) } else { None }
}

#[must_use]
pub fn resolve_send_target(order_len: usize, index: u8) -> Option<usize> {
    select_existing(order_len, index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFacts {
    pub empty: bool,
    pub visible: bool,
    pub occupied: bool,
    pub retained: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailingPlan {
    pub remove: Vec<usize>,
    pub need_append: bool,
}

#[must_use]
pub fn plan_trailing(facts: &[WorkspaceFacts]) -> TrailingPlan {
    if facts.is_empty() {
        return TrailingPlan {
            remove: Vec::new(),
            need_append: true,
        };
    }
    let last = facts.len() - 1;
    let trailing_empty = facts[last].empty && !facts[last].occupied;
    let mut remove: Vec<usize> = Vec::new();
    for (i, f) in facts.iter().enumerate() {
        if i == last && trailing_empty {
            continue;
        }
        if !f.empty || f.visible || f.occupied || f.retained {
            continue;
        }
        remove.push(i);
    }
    while facts.len() - remove.len() < MIN_WORKSPACES && !remove.is_empty() {
        remove.remove(0);
    }
    let remaining: Vec<usize> = (0..facts.len()).filter(|i| !remove.contains(i)).collect();
    let trailing_ok = match remaining.last() {
        None => false,
        Some(&i) => facts[i].empty && !facts[i].occupied,
    };
    TrailingPlan {
        remove,
        need_append: !trailing_ok || remaining.len() < MIN_WORKSPACES,
    }
}

#[must_use]
pub fn trailing_reusable(last_empty: bool, last_occupied: bool) -> bool {
    last_empty && !last_occupied
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplacedRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Survivor {
    pub key: String,
    pub rect: Option<DisplacedRect>,
}

#[must_use]
pub fn choose_displaced_destination(
    survivors: &[Survivor],
    active_key: Option<&str>,
    reference: Option<DisplacedRect>,
) -> Option<String> {
    if survivors.is_empty() {
        return None;
    }
    if survivors.len() == 1 {
        return Some(survivors[0].key.clone());
    }
    if let Some(reference) = reference {
        let rx = f64::from(reference.x) + f64::from(reference.w) / 2.0;
        let ry = f64::from(reference.y) + f64::from(reference.h) / 2.0;
        let mut best: Option<(String, f64)> = None;
        for candidate in survivors {
            let Some(rect) = candidate.rect else { continue };
            let cx = f64::from(rect.x) + f64::from(rect.w) / 2.0;
            let cy = f64::from(rect.y) + f64::from(rect.h) / 2.0;
            let dist = (cx - rx) * (cx - rx) + (cy - ry) * (cy - ry);
            let replace = best.as_ref().is_none_or(|(_, d)| dist < *d);
            if replace {
                best = Some((candidate.key.clone(), dist));
            }
        }
        if let Some((key, _)) = best {
            return Some(key);
        }
    }
    if let Some(active) = active_key
        && survivors.iter().any(|s| s.key == active)
    {
        return Some(active.to_owned());
    }
    Some(survivors[0].key.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_visible(visible: bool) -> WorkspaceFacts {
        WorkspaceFacts {
            empty: true,
            visible,
            occupied: false,
            retained: false,
        }
    }

    fn occupied() -> WorkspaceFacts {
        WorkspaceFacts {
            empty: false,
            visible: true,
            occupied: true,
            retained: false,
        }
    }

    #[test]
    fn select_existing_only() {
        assert_eq!(select_existing(3, 1), Some(0));
        assert_eq!(select_existing(3, 3), Some(2));
        assert_eq!(select_existing(3, 4), None);
        assert_eq!(select_existing(0, 1), None);
        assert_eq!(select_existing(3, 0), None);
        assert_eq!(select_existing(3, 10), None);
        assert_eq!(resolve_send_target(2, 2), Some(1));
        assert_eq!(resolve_send_target(2, 3), None);
    }

    #[test]
    fn trailing_reuse_and_append() {
        assert!(trailing_reusable(true, false));
        assert!(!trailing_reusable(true, true));
        assert!(!trailing_reusable(false, false));
        let plan = plan_trailing(&[occupied(), occupied()]);
        assert!(plan.remove.is_empty() && plan.need_append);
        let plan = plan_trailing(&[occupied(), empty_visible(false)]);
        assert!(plan.remove.is_empty() && !plan.need_append);
    }

    #[test]
    fn cleanup_removes_only_invisible_empty_nonfinal() {
        let facts = vec![
            occupied(),
            empty_visible(false),
            empty_visible(false),
            empty_visible(false),
        ];
        let plan = plan_trailing(&facts);
        assert_eq!(plan.remove, vec![1, 2]);
        assert!(!plan.need_append);
    }

    #[test]
    fn cleanup_preserves_visible_occupied_retained() {
        let facts = vec![
            occupied(),
            WorkspaceFacts {
                empty: true,
                visible: false,
                occupied: true,
                retained: false,
            },
            empty_visible(true),
            empty_visible(false),
        ];
        let plan = plan_trailing(&facts);
        assert!(plan.remove.is_empty() && !plan.need_append);
        let facts = vec![
            occupied(),
            WorkspaceFacts {
                empty: true,
                visible: false,
                occupied: false,
                retained: true,
            },
            empty_visible(false),
        ];
        assert!(plan_trailing(&facts).remove.is_empty());
    }

    #[test]
    fn floors_empty_zero_singleton_retained() {
        // Portable core policy: no minimum-two floor. A single trailing
        // empty is sufficient; only an occupied final or an empty set
        // requests an append. A host native-desktop floor lives in the
        // adapter, never here.
        assert!(plan_trailing(&[]).need_append);
        let plan = plan_trailing(&[empty_visible(false)]);
        assert!(plan.remove.is_empty() && !plan.need_append);
        let plan = plan_trailing(&[occupied()]);
        assert!(plan.remove.is_empty() && plan.need_append);
        let plan = plan_trailing(&[empty_visible(false), empty_visible(false)]);
        assert_eq!(plan.remove, vec![0]);
        assert!(!plan.need_append);
        let plan = plan_trailing(&[
            WorkspaceFacts {
                empty: true,
                visible: false,
                occupied: false,
                retained: true,
            },
            empty_visible(false),
        ]);
        assert!(plan.remove.is_empty() && !plan.need_append);
        let plan = plan_trailing(&[empty_visible(true)]);
        assert!(plan.remove.is_empty() && !plan.need_append);
    }

    #[test]
    fn displaced_prefers_reference_then_primary_then_order() {
        let survivors = vec![
            Survivor {
                key: "a".to_owned(),
                rect: Some(DisplacedRect {
                    x: 0,
                    y: 0,
                    w: 1920,
                    h: 1080,
                }),
            },
            Survivor {
                key: "b".to_owned(),
                rect: Some(DisplacedRect {
                    x: 1920,
                    y: 0,
                    w: 1920,
                    h: 1080,
                }),
            },
        ];
        assert_eq!(
            choose_displaced_destination(
                &survivors,
                Some("a"),
                Some(DisplacedRect {
                    x: 2000,
                    y: 0,
                    w: 800,
                    h: 600
                })
            )
            .as_deref(),
            Some("b")
        );
        assert_eq!(
            choose_displaced_destination(&survivors, Some("b"), None).as_deref(),
            Some("b")
        );
        assert_eq!(
            choose_displaced_destination(&survivors, None, None).as_deref(),
            Some("a")
        );
        assert_eq!(choose_displaced_destination(&[], None, None), None);
    }
}
