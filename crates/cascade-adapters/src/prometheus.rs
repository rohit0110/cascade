use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use cascade_core::{ApiStat, ObservedEdge};
use serde::Deserialize;

use crate::TopologySource;

/// Reads the OpenTelemetry Collector `servicegraph` and `spanmetrics` connector
/// series from a Prometheus-compatible query API.
pub struct PrometheusSource {
    base: String,
    window: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct Resp {
    status: String,
    #[serde(default)]
    error: Option<String>,
    data: Option<Data>,
}

#[derive(Deserialize)]
struct Data {
    result: Vec<Sample>,
}

#[derive(Deserialize)]
struct Sample {
    metric: HashMap<String, String>,
    value: (f64, String),
}

impl PrometheusSource {
    /// `window` is a PromQL range such as `2m`; it must span a few scrape/push intervals.
    pub fn new(base: &str, window: &str) -> Result<Self> {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
        Ok(Self { base: base.trim_end_matches('/').to_owned(), window: window.to_owned(), http })
    }

    async fn query(&self, promql: &str) -> Result<Vec<Sample>> {
        let url = format!("{}/api/v1/query", self.base);
        let resp: Resp = self
            .http
            .get(&url)
            .query(&[("query", promql)])
            .send()
            .await
            .with_context(|| format!("querying {url}"))?
            .error_for_status()?
            .json()
            .await?;
        if resp.status != "success" {
            bail!("prometheus error: {}", resp.error.unwrap_or_default());
        }
        Ok(resp.data.ok_or_else(|| anyhow!("prometheus response had no data"))?.result)
    }
}

fn val(s: &Sample) -> Option<f64> {
    s.value.1.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn label<'a>(s: &'a Sample, k: &str) -> Option<&'a str> {
    s.metric.get(k).map(String::as_str)
}

/// Joins request-rate, failed-rate and p95 series (keyed by label tuple) into rows.
fn join_edges(total: &[Sample], failed: &[Sample], p95: &[Sample], volume: &[Sample]) -> Vec<ObservedEdge> {
    let key = |s: &Sample| Some((label(s, "client")?.to_owned(), label(s, "server")?.to_owned()));
    let failed: HashMap<_, _> = failed.iter().filter_map(|s| Some((key(s)?, val(s)?))).collect();
    let p95: HashMap<_, _> = p95.iter().filter_map(|s| Some((key(s)?, val(s)?))).collect();
    let volume: HashMap<_, _> = volume.iter().filter_map(|s| Some((key(s)?, val(s)?))).collect();
    let mut out: Vec<ObservedEdge> = total
        .iter()
        .filter_map(|s| {
            let k = key(s)?;
            let rps = val(s)?;
            let f = failed.get(&k).copied().unwrap_or(0.0);
            Some(ObservedEdge {
                error_ratio: if rps > 0.0 { (f / rps).clamp(0.0, 1.0) } else { 0.0 },
                p95_ms: p95.get(&k).map(|s| s * 1000.0),
                volume: volume.get(&k).copied(),
                client: k.0,
                server: k.1,
                rps,
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.client, &a.server).cmp(&(&b.client, &b.server)));
    out
}

fn join_apis(total: &[Sample], errors: &[Sample], p95: &[Sample]) -> Vec<ApiStat> {
    let key = |s: &Sample| label(s, "span_name").map(str::to_owned);
    let errors: HashMap<_, _> = errors.iter().filter_map(|s| Some((key(s)?, val(s)?))).collect();
    let p95: HashMap<_, _> = p95.iter().filter_map(|s| Some((key(s)?, val(s)?))).collect();
    let mut by_name: BTreeMap<String, ApiStat> = BTreeMap::new();
    for s in total {
        let (Some(name), Some(rps)) = (key(s), val(s)) else { continue };
        let e = errors.get(&name).copied().unwrap_or(0.0);
        by_name.insert(
            name.clone(),
            ApiStat {
                rps,
                error_ratio: if rps > 0.0 { (e / rps).clamp(0.0, 1.0) } else { 0.0 },
                p95_ms: p95.get(&name).copied(),
                name,
            },
        );
    }
    by_name.into_values().collect()
}

#[async_trait]
impl TopologySource for PrometheusSource {
    fn name(&self) -> &str {
        "prometheus"
    }

    async fn edges(&self) -> Result<Vec<ObservedEdge>> {
        let w = &self.window;
        let q_total = format!("sum by (client, server) (rate(traces_service_graph_request_total[{w}]))");
        let q_failed =
            format!("sum by (client, server) (rate(traces_service_graph_request_failed_total[{w}]))");
        let q_p95 = format!(
            "histogram_quantile(0.95, sum by (client, server, le) \
             (rate(traces_service_graph_request_server_seconds_bucket[{w}])))"
        );
        let q_vol = format!("sum by (client, server) (increase(traces_service_graph_request_total[{w}]))");
        let (total, failed, p95, vol) = tokio::try_join!(
            self.query(&q_total),
            self.query(&q_failed),
            self.query(&q_p95),
            self.query(&q_vol)
        )?;
        Ok(join_edges(&total, &failed, &p95, &vol))
    }

    async fn scalar(&self, query: &str) -> Result<Option<f64>> {
        Ok(self.query(query).await?.first().and_then(val))
    }

    async fn apis(&self, service: &str) -> Result<Vec<ApiStat>> {
        if service.contains(['"', '\\', '\n']) {
            bail!("invalid service name");
        }
        let w = &self.window;
        let sel = format!("service_name=\"{service}\",span_kind=\"SPAN_KIND_SERVER\"");
        let q_total =
            format!("sum by (span_name) (rate(traces_span_metrics_calls_total{{{sel}}}[{w}]))");
        let q_errors = format!(
            "sum by (span_name) (rate(traces_span_metrics_calls_total\
             {{{sel},status_code=\"STATUS_CODE_ERROR\"}}[{w}]))"
        );
        let q_p95 = format!(
            "histogram_quantile(0.95, sum by (span_name, le) \
             (rate(traces_span_metrics_duration_milliseconds_bucket{{{sel}}}[{w}])))"
        );
        let (total, errors, p95) =
            tokio::try_join!(self.query(&q_total), self.query(&q_errors), self.query(&q_p95))?;
        Ok(join_apis(&total, &errors, &p95))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(labels: &[(&str, &str)], v: &str) -> Sample {
        Sample {
            metric: labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            value: (0.0, v.to_string()),
        }
    }

    #[test]
    fn joins_edges_with_missing_failed_and_nan_latency() {
        let total = vec![
            s(&[("client", "a"), ("server", "b")], "10"),
            s(&[("client", "a"), ("server", "c")], "4"),
        ];
        let failed = vec![s(&[("client", "a"), ("server", "c")], "2")];
        let p95 = vec![
            s(&[("client", "a"), ("server", "b")], "0.25"),
            s(&[("client", "a"), ("server", "c")], "NaN"),
        ];
        let vol = vec![s(&[("client", "a"), ("server", "b")], "600")];
        let e = join_edges(&total, &failed, &p95, &vol);
        assert_eq!((e[0].volume, e[1].volume), (Some(600.0), None));
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].server.as_str(), e[0].error_ratio, e[0].p95_ms), ("b", 0.0, Some(250.0)));
        assert_eq!((e[1].server.as_str(), e[1].error_ratio, e[1].p95_ms), ("c", 0.5, None));
    }

    #[test]
    fn joins_apis() {
        let total = vec![s(&[("span_name", "GET /x")], "5"), s(&[("span_name", "GET /a")], "1")];
        let errors = vec![s(&[("span_name", "GET /x")], "1")];
        let a = join_apis(&total, &errors, &[]);
        assert_eq!(a[0].name, "GET /a");
        assert_eq!(a[1].error_ratio, 0.2);
    }
}
