use serde::{Deserialize, Serialize};

/// Monotonically-increasing schema version for the canonical IR.
///
/// Bump whenever a field's meaning changes, a required field is added, or an
/// enum gains or loses a variant — so provider drift surfaces as a visible
/// validation error rather than a silent misinterpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SchemaVersion(pub u32);

impl SchemaVersion {
    pub const CURRENT: Self = Self(1);

    pub fn is_compatible(self, other: Self) -> bool {
        self.0 == other.0
    }
}

impl Default for SchemaVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}
