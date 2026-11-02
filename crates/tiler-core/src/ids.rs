//! Opaque session-scoped identifiers (portable core).
//!
//! Bounded validated owner/generation/correlation tokens. Backing strings are
//! private; construction is fallible via [`OwnerId::parse`],
//! [`GenerationId::parse`], and [`CorrelationId::parse`], which share the
//! existing adapter acceptance alphabets/bounds (see [`crate::contract`]).
//! Access is via [`OwnerId::as_str`] (and siblings) only. [`std::fmt::Debug`]
//! is redacted and no [`std::fmt::Display`] is provided, so status views,
//! errors, and debug renders never echo token bytes.

/// Opaque owner token bound (shared with contract/planner acceptance).
pub const MAX_OWNER_LEN: usize = 128;
/// Opaque generation bound (shared with contract/planner acceptance).
pub const MAX_GENERATION_LEN: usize = 64;
/// Opaque correlation bound (shared with contract/planner acceptance).
pub const MAX_CORRELATION_LEN: usize = 128;
pub const MAX_NATIVE_WORKSPACE_ID_LEN: usize = 128;

fn is_owner_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OWNER_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Owner token validity: non-empty bounded opaque token.
#[must_use]
pub fn is_owner_id(value: &str) -> bool {
    is_owner_token(value)
}

/// Correlation validity: non-empty bounded opaque token.
#[must_use]
pub fn is_correlation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CORRELATION_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Generation validity: lowercase/digit/dash, bounded.
#[must_use]
pub fn is_generation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_GENERATION_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Opaque session owner token. Private backing; [`OwnerId::as_str`] only.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct OwnerId(String);

impl OwnerId {
    /// Bounded validated construction; `None` on any malformed input.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if is_owner_id(value) {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

    /// Borrow the token bytes for exact binding comparisons.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for OwnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OwnerId(redacted)")
    }
}

/// Opaque session generation token. Private backing; [`GenerationId::as_str`] only.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct GenerationId(String);

impl GenerationId {
    /// Bounded validated construction; `None` on any malformed input.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if is_generation_id(value) {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

    /// Borrow the token bytes for exact binding comparisons.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for GenerationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GenerationId(redacted)")
    }
}

/// Opaque correlation token. Private backing; [`CorrelationId::as_str`] only.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CorrelationId(String);

impl CorrelationId {
    /// Bounded validated construction; `None` on any malformed input.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if is_correlation_id(value) {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

    /// Borrow the token bytes for exact binding comparisons.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CorrelationId(redacted)")
    }
}

/// Native workspace identity. Kept distinct from logical workspace IDs even
/// though both use the same bounded opaque wire alphabet.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NativeWorkspaceId(String);

impl NativeWorkspaceId {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if !value.is_empty()
            && value.len() <= MAX_NATIVE_WORKSPACE_ID_LEN
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for NativeWorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeWorkspaceId(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_reject_without_echo() {
        assert!(OwnerId::parse("owner-1").is_some());
        assert!(OwnerId::parse("").is_none());
        assert!(OwnerId::parse("bad id").is_none());
        assert!(OwnerId::parse(&"x".repeat(MAX_OWNER_LEN + 1)).is_none());
        assert!(GenerationId::parse("gen-1").is_some());
        assert!(GenerationId::parse("GEN-1").is_none());
        assert!(GenerationId::parse("").is_none());
        assert!(CorrelationId::parse("corr-1").is_some());
        assert!(CorrelationId::parse("bad corr").is_none());
        assert!(CorrelationId::parse("").is_none());
    }

    #[test]
    fn debug_is_redacted() {
        let owner = OwnerId::parse("owner-1").expect("valid");
        assert_eq!(owner.as_str(), "owner-1");
        assert!(!format!("{owner:?}").contains("owner-1"));
        let generation = GenerationId::parse("gen-1").expect("valid");
        assert!(!format!("{generation:?}").contains("gen-1"));
        let correlation = CorrelationId::parse("corr-1").expect("valid");
        assert!(!format!("{correlation:?}").contains("corr-1"));
        let native = NativeWorkspaceId::parse("native-1").expect("valid");
        assert_eq!(native.as_str(), "native-1");
        assert!(!format!("{native:?}").contains("native-1"));
    }

    #[test]
    fn native_workspace_id_is_bounded_and_opaque() {
        assert!(NativeWorkspaceId::parse("ws.1").is_some());
        assert!(NativeWorkspaceId::parse("").is_none());
        assert!(NativeWorkspaceId::parse("bad id").is_none());
        assert!(NativeWorkspaceId::parse(&"x".repeat(MAX_NATIVE_WORKSPACE_ID_LEN + 1)).is_none());
    }
}
