//! Core-owned connection, handshake, and settings vocabulary.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FactName {
    Privileged,
    Managed,
    AcceptsInput,
    ActivationInhibited,
    ToolWindow,
    Owned,
    Transient,
    Modal,
    Resizable,
    Above,
    Below,
    Owner,
    Parent,
    Role,
    AppId,
    Executable,
    ProcessId,
    Class,
    Title,
    ActivationPolicy,
    Layer,
}

impl FactName {
    pub const ALL: [Self; 21] = [
        Self::Privileged,
        Self::Managed,
        Self::AcceptsInput,
        Self::ActivationInhibited,
        Self::ToolWindow,
        Self::Owned,
        Self::Transient,
        Self::Modal,
        Self::Resizable,
        Self::Above,
        Self::Below,
        Self::Owner,
        Self::Parent,
        Self::Role,
        Self::AppId,
        Self::Executable,
        Self::ProcessId,
        Self::Class,
        Self::Title,
        Self::ActivationPolicy,
        Self::Layer,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Privileged => "privileged",
            Self::Managed => "managed",
            Self::AcceptsInput => "acceptsInput",
            Self::ActivationInhibited => "activationInhibited",
            Self::ToolWindow => "toolWindow",
            Self::Owned => "owned",
            Self::Transient => "transient",
            Self::Modal => "modal",
            Self::Resizable => "resizable",
            Self::Above => "above",
            Self::Below => "below",
            Self::Owner => "owner",
            Self::Parent => "parent",
            Self::Role => "role",
            Self::AppId => "appId",
            Self::Executable => "executable",
            Self::ProcessId => "processId",
            Self::Class => "class",
            Self::Title => "title",
            Self::ActivationPolicy => "activationPolicy",
            Self::Layer => "layer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeSelectorScope {
    PerOutput,
    AllOutputs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCapabilities {
    pub host_owned: bool,
    pub equal_count: bool,
    pub birth_clear: bool,
    pub native_selector_scope: NativeSelectorScope,
    pub window_suppression: bool,
    pub maximize: bool,
    pub fullscreen: bool,
    pub native_sticky: bool,
    pub above: bool,
    pub warp: bool,
    pub tiled_float_transfer: bool,
    pub unavailable_facts: Vec<FactName>,
}

impl Default for HostCapabilities {
    fn default() -> Self {
        Self {
            host_owned: false,
            equal_count: false,
            birth_clear: false,
            native_selector_scope: NativeSelectorScope::PerOutput,
            window_suppression: false,
            maximize: false,
            fullscreen: false,
            native_sticky: false,
            above: false,
            warp: false,
            tiled_float_transfer: false,
            unavailable_facts: Vec::new(),
        }
    }
}

impl HostCapabilities {
    /// Test/support helper: default capabilities for a connection with no
    /// native identity producers. Declares `executable` and `processId`
    /// unavailable so a null [`ApplicationIdentity`] stays support-consistent
    /// with the strict handshake gate.
    #[doc(hidden)]
    #[must_use]
    pub fn without_native_identity() -> Self {
        Self {
            unavailable_facts: vec![FactName::Executable, FactName::ProcessId],
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationIdentity {
    pub app_id: Option<String>,
    pub executable: Option<String>,
    pub process_id: Option<String>,
}

impl ApplicationIdentity {
    /// Own-identity support consistency (mirrors the strict handshake gate):
    /// declaring `executable`/`processId` unavailable requires the matching
    /// own value to be null; support requires it non-null. `appId` carries
    /// no unavailable coupling. Process identity is never inferred from
    /// executable/appId/class or metadata spelling.
    #[must_use]
    pub fn support_consistent(&self, unavailable: &[FactName]) -> bool {
        (unavailable.contains(&FactName::Executable) == self.executable.is_none())
            && (unavailable.contains(&FactName::ProcessId) == self.process_id.is_none())
    }

    /// Grammar gate for carried identity text. This is the single source
    /// the strict wire codec calls: `appId`/`executable` accept any
    /// non-empty text within the metadata bound without NUL, while
    /// `processId` lives in the opaque Id namespace. Nulls carry no text
    /// and always pass; support coupling stays in [`Self::support_consistent`].
    #[must_use]
    pub fn grammar_valid(&self) -> bool {
        self.app_id.as_deref().is_none_or(valid_identity_text)
            && self.executable.as_deref().is_none_or(valid_identity_text)
            && self
                .process_id
                .as_deref()
                .is_none_or(valid_process_identity_text)
    }
}

/// Carried `appId`/`executable` text grammar shared with the wire codec:
/// non-empty, within the metadata bound, and NUL-free.
#[must_use]
pub fn valid_identity_text(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= crate::world_report::MAX_METADATA_TEXT_BYTES
        && !value.contains('\0')
}

/// Carried `processId` grammar shared with the wire codec: the existing
/// opaque Id namespace, never a JSON number on the wire and never inferred
/// from executable/appId/class.
#[must_use]
pub fn valid_process_identity_text(value: &str) -> bool {
    crate::bounds::is_opaque_id(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolKind {
    Malformed,
    Oversized,
    SnapshotInvalid,
    UnknownField,
    UnknownValue,
    UnsupportedVersion,
    OwnerInvalid,
    CorrelationInvalid,
    GenerationMismatch,
    ObservationStale,
    RequestSuperseded,
    RequestReused,
    ReplyOversize,
    RevisionExhausted,
    CreationExhausted,
}

impl ProtocolKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::Oversized => "oversized",
            Self::SnapshotInvalid => "snapshot-invalid",
            Self::UnknownField => "unknown-field",
            Self::UnknownValue => "unknown-value",
            Self::UnsupportedVersion => "unsupported-version",
            Self::OwnerInvalid => "owner-invalid",
            Self::CorrelationInvalid => "correlation-invalid",
            Self::GenerationMismatch => "generation-mismatch",
            Self::ObservationStale => "observation-stale",
            Self::RequestSuperseded => "request-superseded",
            Self::RequestReused => "request-reused",
            Self::ReplyOversize => "reply-oversize",
            Self::RevisionExhausted => "revision-exhausted",
            Self::CreationExhausted => "creation-exhausted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeRequest {
    pub adapter: String,
    pub request: String,
    pub application_identity: ApplicationIdentity,
    pub capabilities: HostCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeReply {
    pub adapter: String,
    pub request: String,
    pub generation: Option<String>,
    pub ready: bool,
    pub kind: Option<ProtocolKind>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RawSettingValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsRequest {
    pub adapter: String,
    pub generation: String,
    pub request: String,
    pub revision: u64,
    pub values: Vec<(String, RawSettingValue)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsStatus {
    Accepted,
    Stale,
    RevisionReused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsDiagnosticKind {
    Missing,
    UnknownField,
    WrongType,
    OutOfRange,
    UnknownValue,
    RestartRequired,
    UnsupportedMode,
    UnsupportedCapability,
}

impl SettingsDiagnosticKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::UnknownField => "unknown-field",
            Self::WrongType => "wrong-type",
            Self::OutOfRange => "out-of-range",
            Self::UnknownValue => "unknown-value",
            Self::RestartRequired => "restart-required",
            Self::UnsupportedMode => "unsupported-mode",
            Self::UnsupportedCapability => "unsupported-capability",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsDiagnostic {
    pub key: String,
    pub kind: SettingsDiagnosticKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsReply {
    pub adapter: String,
    pub generation: String,
    pub request: String,
    pub revision: u64,
    pub status: SettingsStatus,
    pub diagnostics: Vec<SettingsDiagnostic>,
}
