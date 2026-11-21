//! Connection-lifetime configuration, capabilities, and effective settings
//! owned by [`crate::world_engine::WorldEngine`] outside transactional world
//! candidates.

use crate::connection::{ApplicationIdentity, HostCapabilities};
use crate::ids::OwnerId;
use crate::settings_state::SettingsState;

/// Connection-bound identity, capabilities, and effective policy owned by
/// [`crate::world_engine::WorldEngine`]. Immutable during a report transaction.
#[derive(Debug, Clone)]
pub(crate) struct WorldConnectionState {
    pub(crate) core_generation_str: String,
    pub(crate) generation_seed: String,
    pub(crate) connection_number: u64,
    pub(crate) adapter: Option<String>,
    pub(crate) connection_owner: Option<OwnerId>,
    pub(crate) application_identity: Option<ApplicationIdentity>,
    pub(crate) capabilities: Option<HostCapabilities>,
    pub(crate) settings: SettingsState,
    pub(crate) initial_settings_accepted: bool,
}

impl WorldConnectionState {
    #[must_use]
    pub(crate) fn new(core_generation: &str) -> Self {
        Self {
            core_generation_str: core_generation.to_owned(),
            generation_seed: core_generation.to_owned(),
            connection_number: 0,
            adapter: None,
            connection_owner: None,
            application_identity: None,
            capabilities: None,
            settings: SettingsState::default(),
            initial_settings_accepted: false,
        }
    }

    #[must_use]
    pub(crate) fn capabilities(&self) -> Option<&HostCapabilities> {
        self.capabilities.as_ref()
    }

    #[must_use]
    pub(crate) fn application_identity(&self) -> Option<&ApplicationIdentity> {
        self.application_identity.as_ref()
    }

    #[must_use]
    pub(crate) fn settings(&self) -> &SettingsState {
        &self.settings
    }

    #[must_use]
    pub(crate) fn host_owned_workspace_domains(&self) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.host_owned)
    }

    #[must_use]
    pub(crate) fn equal_workspace_count(&self) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.equal_count)
    }

    #[must_use]
    pub(crate) fn shared_native_roster(&self) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.host_owned && caps.shared_native_roster)
    }

    #[must_use]
    pub(crate) fn birth_clear(&self) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.birth_clear)
    }

    #[must_use]
    pub(crate) fn switch_target_setting(&self) -> crate::cursor_targeting::SwitchTarget {
        self.settings.switch_target
    }

    #[must_use]
    pub(crate) fn fresh_placement_setting(&self) -> crate::cursor_targeting::FreshPlacement {
        self.settings.fresh_placement
    }

    #[must_use]
    pub(crate) fn focus_follows_cursor_setting(&self) -> bool {
        matches!(
            self.settings
                .effective
                .get(crate::settings_catalog::FIELD_FOCUS_FOLLOWS_CURSOR),
            Some(crate::settings_state::EffectiveSetting::Bool(true))
        )
    }

    #[must_use]
    pub(crate) fn cursor_warp_setting(&self) -> bool {
        matches!(
            self.settings
                .effective
                .get(crate::settings_catalog::FIELD_CURSOR_WARP),
            Some(crate::settings_state::EffectiveSetting::Bool(true))
        ) && self.capabilities.as_ref().is_some_and(|caps| caps.warp)
    }

    #[must_use]
    pub(crate) fn gap_setting(&self) -> i32 {
        match self
            .settings
            .effective
            .get(crate::settings_catalog::FIELD_INNER_GAP)
        {
            Some(crate::settings_state::EffectiveSetting::Int(value)) => *value,
            _ => 8,
        }
    }

    #[must_use]
    pub(crate) fn outer_gap_setting(&self) -> i32 {
        match self
            .settings
            .effective
            .get(crate::settings_catalog::FIELD_OUTER_GAP)
        {
            Some(crate::settings_state::EffectiveSetting::Int(value)) => *value,
            _ => 8,
        }
    }

    #[must_use]
    pub(crate) fn scaled_gaps(&self, scale: f64) -> Option<(i32, i32)> {
        Some((
            crate::world_engine::WorldEngine::scale_once(self.gap_setting(), scale)?,
            crate::world_engine::WorldEngine::scale_once(self.outer_gap_setting(), scale)?,
        ))
    }

    #[must_use]
    pub(crate) fn workspace_mode_setting(&self) -> crate::workspace_scope::WorkspaceMode {
        self.settings.workspace_mode
    }

    pub(crate) fn shared_roster_slots(&self) -> bool {
        use crate::connection::NativeSelectorScope;
        use crate::workspace_scope::WorkspaceMode;

        self.capabilities().is_some_and(|caps| {
            caps.host_owned
                && caps.shared_native_roster
                && caps.native_selector_scope == NativeSelectorScope::PerOutput
        }) && matches!(
            self.workspace_mode_setting(),
            WorkspaceMode::PerOutputLocal | WorkspaceMode::GlobalUnique
        )
    }

    #[must_use]
    pub(crate) fn fixed_size_predicate_setting(&self) -> crate::size_hints::FixedSizePredicate {
        self.settings.fixed_size_predicate
    }

    #[must_use]
    pub(crate) fn born_maximized_setting(&self) -> crate::born_maximized::BornMaximized {
        self.settings.born_maximized
    }

    #[must_use]
    pub(crate) fn default_tiled(&self) -> bool {
        matches!(
            self.settings
                .effective
                .get(crate::settings_catalog::FIELD_WORKSPACE_DEFAULT_TILED),
            Some(crate::settings_state::EffectiveSetting::Bool(true))
        )
    }
}
