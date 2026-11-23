#![forbid(unsafe_code)]

//! Read-only Mission Control surface tree dump.
//!
//! Route-discovery diagnostic only: no launch, no writes, no synthetic
//! events, no adversarial fallbacks. It dumps the bounded AX child tree of
//! the shell processes that can own space controls on current macOS — but
//! whose shape has moved across releases and is not assumed here:
//! `com.apple.dock` (the historical Hammerspoon `mc` subtree), the Mission
//! Control app process (`com.apple.exposelauncher`), and the window manager
//! chrome process (`com.apple.WindowManager`). Nothing is pressed; the
//! report exists so a create control is found by observation.
//!
//! Every read uses the existing per-element messaging timeout; the walk is
//! depth- and element-bounded and reports truncation explicitly (never a
//! silent partial tree). Labels (`AXTitle`/`AXDescription`) are opt-in and
//! confined to these exact shell bundles: identity evidence for a control
//! is its role/identifier/actions, and titles are user-adjacent metadata.

use serde::{Deserialize, Serialize};

/// Exact shell targets, in dump order. Labels name the surface, not a
/// content assertion.
pub const TREE_TARGETS: &[(&str, &str)] = &[
    ("dock", "com.apple.dock"),
    ("mission-control", "com.apple.exposelauncher"),
    ("window-manager", "com.apple.WindowManager"),
];

/// Fixed walk bounds: deep enough for the Dock's historical
/// menu-bar/dock/mc nesting, small enough that a pathological tree cannot
/// turn one invocation into an unbounded AX crawl.
pub const TREE_MAX_DEPTH: usize = 12;
/// Per-target element cap (every counted element costs several bounded AX
/// round trips).
pub const TREE_MAX_ELEMENTS: usize = 400;

/// One AX element in a bounded shell tree. `path` is the child-index chain
/// from the application element (`app`, `app.0`, `app.0.3`), so every
/// finding is reproducible by path. Errors are recorded, never hidden: an
/// unreadable attribute leaves its field absent and adds an `errors` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxTreeNode {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subrole: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_id: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// No children were read because the depth bound was reached.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub depth_limited: bool,
    /// No children were read because the element cap was already spent.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub element_limited: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<AxTreeNode>,
}

/// One live process of a target bundle: identity plus its tree (or the
/// reason no tree was produced).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McTreeProcess {
    pub pid: i32,
    /// Fresh per-element-traversal bundle read for the AX target (never the
    /// listing-time value alone).
    pub bundle_fresh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<AxTreeNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One target surface: every live process of the exact bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McTreeTarget {
    pub label: String,
    pub bundle: String,
    pub processes: Vec<McTreeProcess>,
}

/// Whole read-only report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McTreeReport {
    pub session: crate::ffi::SessionState,
    pub ax_trusted: Option<bool>,
    pub labels_included: bool,
    pub max_depth: usize,
    pub max_elements: usize,
    pub targets: Vec<McTreeTarget>,
    pub synthetic_events_posted: bool,
    pub mutates: bool,
}

/// Remaining element budget for one target walk; `spend` reports whether a
/// node may be read so a truncated tree is marked at the exact node.
struct Budget {
    remaining: usize,
}

impl Budget {
    fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }

    fn spend(&mut self) -> bool {
        match self.remaining.checked_sub(1) {
            Some(left) => {
                self.remaining = left;
                true
            }
            None => false,
        }
    }
}

#[cfg(target_os = "macos")]
mod live {
    use super::{AxTreeNode, Budget, McTreeProcess, McTreeReport, McTreeTarget, TREE_TARGETS};
    use crate::ffi;
    use crate::mc_probe::{
        MC_AX_TIMEOUT_SECS, McError, require_ax_trust, require_unlocked_session,
    };

    /// Read one string attribute; an unreadable read is recorded, never
    /// invented. A missing/valueless attribute is simply absent.
    fn read_string(
        element: &ffi::AxElement,
        name: &str,
        errors: &mut Vec<String>,
    ) -> Option<String> {
        match ffi::ax_string_attribute(element, name) {
            Ok(value) => value,
            Err(code) => {
                errors.push(format!("{name}:{}", ffi::ax_error_name(code)));
                None
            }
        }
    }

    /// One bounded node: identity fields plus actions, then recursion under
    /// the depth/element bounds. Read-only; the caller holds the budget.
    fn read_node(
        element: &ffi::AxElement,
        path: &str,
        depth: usize,
        labels: bool,
        budget: &mut Budget,
    ) -> AxTreeNode {
        let mut errors = Vec::new();
        let role = read_string(element, "AXRole", &mut errors);
        let subrole = read_string(element, "AXSubrole", &mut errors);
        let identifier = match ffi::ax_identifier(element) {
            Ok(value) => value,
            Err(code) => {
                errors.push(format!("AXIdentifier:{}", ffi::ax_error_name(code)));
                None
            }
        };
        let display_id = match ffi::ax_display_id(element) {
            Ok(value) => value,
            Err(code) => {
                errors.push(format!("AXDisplayID:{}", ffi::ax_error_name(code)));
                None
            }
        };
        let (title, description) = if labels {
            (
                read_string(element, "AXTitle", &mut errors),
                read_string(element, "AXDescription", &mut errors),
            )
        } else {
            (None, None)
        };
        let actions = match ffi::ax_action_names(element) {
            Ok(names) => names,
            Err(code) => {
                errors.push(format!("AXActionNames:{code}"));
                Vec::new()
            }
        };

        let mut node = AxTreeNode {
            path: path.to_owned(),
            role,
            subrole,
            identifier,
            title,
            description,
            display_id,
            actions,
            errors,
            depth_limited: false,
            element_limited: false,
            children: Vec::new(),
        };

        if depth >= super::TREE_MAX_DEPTH {
            node.depth_limited = true;
            return node;
        }
        let children = match ffi::ax_children(element) {
            Ok(children) => children,
            Err(code) => {
                node.errors
                    .push(format!("AXChildren:{}", ffi::ax_error_name(code)));
                return node;
            }
        };
        for (index, child) in children.iter().enumerate() {
            if !budget.spend() {
                node.element_limited = true;
                break;
            }
            let child_path = format!("{path}.{index}");
            node.children
                .push(read_node(child, &child_path, depth + 1, labels, budget));
        }
        node
    }

    /// All live processes of one exact bundle in this session's application
    /// table. Empty means absent here; the report never invents one.
    fn pids_for_bundle(bundle: &str) -> Vec<i32> {
        let mut pids: Vec<i32> = ffi::running_apps()
            .into_iter()
            .filter(|(_, listed)| listed == bundle)
            .map(|(pid, _)| pid)
            .filter(|pid| *pid > 1)
            .collect();
        pids.sort_unstable();
        pids.dedup();
        pids
    }

    fn dump_target(label: &'static str, bundle: &'static str, labels: bool) -> McTreeTarget {
        let mut processes = Vec::new();
        for pid in pids_for_bundle(bundle) {
            let bundle_fresh = ffi::bundle_id_for_pid(pid);
            let mut budget = Budget::new(super::TREE_MAX_ELEMENTS);
            let (tree, error) = match bundle_fresh.as_deref() {
                Some(fresh) if fresh == bundle => match ffi::AxElement::application(pid) {
                    Some(element) => {
                        let status = ffi::ax_set_messaging_timeout(&element, MC_AX_TIMEOUT_SECS);
                        if status != 0 {
                            (
                                None,
                                Some(format!(
                                    "AX messaging timeout unset: {}",
                                    ffi::ax_error_name(status)
                                )),
                            )
                        } else if budget.spend() {
                            (
                                Some(read_node(&element, "app", 0, labels, &mut budget)),
                                None,
                            )
                        } else {
                            (
                                None,
                                Some("element budget exhausted before the app root".to_owned()),
                            )
                        }
                    }
                    None => (None, Some("AX application element unavailable".to_owned())),
                },
                Some(fresh) => (
                    None,
                    Some(format!(
                        "fresh bundle read '{fresh}' no longer names {bundle}"
                    )),
                ),
                None => (None, Some("fresh bundle read unavailable".to_owned())),
            };
            processes.push(McTreeProcess {
                pid,
                bundle_fresh,
                tree,
                error,
            });
        }
        McTreeTarget {
            label: label.to_owned(),
            bundle: bundle.to_owned(),
            processes,
        }
    }

    /// Read-only tree dump over the fixed shell targets. Refuses unless the
    /// session is unlocked and Accessibility trust is actually observed:
    /// reading a foreign AX tree without trust would only produce a tree of
    /// `kAXErrorAPIDisabled` noise, never evidence.
    pub fn run(labels: bool) -> Result<String, (i32, String)> {
        require_unlocked_session(&ffi::session_state()).map_err(|e: McError| (1, e.to_string()))?;
        require_ax_trust(ffi::ax_is_process_trusted()).map_err(|e: McError| (1, e.to_string()))?;
        let report = McTreeReport {
            session: ffi::session_state(),
            ax_trusted: ffi::ax_is_process_trusted(),
            labels_included: labels,
            max_depth: super::TREE_MAX_DEPTH,
            max_elements: super::TREE_MAX_ELEMENTS,
            targets: TREE_TARGETS
                .iter()
                .map(|(label, bundle)| dump_target(label, bundle, labels))
                .collect(),
            synthetic_events_posted: false,
            mutates: false,
        };
        serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
    }
}

/// Entry point used by the CLI (macOS-only; no cross-platform stub state to
/// observe).
#[cfg(target_os = "macos")]
pub fn run_mc_tree(labels: bool) -> Result<String, (i32, String)> {
    live::run(labels)
}

#[cfg(not(target_os = "macos"))]
pub fn run_mc_tree(_labels: bool) -> Result<String, (i32, String)> {
    Err((1, "mc-tree is macOS-only".to_owned()))
}
