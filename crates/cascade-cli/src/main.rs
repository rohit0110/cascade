use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use cascade_adapters::{PrometheusSource, TopologySource};
use cascade_core::{evaluate, DepKind, Faults, Impact, ObservedEdge, ServiceId};
use cascade_registry::{verify, DepDecl, Registry, ServiceFile, VerifyOptions};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cascade", version, about = "Business impact on top of your existing observability")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check declared dependencies against what telemetry shows. Exits 1 on undeclared dependencies.
    Verify {
        #[arg(long, default_value = ".")]
        registry: PathBuf,
        #[command(flatten)]
        obs: ObservedArgs,
        /// Also fail on declared-but-unobserved edges and callers with no cascade.yaml.
        #[arg(long)]
        strict: bool,
    },
    /// Write starter cascade.yaml files from observed callers (owners and features left for humans).
    Init {
        #[arg(long)]
        out: PathBuf,
        #[command(flatten)]
        obs: ObservedArgs,
        /// Overwrite files that already exist.
        #[arg(long)]
        force: bool,
    },
    /// What breaks if these services fail? Uses declared dependencies only.
    Simulate {
        #[arg(long, default_value = ".")]
        registry: PathBuf,
        #[arg(long, value_delimiter = ',')]
        down: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        degraded: Vec<String>,
    },
}

#[derive(Args)]
struct ObservedArgs {
    /// Prometheus base URL with servicegraph metrics.
    #[arg(long, env = "CASCADE_PROMETHEUS", conflicts_with = "observed")]
    prometheus: Option<String>,
    #[arg(long, default_value = "10m")]
    window: String,
    /// JSON file of observed edges instead of querying (offline CI).
    #[arg(long)]
    observed: Option<PathBuf>,
    #[arg(long, value_delimiter = ',', default_value = "unknown")]
    ignore: Vec<String>,
    #[arg(long, default_value_t = 0.0)]
    min_rps: f64,
}

impl ObservedArgs {
    async fn edges(&self) -> Result<Vec<ObservedEdge>> {
        match (&self.prometheus, &self.observed) {
            (Some(url), None) => PrometheusSource::new(url, &self.window)?.edges().await,
            (None, Some(f)) => {
                let t = fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?;
                Ok(serde_json::from_str(&t).context("observed file must be a JSON array of edges")?)
            }
            _ => bail!("give exactly one of --prometheus or --observed"),
        }
    }

    fn ignore(&self) -> BTreeSet<String> {
        self.ignore.iter().filter(|s| !s.is_empty()).cloned().collect()
    }
}

fn load(dir: &Path) -> Result<Registry> {
    let r = Registry::load_dir(dir)?;
    for w in &r.warnings {
        eprintln!("warning: {w}");
    }
    Ok(r)
}

async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.cmd {
        Cmd::Verify { registry, obs, strict } => {
            let reg = load(&registry)?;
            let observed = obs.edges().await?;
            let r = verify(&reg, &observed, &VerifyOptions { ignore: obs.ignore(), min_rps: obs.min_rps });
            for e in &r.undeclared {
                println!("error: undeclared dependency {} -> {} (seen in telemetry, not in {}'s cascade.yaml)", e.client, e.server, e.client);
            }
            for e in &r.unobserved {
                println!("warning: declared dependency {} -> {} not seen in the window", e.client, e.server);
            }
            for s in &r.undeclared_services {
                println!("warning: {s} makes calls but has no cascade.yaml");
            }
            let failed = !r.undeclared.is_empty()
                || (strict && (!r.unobserved.is_empty() || !r.undeclared_services.is_empty()));
            println!(
                "{}: {} undeclared, {} unobserved, {} services without cascade.yaml",
                if failed { "FAIL" } else { "ok" },
                r.undeclared.len(), r.unobserved.len(), r.undeclared_services.len()
            );
            Ok(if failed { ExitCode::from(1) } else { ExitCode::SUCCESS })
        }
        Cmd::Init { out, obs, force } => {
            let observed = obs.edges().await?;
            let ignore = obs.ignore();
            let mut deps: std::collections::BTreeMap<String, BTreeSet<String>> = Default::default();
            for e in observed.iter().filter(|e| {
                !ignore.contains(&e.client) && !ignore.contains(&e.server) && e.rps >= obs.min_rps
            }) {
                deps.entry(e.client.clone()).or_default().insert(e.server.clone());
            }
            fs::create_dir_all(&out)?;
            for (svc, servers) in deps {
                let path = out.join(format!("{svc}.yaml"));
                if path.exists() && !force {
                    println!("skip {} (exists; use --force)", path.display());
                    continue;
                }
                let f = ServiceFile {
                    service: svc.clone(),
                    owner: "TODO-owner".into(),
                    kind: Default::default(),
                    // Hard is the safe default; whether a call is optional is a human decision.
                    depends_on: servers
                        .into_iter()
                        .map(|s| DepDecl { service: s, api: None, kind: DepKind::Hard })
                        .collect(),
                    features: vec![],
                };
                let header = "# Generated by `cascade init` from observed traffic.\n# Set the owner, add features with tiers, and mark optional dependencies `kind: soft`.\n";
                fs::write(&path, format!("{header}{}", serde_yaml::to_string(&f)?))?;
                println!("wrote {}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Simulate { registry, down, degraded } => {
            let reg = load(&registry)?;
            let graph = reg.graph();
            let mut faults = Faults::default();
            for s in &down {
                faults.services.insert(ServiceId(s.clone()), Impact::Down);
            }
            for s in &degraded {
                faults.services.entry(ServiceId(s.clone())).or_insert(Impact::Degraded);
            }
            for s in down.iter().chain(&degraded) {
                if graph.node(&ServiceId(s.clone())).is_none() {
                    bail!("unknown service `{s}` (not in the registry or any declared dependency)");
                }
            }
            let model = reg.model();
            let outcome = evaluate(&model, &faults);
            let mut hit: Vec<_> = model
                .features
                .iter()
                .filter_map(|f| outcome.features.get(&f.id).filter(|r| r.impact > Impact::None).map(|r| (f, r)))
                .collect();
            hit.sort_by(|a, b| b.1.impact.cmp(&a.1.impact).then(a.0.tier.cmp(&b.0.tier)).then(a.0.id.cmp(&b.0.id)));
            if hit.is_empty() {
                println!("no declared feature is affected");
            }
            for (f, r) in hit {
                println!(
                    "{:<8} {:?}  {:<28} owner={} cause={}",
                    r.impact.as_str(), f.tier, f.id, f.owner,
                    r.causes.iter().map(|c| c.0.as_str()).collect::<Vec<_>>().join(",")
                );
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
