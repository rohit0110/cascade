use cascade_core::{DepKind, Impact, NodeKind, Tier};
use cascade_registry::DriftReport;
use serde::Serialize;

/// How far a statement can be trusted, from how many independent sources agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Declared in a cascade.yaml and seen in telemetry.
    High,
    /// Seen in telemetry only: real, but nobody has said what it means.
    Medium,
    /// Declared only: nothing in telemetry backs it up.
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Declared,
    Observed,
    Both,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDto {
    pub id: String,
    pub owner: Option<String>,
    pub declared: bool,
    pub kind: NodeKind,
    pub impact: Impact,
    /// Where the failure behind this node's impact originates.
    pub causes: Vec<String>,
    /// Most critical tier among impacted features hosted here.
    pub tier: Option<Tier>,
    pub features: Vec<String>,
    /// Inbound traffic aggregated over all callers.
    pub rps: Option<f64>,
    pub error_ratio: Option<f64>,
    /// Slowest p95 among inbound edges.
    pub p95_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeDto {
    pub client: String,
    pub server: String,
    pub kind: DepKind,
    pub origin: Origin,
    pub confidence: Confidence,
    pub health: Impact,
    pub rps: Option<f64>,
    pub error_ratio: Option<f64>,
    pub p95_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureDto {
    pub id: String,
    pub name: String,
    pub tier: Tier,
    pub owner: String,
    pub services: Vec<String>,
    pub impact: Impact,
    pub causes: Vec<String>,
    pub confidence: Confidence,
}

/// A declared business metric and its latest value.
#[derive(Debug, Clone, Serialize)]
pub struct MetricDto {
    pub feature: String,
    pub service: String,
    pub name: String,
    pub unit: Option<String>,
    pub value: Option<f64>,
    /// Why there is no value: no query declared, unsupported source, or the query failed.
    pub error: Option<String>,
}

/// Everything the API serves, computed once per poll.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Snapshot {
    pub version: u64,
    pub as_of_unix: u64,
    pub source: String,
    pub last_error: Option<String>,
    pub poll_errors_total: u64,
    pub nodes: Vec<NodeDto>,
    pub edges: Vec<EdgeDto>,
    pub features: Vec<FeatureDto>,
    pub metrics: Vec<MetricDto>,
    pub drift: DriftReport,
    pub registry_warnings: Vec<String>,
}
