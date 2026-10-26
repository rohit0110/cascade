mod registry;
mod schema;
mod verify;

pub use registry::{Registry, RegistryError};
pub use schema::{DepDecl, FeatureDecl, MetricDecl, ServiceFile};
pub use verify::{verify, DriftReport, EdgeRef, VerifyOptions};
