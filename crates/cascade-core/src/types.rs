use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServiceId(pub String);

impl From<&str> for ServiceId {
    fn from(s: &str) -> Self {
        ServiceId(s.to_owned())
    }
}

impl fmt::Display for ServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a node is, so UIs can draw databases differently from services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    #[default]
    Service,
    Gateway,
    Database,
    Cache,
    Queue,
    External,
    /// Feature flags, config stores.
    Config,
}

/// Business criticality. `P0` is the most critical, so `P0 < P3` in ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    P0,
    P1,
    P2,
    P3,
}

/// Whether a caller can keep working when the callee fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DepKind {
    Hard,
    Soft,
}

/// Severity of a failure, ordered `None < Degraded < Down`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Impact {
    #[default]
    None,
    Degraded,
    Down,
}

impl Impact {
    pub fn as_str(self) -> &'static str {
        match self {
            Impact::None => "none",
            Impact::Degraded => "degraded",
            Impact::Down => "down",
        }
    }

    pub fn level(self) -> u8 {
        self as u8
    }


    /// What a caller suffers when a callee has `self`, given the dependency kind.
    /// A soft dependency never takes the caller fully down.
    pub fn through(self, kind: DepKind) -> Impact {
        match kind {
            DepKind::Hard => self,
            DepKind::Soft => self.min(Impact::Degraded),
        }
    }
}
