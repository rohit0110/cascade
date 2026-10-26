use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cascade_adapters::TopologySource;
use cascade_registry::Registry;
use tokio::sync::watch;
use tokio::time::{interval, MissedTickBehavior};

use crate::engine::{Engine, EngineConfig};
use crate::state::{MetricDto, Snapshot};

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Evaluates every business metric declared in the registry. A failure only blanks
/// that one metric (with the reason), never the whole snapshot.
pub async fn eval_metrics(source: &Arc<dyn TopologySource>, reg: &Registry) -> Vec<MetricDto> {
    let mut set = tokio::task::JoinSet::new();
    for (i, (feature, service, m)) in reg.metrics().into_iter().enumerate() {
        let source = source.clone();
        set.spawn(async move {
            let (value, error) = match (m.source.as_str(), m.query.as_deref()) {
                ("prometheus", Some(q)) => match source.scalar(q).await {
                    Ok(Some(v)) => (Some(v), None),
                    Ok(None) => (None, Some("query returned no data".to_string())),
                    Err(e) => (None, Some(format!("{e:#}"))),
                },
                ("prometheus", None) => (None, Some("no query declared".to_string())),
                (other, _) => (None, Some(format!("unsupported source `{other}`"))),
            };
            (i, MetricDto { feature, service, name: m.name, unit: m.unit, value, error })
        });
    }
    let mut out = Vec::new();
    while let Some(r) = set.join_next().await {
        if let Ok(x) = r {
            out.push(x);
        }
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, m)| m).collect()
}

/// Polls the topology source forever, publishing a new snapshot each round.
/// A failed poll keeps the last good picture and records the error, so the UI
/// can show data as of a time rather than nothing.
pub async fn run(
    registry_dir: PathBuf,
    every: Duration,
    cfg: EngineConfig,
    source: Arc<dyn TopologySource>,
    tx: watch::Sender<Arc<Snapshot>>,
) {
    let mut engine = Engine::new(cfg);
    let mut registry: Option<Registry> = None;
    let mut version = 0u64;
    let mut errors = 0u64;
    let mut tick = interval(every);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tick.tick().await;
        let mut problem: Option<String> = None;

        match Registry::load_dir(&registry_dir) {
            Ok(r) => registry = Some(r),
            Err(e) => {
                tracing::error!("registry: {e}");
                problem = Some(format!("registry: {e}"));
            }
        }
        let reg = registry.clone().unwrap_or_default();

        match source.edges().await {
            Ok(edges) => {
                version += 1;
                let mut snap = engine.evaluate(&reg, &edges);
                snap.metrics = eval_metrics(&source, &reg).await;
                snap.version = version;
                snap.as_of_unix = now_unix();
                snap.source = source.name().to_owned();
                snap.last_error = problem;
                if snap.last_error.is_some() {
                    errors += 1;
                }
                snap.poll_errors_total = errors;
                let _ = tx.send(Arc::new(snap));
            }
            Err(e) => {
                tracing::error!("poll: {e:#}");
                errors += 1;
                let mut snap = (**tx.borrow()).clone();
                snap.last_error = Some(format!("{e:#}"));
                snap.poll_errors_total = errors;
                let _ = tx.send(Arc::new(snap));
            }
        }
    }
}
