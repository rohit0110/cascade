use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use cascade_adapters::TopologySource;
use cascade_core::{ApiStat, ObservedEdge};
use cascade_registry::{Registry, ServiceFile};
use cascade_server::api::{router, AppState};
use cascade_server::engine::{Engine, EngineConfig};
use cascade_server::poller::now_unix;
use cascade_server::{metrics, state::Snapshot};
use http_body_util::BodyExt;
use tokio::sync::watch;
use tower::ServiceExt;

struct Fake;

#[async_trait]
impl TopologySource for Fake {
    fn name(&self) -> &str {
        "fake"
    }
    async fn edges(&self) -> Result<Vec<ObservedEdge>> {
        Ok(vec![])
    }
    async fn scalar(&self, q: &str) -> Result<Option<f64>> {
        match q {
            "fail" => anyhow::bail!("boom"),
            "empty" => Ok(None),
            _ => Ok(Some(42.0)),
        }
    }
    async fn apis(&self, service: &str) -> Result<Vec<ApiStat>> {
        if service == "boom" {
            anyhow::bail!("prometheus down");
        }
        Ok(vec![
            ApiStat { name: "POST /charge".into(), rps: 2.0, error_ratio: 1.0, p95_ms: Some(10.0) },
            ApiStat { name: "GET /health".into(), rps: 9.0, error_ratio: 0.0, p95_ms: None },
        ])
    }
}

fn snapshot(as_of: u64) -> Snapshot {
    let reg = Registry::from_files(vec![(
        PathBuf::from("t"),
        serde_yaml::from_str::<ServiceFile>(
            "service: checkout\nowner: pay\nfeatures:\n  - {id: checkout, tier: P0, depends_on: [{service: payment}]}\n",
        )
        .unwrap(),
    )])
    .unwrap();
    let obs = vec![ObservedEdge { client: "checkout".into(), server: "payment".into(), rps: 5.0, error_ratio: 1.0, volume: None, p95_ms: None }];
    let mut eng = Engine::new(EngineConfig { raise_after: 1, ..EngineConfig::default() });
    let mut s = eng.evaluate(&reg, &obs);
    s.version = 7;
    s.as_of_unix = as_of;
    s.source = "fake".into();
    s
}

fn app(s: Snapshot) -> axum::Router {
    let (tx, rx) = watch::channel(Arc::new(s));
    std::mem::forget(tx); // keep the channel open for the test's lifetime
    router(AppState { rx, source: Arc::new(Fake), poll_every: Duration::from_secs(15) })
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let r = app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
    let status = r.status();
    let body = r.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn graph_impact_and_staleness() {
    let a = app(snapshot(now_unix()));
    let (st, body) = get(&a, "/api/graph").await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["stale"], false);
    assert_eq!(v["version"], 7);
    assert_eq!(v["edges"][0]["health"], "down");

    let (_, body) = get(&a, "/api/impact").await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["features"][0]["id"], "checkout");
    assert_eq!(v["features"][0]["causes"][0], "payment");

    let old = app(snapshot(now_unix() - 3600));
    let (_, body) = get(&old, "/api/graph").await;
    assert_eq!(serde_json::from_str::<serde_json::Value>(&body).unwrap()["stale"], true);
    let never = app(Snapshot::default());
    let (_, body) = get(&never, "/api/graph").await;
    assert_eq!(serde_json::from_str::<serde_json::Value>(&body).unwrap()["stale"], true);
}

#[tokio::test]
async fn edge_apis_search_and_errors() {
    let a = app(snapshot(now_unix()));
    let (_, body) = get(&a, "/api/edges/checkout/payment/apis?q=CHARGE").await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["apis"].as_array().unwrap().len(), 1);
    assert_eq!(v["apis"][0]["name"], "POST /charge");

    let (_, body) = get(&a, "/api/edges/checkout/payment/apis").await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["apis"][0]["name"], "POST /charge", "failing APIs sort first");

    let (st, _) = get(&a, "/api/edges/x/boom/apis").await;
    assert_eq!(st, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn metrics_and_nodegraph_and_misc() {
    let a = app(snapshot(1000));
    let (st, body) = get(&a, "/metrics").await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains("cascade_feature_impact{feature=\"checkout\",tier=\"P0\",owner=\"pay\"} 2"));
    assert!(body.contains("cascade_edge_health{client=\"checkout\",server=\"payment\"} 2"));
    assert!(body.contains("cascade_snapshot_timestamp_seconds 1000"));

    let (_, body) = get(&a, "/api/grafana/nodegraph").await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["edges"][0]["source"], "checkout");
    assert_eq!(v["nodes"].as_array().unwrap().len(), 2);

    assert_eq!(get(&a, "/healthz").await.1, "ok");
    assert_eq!(get(&a, "/api/drift").await.0, StatusCode::OK);
    assert_eq!(get(&a, "/nope.js").await.0, StatusCode::NOT_FOUND);
}

#[test]
fn metric_labels_are_escaped() {
    let mut s = snapshot(1);
    s.features[0].owner = "a\"b\\c".into();
    assert!(metrics::render(&s).contains("owner=\"a\\\"b\\\\c\""));
}

#[tokio::test]
async fn business_metrics_are_evaluated_per_metric() {
    let reg = Registry::from_files(vec![(
        PathBuf::from("t"),
        serde_yaml::from_str::<ServiceFile>(
            "service: pay\nowner: o\nfeatures:\n  - id: f\n    tier: P0\n    metrics:\n      - {name: ok, query: q, unit: /min}\n      - {name: nothing, query: empty}\n      - {name: broken, query: fail}\n      - {name: noquery}\n      - {name: other, source: datadog, query: x}\n",
        )
        .unwrap(),
    )])
    .unwrap();
    let source: Arc<dyn TopologySource> = Arc::new(Fake);
    let m = cascade_server::poller::eval_metrics(&source, &reg).await;
    let by = |n: &str| m.iter().find(|x| x.name == n).unwrap();
    assert_eq!(m.len(), 5);
    assert_eq!((by("ok").value, by("ok").unit.as_deref()), (Some(42.0), Some("/min")));
    assert_eq!(by("nothing").error.as_deref(), Some("query returned no data"));
    assert!(by("broken").error.as_ref().unwrap().contains("boom"));
    assert_eq!(by("noquery").error.as_deref(), Some("no query declared"));
    assert!(by("other").error.as_ref().unwrap().contains("unsupported source"));
    assert_eq!(m[0].name, "ok", "declared order is kept");

    let mut s = snapshot(now_unix());
    s.metrics = m;
    let a = app(s);
    let (_, body) = get(&a, "/api/metrics").await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["metrics"][0]["value"], 42.0);
    let (_, prom) = get(&a, "/metrics").await;
    assert!(prom.contains("cascade_business_metric{feature=\"f\",metric=\"ok\"} 42"));
    assert!(!prom.contains("metric=\"broken\""));
}
