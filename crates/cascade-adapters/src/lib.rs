mod prometheus;

use anyhow::Result;
use async_trait::async_trait;
use cascade_core::{ApiStat, ObservedEdge};

pub use prometheus::PrometheusSource;

/// Where Cascade learns topology and edge health. Implementations read from an
/// observability stack that already exists; Cascade stores none of this data.
#[async_trait]
pub trait TopologySource: Send + Sync {
    fn name(&self) -> &str;
    /// Edges seen over the source's current window.
    async fn edges(&self) -> Result<Vec<ObservedEdge>>;
    /// One instant-query value (for declared business metrics). `None` when the query returns nothing.
    async fn scalar(&self, query: &str) -> Result<Option<f64>>;
    /// APIs served by `service` (inbound), for the edge drill-down.
    async fn apis(&self, service: &str) -> Result<Vec<ApiStat>>;
}
