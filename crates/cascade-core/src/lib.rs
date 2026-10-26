//! Pure domain logic for Cascade: no I/O, no async.

mod graph;
mod health;
mod model;
mod observed;
mod reach;
mod types;

pub use graph::Graph;
pub use health::HealthTracker;
pub use model::{Dep, FeatureSpec, Model, ServiceSpec};
pub use observed::{ApiStat, ObservedEdge, Thresholds};
pub use reach::{evaluate, Faults, Outcome, Reach};
pub use types::{DepKind, Impact, NodeKind, ServiceId, Tier};
