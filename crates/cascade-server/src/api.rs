use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use cascade_adapters::TopologySource;
use cascade_core::Impact;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tokio_stream::StreamExt;

use crate::metrics;
use crate::poller::now_unix;
use crate::state::{EdgeDto, FeatureDto, MetricDto, NodeDto, Snapshot};

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct Assets;

#[derive(Clone)]
pub struct AppState {
    pub rx: watch::Receiver<Arc<Snapshot>>,
    pub source: Arc<dyn TopologySource>,
    pub poll_every: Duration,
}

#[derive(Serialize)]
struct Meta {
    version: u64,
    as_of_unix: u64,
    age_seconds: u64,
    /// True when the data is more than three poll intervals old.
    stale: bool,
    source: String,
    last_error: Option<String>,
}

impl AppState {
    fn snap(&self) -> Arc<Snapshot> {
        self.rx.borrow().clone()
    }

    fn meta(&self, s: &Snapshot) -> Meta {
        let age = now_unix().saturating_sub(s.as_of_unix);
        Meta {
            version: s.version,
            as_of_unix: s.as_of_unix,
            age_seconds: age,
            stale: s.as_of_unix == 0 || age > self.poll_every.as_secs() * 3,
            source: s.source.clone(),
            last_error: s.last_error.clone(),
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/metrics", get(metrics_h))
        .route("/api/graph", get(graph))
        .route("/api/impact", get(impact))
        .route("/api/features", get(features))
        .route("/api/drift", get(drift))
        .route("/api/metrics", get(business_metrics))
        .route("/api/events", get(events))
        .route("/api/edges/:client/:server/apis", get(edge_apis))
        .route("/api/grafana/nodegraph", get(nodegraph))
        .fallback(ui)
        .with_state(state)
}

#[derive(Serialize)]
struct GraphResp {
    #[serde(flatten)]
    meta: Meta,
    nodes: Vec<NodeDto>,
    edges: Vec<EdgeDto>,
}

async fn graph(State(st): State<AppState>) -> Json<GraphResp> {
    let s = st.snap();
    Json(GraphResp { meta: st.meta(&s), nodes: s.nodes.clone(), edges: s.edges.clone() })
}

#[derive(Serialize)]
struct FeaturesResp {
    #[serde(flatten)]
    meta: Meta,
    features: Vec<FeatureDto>,
}

/// Only the features that are currently affected, worst first.
async fn impact(State(st): State<AppState>) -> Json<FeaturesResp> {
    let s = st.snap();
    let features = s.features.iter().filter(|f| f.impact > Impact::None).cloned().collect();
    Json(FeaturesResp { meta: st.meta(&s), features })
}

async fn features(State(st): State<AppState>) -> Json<FeaturesResp> {
    let s = st.snap();
    Json(FeaturesResp { meta: st.meta(&s), features: s.features.clone() })
}

#[derive(Serialize)]
struct MetricsResp {
    #[serde(flatten)]
    meta: Meta,
    metrics: Vec<MetricDto>,
}

/// Declared business metrics with their latest values.
async fn business_metrics(State(st): State<AppState>) -> Json<MetricsResp> {
    let s = st.snap();
    Json(MetricsResp { meta: st.meta(&s), metrics: s.metrics.clone() })
}

async fn drift(State(st): State<AppState>) -> Json<serde_json::Value> {
    let s = st.snap();
    Json(json!({ "drift": s.drift, "registry_warnings": s.registry_warnings }))
}

async fn metrics_h(State(st): State<AppState>) -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        metrics::render(&st.snap()),
    )
        .into_response()
}

/// Notifies the UI that a new snapshot exists; the UI then refetches.
async fn events(
    State(st): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = WatchStream::new(st.rx.clone()).map(|s| {
        Ok(Event::default()
            .event("snapshot")
            .data(json!({ "version": s.version, "as_of_unix": s.as_of_unix }).to_string()))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct ApiQuery {
    q: Option<String>,
}

/// APIs served by the callee of an edge. Servicegraph carries no route label, so
/// this lists the callee's inbound APIs across all callers.
async fn edge_apis(
    State(st): State<AppState>,
    Path((client, server)): Path<(String, String)>,
    Query(q): Query<ApiQuery>,
) -> Response {
    match st.source.apis(&server).await {
        Ok(mut apis) => {
            if let Some(q) = q.q.as_deref().map(str::to_lowercase).filter(|q| !q.is_empty()) {
                apis.retain(|a| a.name.to_lowercase().contains(&q));
            }
            apis.sort_by(|a, b| b.error_ratio.total_cmp(&a.error_ratio).then(b.rps.total_cmp(&a.rps)));
            Json(json!({
                "client": client, "server": server, "apis": apis,
                "scope": "all callers of server",
            }))
            .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

/// Field names follow Grafana's Node Graph panel schema, so the response can be
/// mapped by a JSON datasource or the Cascade datasource plugin.
async fn nodegraph(State(st): State<AppState>) -> Json<serde_json::Value> {
    let s = st.snap();
    let nodes: Vec<_> = s
        .nodes
        .iter()
        .map(|n| {
            let failed = match n.impact {
                Impact::None => 0.0,
                Impact::Degraded => 0.5,
                Impact::Down => 1.0,
            };
            json!({
                "id": n.id, "title": n.id,
                "subTitle": n.owner.clone().unwrap_or_else(|| "unowned".into()),
                "mainStat": n.impact.as_str(),
                "secondaryStat": n.tier.map(|t| format!("{t:?}")).unwrap_or_default(),
                "arc__failed": failed, "arc__ok": 1.0 - failed,
            })
        })
        .collect();
    let edges: Vec<_> = s
        .edges
        .iter()
        .map(|e| {
            json!({
                "id": format!("{}->{}", e.client, e.server),
                "source": e.client, "target": e.server,
                "mainStat": e.error_ratio.map(|r| format!("{:.1}% err", r * 100.0)).unwrap_or_default(),
                "detail__health": e.health.as_str(),
                "detail__kind": e.kind,
                "detail__confidence": e.confidence,
            })
        })
        .collect();
    Json(json!({ "nodes": nodes, "edges": edges }))
}

async fn ui(uri: Uri) -> Response {
    let p = uri.path().trim_start_matches('/');
    let p = if p.is_empty() { "index.html" } else { p };
    let file = Assets::get(p).map(|f| (p.to_owned(), f)).or_else(|| {
        // SPA fallback for client routes, but never for missing asset files.
        (!p.contains('.')).then(|| Assets::get("index.html").map(|f| ("index.html".to_owned(), f))).flatten()
    });
    match file {
        Some((name, f)) => {
            let mime = mime_of(&name);
            ([(header::CONTENT_TYPE, mime)], f.data.into_owned()).into_response()
        }
        None if p == "index.html" => (
            StatusCode::NOT_FOUND,
            "UI is not built into this binary. The JSON API is available under /api/.",
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn mime_of(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}
