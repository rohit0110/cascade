use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use cascade_adapters::PrometheusSource;
use cascade_core::Thresholds;
use cascade_server::api::{router, AppState};
use cascade_server::engine::EngineConfig;
use cascade_server::poller;
use cascade_server::state::Snapshot;
use clap::Parser;
use tokio::sync::watch;

#[derive(Parser)]
#[command(version, about = "Cascade: business impact on top of your existing observability")]
struct Args {
    #[arg(long, env = "CASCADE_LISTEN", default_value = "0.0.0.0:7070")]
    listen: SocketAddr,
    /// Directory of cascade.yaml files; re-read every poll.
    #[arg(long, env = "CASCADE_REGISTRY", default_value = "/registry")]
    registry: PathBuf,
    #[arg(long, env = "CASCADE_PROMETHEUS", default_value = "http://prometheus:9090")]
    prometheus: String,
    /// PromQL range for rates; should span several metric push intervals.
    #[arg(long, env = "CASCADE_WINDOW", default_value = "2m")]
    window: String,
    #[arg(long, env = "CASCADE_POLL_SECS", default_value_t = 15)]
    poll_secs: u64,
    /// Services to disregard (telemetry artifacts, load generators).
    #[arg(long, env = "CASCADE_IGNORE", value_delimiter = ',', default_value = "unknown")]
    ignore: Vec<String>,
    #[arg(long, env = "CASCADE_DEGRADED_ERROR_RATIO", default_value_t = 0.05)]
    degraded_error_ratio: f64,
    #[arg(long, env = "CASCADE_DOWN_ERROR_RATIO", default_value_t = 0.5)]
    down_error_ratio: f64,
    #[arg(long, env = "CASCADE_MIN_RPS", default_value_t = 0.02)]
    min_rps: f64,
    /// Ignore error ratios computed from fewer requests than this in the window.
    #[arg(long, env = "CASCADE_MIN_REQUESTS", default_value_t = 10.0)]
    min_requests: f64,
    #[arg(long, env = "CASCADE_RAISE_AFTER", default_value_t = 2)]
    raise_after: u32,
    #[arg(long, env = "CASCADE_CLEAR_AFTER", default_value_t = 3)]
    clear_after: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let a = Args::parse();

    let source: Arc<dyn cascade_adapters::TopologySource> =
        Arc::new(PrometheusSource::new(&a.prometheus, &a.window)?);
    let cfg = EngineConfig {
        thresholds: Thresholds {
            degraded_error_ratio: a.degraded_error_ratio,
            down_error_ratio: a.down_error_ratio,
            min_rps: a.min_rps,
            min_requests: a.min_requests,
        },
        ignore: a.ignore.into_iter().filter(|s| !s.is_empty()).collect::<BTreeSet<_>>(),
        raise_after: a.raise_after,
        clear_after: a.clear_after,
    };
    let every = Duration::from_secs(a.poll_secs.max(1));
    let (tx, rx) = watch::channel(Arc::new(Snapshot::default()));
    tokio::spawn(poller::run(a.registry.clone(), every, cfg, source.clone(), tx));

    let app = router(AppState { rx, source, poll_every: every });
    let listener = tokio::net::TcpListener::bind(a.listen).await?;
    tracing::info!("cascade listening on {}", a.listen);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
