use cascade_core::{DepKind, NodeKind, Tier};
use serde::{Deserialize, Serialize};

fn is_service(k: &NodeKind) -> bool {
    *k == NodeKind::Service
}

fn prometheus() -> String {
    "prometheus".into()
}

fn hard() -> DepKind {
    DepKind::Hard
}

/// One `cascade.yaml`: a service, its owner, the features it serves and what it calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceFile {
    pub service: String,
    pub owner: String,
    /// Drawn differently in the UI (database, cache, queue...). Defaults to `service`.
    #[serde(default, skip_serializing_if = "is_service")]
    pub kind: NodeKind,
    /// Dependencies of the service as a whole (e.g. its database).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<DepDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<FeatureDecl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureDecl {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub tier: Tier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entrypoints: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<DepDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<MetricDecl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepDecl {
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(default = "hard")]
    pub kind: DepKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricDecl {
    pub name: String,
    #[serde(default = "prometheus")]
    pub source: String,
    /// Instant query returning one value, e.g. `sum(rate(orders_total[5m])) * 60`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}
