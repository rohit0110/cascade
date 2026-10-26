use serde::{Deserialize, Serialize};

use crate::types::Impact;

/// One caller -> callee relationship seen in telemetry, over the last window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservedEdge {
    pub client: String,
    pub server: String,
    pub rps: f64,
    pub error_ratio: f64,
    /// Requests in the window. `None` when the source can't say (then the volume gate is skipped).
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub p95_ms: Option<f64>,
}

/// One API (route/RPC) served by a service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiStat {
    pub name: String,
    pub rps: f64,
    pub error_ratio: f64,
    #[serde(default)]
    pub p95_ms: Option<f64>,
}

/// Maps an error ratio to an impact level. Traffic below `min_rps` is too thin to judge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub degraded_error_ratio: f64,
    pub down_error_ratio: f64,
    pub min_rps: f64,
    /// Too few requests in the window to trust an error ratio (2 requests, 1 failed, is not 50%).
    pub min_requests: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { degraded_error_ratio: 0.05, down_error_ratio: 0.5, min_rps: 0.02, min_requests: 10.0 }
    }
}

impl Thresholds {
    pub fn classify(&self, error_ratio: f64, rps: f64, volume: Option<f64>) -> Impact {
        if rps < self.min_rps || error_ratio.is_nan() || volume.is_some_and(|v| v < self.min_requests) {
            Impact::None
        } else if error_ratio >= self.down_error_ratio {
            Impact::Down
        } else if error_ratio >= self.degraded_error_ratio {
            Impact::Degraded
        } else {
            Impact::None
        }
    }
}
