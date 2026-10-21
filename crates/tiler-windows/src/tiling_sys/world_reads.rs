//! World-backed read helpers for the single-owner cutover (stage 2b).
//!
//! The legacy `Engine`/`ManagedWorkspaces`/`WorkspaceScopeOwner` policy
//! owners are deleted: every routing query below reads the sequenced
//! [`WorldEngine`](tiler_core::world_engine::WorldEngine) through
//! [`WorldOwner`](super::world_owner::WorldOwner) plus the mechanical
//! adapter caches (`output_tokens`, `member_tokens`, `hidden_claims`).
//! Device keys (native `\\.\DISPLAY` names) map to wire ids through
//! `output_tokens` only; the core speaks wire ids and logical workspace ids.
//! Nothing here mutates policy: float/session answers additionally consult
//! the last applied reply (a read-only presentation projection) and the
//! adapter-owned intent lanes (`floated`, `sticky`, `fixed_auto_seen`).

use super::*;
use tiler_core::world_report::TilingMode;

/// World-backed home for one member: the native device key plus the core
/// logical workspace id. `hidden` mirrors the adapter hide-claim ledger
/// (independent of the observation gate, like the old session flag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Home {
    pub output: String,
    pub workspace: String,
    pub hidden: bool,
}

impl TileLoop {
    /// Fresh single-window readback for arrival proof: one live native
    /// frame read AFTER an applied world command, never the pre-dispatch
    /// snapshot. Mirrors the retired apply path's per-write readback.
    pub(super) fn readback_frame(
        &mut self,
        me: &ProcessIdentity,
        fulls: &[Rect],
        hwnd: u64,
    ) -> Option<Rect> {
        let window =
            super::observe::observe_window(hwnd as isize as HWND, me, fulls, &mut self.tokens)
                .ok()?;
        (window.hwnd == hwnd).then_some(window.visible)
    }

    /// Core domain fingerprint for one device output: FNV-1a over the
    /// logical ring plus the current view. Observation-stable
    /// (observation ticks never move rings/views) but commit-sensitive,
    /// so only real topology changes fail the START fence.
    pub(super) fn preview_domain_fp(&self, device: &str) -> u64 {
        let wire = self.wire_of(device).unwrap_or_default();
        let engine = self.world.engine();
        let mut hash: u64 = 14695981039346656037;
        let mut feed = |text: &str| {
            for byte in text.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(1099511628211);
            }
            hash ^= 0xff;
            hash = hash.wrapping_mul(1099511628211);
        };
        for id in engine.logical_ring(&wire) {
            feed(&id);
        }
        if let Some(view) = engine.logical_view(&wire) {
            feed(&view);
        }
        hash
    }
}

impl TileLoop {
    /// Wire id for one native device key, or `None` when unknown.
    pub(super) fn wire_of(&self, device: &str) -> Option<String> {
        self.output_tokens.get(device).cloned()
    }

    /// Native device key for one wire id, or `None` when unknown.
    pub(super) fn device_of(&self, wire: &str) -> Option<String> {
        self.output_tokens
            .iter()
            .find(|(_, id)| id.as_str() == wire)
            .map(|(device, _)| device.clone())
    }

    /// Current core logical view for one native device output.
    pub(super) fn active_view(&self, device: &str) -> Option<String> {
        let wire = self.wire_of(device)?;
        self.world.engine().logical_view(&wire)
    }

    /// Mechanical device outputs in stable order.
    pub(super) fn output_devices(&self) -> Vec<String> {
        let mut devices: Vec<String> = self.output_tokens.keys().cloned().collect();
        devices.sort();
        devices
    }

    /// Core logical ring for one native device output (empty when unknown).
    pub(super) fn workspace_ids(&self, device: &str) -> Vec<String> {
        let Some(wire) = self.wire_of(device) else {
            return Vec::new();
        };
        self.world.engine().logical_ring(&wire)
    }

    /// Core tiling mode for one device workspace (unknown reads tiled,
    /// matching the legacy default-on session).
    pub(super) fn is_tiled_view(&self, device: &str, workspace: &str) -> bool {
        let Some(wire) = self.wire_of(device) else {
            return true;
        };
        matches!(
            self.world.engine().logical_tiling(&wire, workspace),
            TilingMode::Tiled
        )
    }

    /// Core tiling mode as `Some`, or `None` when the workspace is unknown
    /// on the output (stale-intent detection needs the distinction).
    pub(super) fn workspace_mode_known_here(&self, device: &str, workspace: &str) -> Option<bool> {
        let wire = self.wire_of(device)?;
        if !self
            .world
            .engine()
            .logical_ring(&wire)
            .iter()
            .any(|id| id == workspace)
        {
            return None;
        }
        Some(matches!(
            self.world.engine().logical_tiling(&wire, workspace),
            TilingMode::Tiled
        ))
    }

    /// World-backed home for one member key: the stable token binds the
    /// core homing, the wire id maps back to the device key.
    pub(super) fn member_home(&self, key: &crate::native_guards::WindowKey) -> Option<Home> {
        let token = self.member_tokens.get(key)?.clone();
        let (wire, workspace) = self
            .world
            .engine()
            .adapter_homing(&tiler_core::directional::WindowId(token))?;
        let output = self.device_of(&wire)?;
        Some(Home {
            output,
            workspace,
            hidden: self.hidden_claims.contains_key(key),
        })
    }

    /// World-backed workspace members: core logical members mapped back to
    /// live member keys through the stable token table.
    pub(super) fn workspace_members(
        &self,
        device: &str,
        workspace: &str,
    ) -> std::collections::BTreeSet<crate::native_guards::WindowKey> {
        let Some(wire) = self.wire_of(device) else {
            return Default::default();
        };
        let members = self.world.engine().logical_members(&wire, workspace);
        let by_token: HashMap<&str, &crate::native_guards::WindowKey> = self
            .member_tokens
            .iter()
            .map(|(key, token)| (token.as_str(), key))
            .collect();
        members
            .iter()
            .filter_map(|id| by_token.get(id.0.as_str()).cloned().cloned())
            .collect()
    }

    /// Mechanical output token for logs: the wire id, or the device key
    /// when unmapped.
    pub(super) fn output_token(&self, device: &str) -> String {
        self.wire_of(device).unwrap_or_else(|| device.to_owned())
    }

    /// Workspace token for logs: the core logical workspace id itself.
    pub(super) fn workspace_token(&self, _device: &str, workspace: &str) -> String {
        workspace.to_owned()
    }

    /// Mechanically ensure one device carries a stable wire id (never
    /// reuses or reorders live ids).
    pub(super) fn ensure_device(&mut self, device: &str) {
        if self.output_tokens.contains_key(device) {
            return;
        }
        let mut next: u32 = 1;
        for id in self.output_tokens.values() {
            if let Some(n) = id.strip_prefix('o').and_then(|n| n.parse::<u32>().ok()) {
                next = next.max(n.saturating_add(1));
            }
        }
        while self
            .output_tokens
            .values()
            .any(|id| id == &format!("o{next}"))
        {
            next = next.saturating_add(1);
        }
        self.output_tokens
            .insert(device.to_owned(), format!("o{next}"));
    }

    /// Forget one member everywhere (verified-close cleanup): drops the
    /// token/identity/tag/rect bindings. Hide claims stay with the caller
    /// ledger. Returns true when the window was known.
    pub(super) fn forget_member(&mut self, key: &crate::native_guards::WindowKey) -> bool {
        let Some(token) = self.member_tokens.remove(key) else {
            self.member_identity.remove(key);
            self.member_tags.remove(key);
            return false;
        };
        self.member_rects.remove(&token);
        self.float_rects.remove(&token);
        self.hint_logged.remove(&token);
        self.hint_fallback_uses.remove(&token);
        self.overlay_now.remove(&token);
        self.member_identity.remove(key);
        self.member_tags.remove(key);
        true
    }

    /// Saved default for newly created workspaces (restart-adopted live
    /// setting, default on). The core seeds logical workspaces from the
    /// report ingress; this only feeds tray truth lines.
    pub(super) fn default_tiled_seed(&self) -> bool {
        self.settings_live
            .as_ref()
            .map(|live| live.settings.core.workspace.default_tiled)
            .unwrap_or(true)
    }

    /// Whether the core currently homes one token as an intentional float:
    /// the last applied reply row, else the adapter intent lanes.
    pub(super) fn core_is_float_token(&self, token: &str) -> bool {
        if let Some(reply) = self.world.last_reply()
            && reply
                .windows
                .iter()
                .any(|row| row.lifetime.0 == token && row.floating)
        {
            return true;
        }
        self.member_tokens.iter().any(|(key, bound)| {
            bound == token && (self.floated.contains(key) || self.sticky.contains_key(key))
        })
    }

    /// Whether one token floats by automatic fixed-size classification:
    /// the adapter D4 latch (the core automatic mark no longer survives as
    /// a separate adapter-readable session flag).
    pub(super) fn core_is_automatic_token(&self, token: &str) -> bool {
        self.member_tokens
            .iter()
            .any(|(key, bound)| bound == token && self.fixed_auto_seen.contains(key))
    }
}
