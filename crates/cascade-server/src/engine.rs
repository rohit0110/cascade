use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::Hash;

use cascade_core::{
    evaluate, Dep, DepKind, Faults, HealthTracker, Impact, ObservedEdge, ServiceId, ServiceSpec,
    Thresholds,
};
use cascade_registry::{verify, Registry, VerifyOptions};

use crate::state::{Confidence, EdgeDto, FeatureDto, NodeDto, Origin, Snapshot};

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub thresholds: Thresholds,
    /// Services that are telemetry artifacts or non-business actors (`unknown`, load generators).
    pub ignore: BTreeSet<String>,
    /// Consecutive bad windows before an edge/service is flagged.
    pub raise_after: u32,
    /// Consecutive good windows before the flag clears.
    pub clear_after: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            thresholds: Thresholds::default(),
            ignore: ["unknown".to_string()].into(),
            raise_after: 2,
            clear_after: 3,
        }
    }
}

/// Holds the hysteresis state between polls and turns (registry, observed edges)
/// into a [`Snapshot`].
pub struct Engine {
    cfg: EngineConfig,
    edge_t: HashMap<(String, String), HealthTracker>,
    svc_t: HashMap<String, HealthTracker>,
}

fn step<K: Clone + Eq + Hash>(
    trackers: &mut HashMap<K, HealthTracker>,
    raw: &HashMap<K, Impact>,
    raise: u32,
    clear: u32,
) {
    for (k, &lvl) in raw {
        trackers.entry(k.clone()).or_insert_with(|| HealthTracker::new(raise, clear)).observe(lvl);
    }
    for (k, t) in trackers.iter_mut() {
        if !raw.contains_key(k) {
            t.observe(Impact::None);
        }
    }
    trackers.retain(|k, t| t.current() > Impact::None || raw.contains_key(k));
}

/// Inbound traffic of one service, summed over its callers.
#[derive(Default)]
struct Inbound {
    rps: f64,
    failed_rps: f64,
    /// `None` as soon as any caller's volume is unknown.
    volume: Option<f64>,
}

impl Engine {
    pub fn new(cfg: EngineConfig) -> Self {
        Self { cfg, edge_t: HashMap::new(), svc_t: HashMap::new() }
    }

    pub fn evaluate(&mut self, reg: &Registry, observed: &[ObservedEdge]) -> Snapshot {
        let cfg = &self.cfg;
        let obs: Vec<&ObservedEdge> = observed
            .iter()
            .filter(|e| !cfg.ignore.contains(&e.client) && !cfg.ignore.contains(&e.server))
            .collect();

        // Edge health, debounced.
        let edge_raw: HashMap<(String, String), Impact> = obs
            .iter()
            .map(|e| {
                let level = cfg.thresholds.classify(e.error_ratio, e.rps, e.volume);
                ((e.client.clone(), e.server.clone()), level)
            })
            .collect();
        step(&mut self.edge_t, &edge_raw, cfg.raise_after, cfg.clear_after);

        // Service health: error ratio over everything that calls it, debounced.
        let mut inbound: BTreeMap<&str, Inbound> = BTreeMap::new();
        for e in &obs {
            let a = inbound.entry(e.server.as_str()).or_insert_with(|| Inbound { volume: Some(0.0), ..Default::default() });
            a.rps += e.rps;
            a.failed_rps += e.rps * e.error_ratio;
            a.volume = a.volume.zip(e.volume).map(|(x, y)| x + y);
        }
        let ratio = |a: &Inbound| if a.rps > 0.0 { a.failed_rps / a.rps } else { 0.0 };
        let svc_raw: HashMap<String, Impact> = inbound
            .iter()
            .map(|(s, a)| (s.to_string(), cfg.thresholds.classify(ratio(a), a.rps, a.volume)))
            .collect();
        step(&mut self.svc_t, &svc_raw, cfg.raise_after, cfg.clear_after);

        // Root-cause pruning: a service with a failing callee is a symptom, not a cause.
        let flagged: HashMap<&str, Impact> = self
            .svc_t
            .iter()
            .filter(|(_, t)| t.current() > Impact::None)
            .map(|(s, t)| (s.as_str(), t.current()))
            .collect();
        let mut callees: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in &obs {
            callees.entry(e.client.as_str()).or_default().push(e.server.as_str());
        }
        let mut roots: HashMap<&str, Impact> = flagged
            .iter()
            .filter(|(s, _)| {
                !callees.get(*s).is_some_and(|cs| cs.iter().any(|c| flagged.contains_key(c)))
            })
            .map(|(s, i)| (*s, *i))
            .collect();
        if roots.is_empty() {
            roots = flagged.clone(); // a pure cycle: nothing to prune to
        }

        let mut faults = Faults::default();
        for (s, i) in &roots {
            faults.services.insert(ServiceId(s.to_string()), *i);
        }
        // An edge failing while its server looks healthy overall: blame that call only.
        for ((c, s), t) in &self.edge_t {
            if t.current() > Impact::None && !flagged.contains_key(s.as_str()) {
                faults.edges.insert((ServiceId(c.clone()), ServiceId(s.clone())), t.current());
            }
        }

        // Model = declared dependencies, plus observed-but-undeclared calls. Those are
        // assumed hard and service-wide (unknown which feature makes them): the safe reading.
        let declared = reg.declared_edges();
        let mut model = reg.model();
        let mut graph = reg.graph();
        for e in &obs {
            if declared.contains_key(&(e.client.clone(), e.server.clone())) {
                continue;
            }
            graph.add_dependency(e.client.as_str(), e.server.as_str(), DepKind::Hard);
            let dep = Dep::new(&e.server, DepKind::Hard);
            match model.services.iter_mut().find(|s| s.id.0 == e.client) {
                Some(s) => s.deps.push(dep),
                None => model.services.push(ServiceSpec { id: ServiceId(e.client.clone()), deps: vec![dep] }),
            }
        }
        let outcome = evaluate(&model, &faults);

        // Assemble DTOs.
        let obs_map: HashMap<(&str, &str), &ObservedEdge> =
            obs.iter().map(|e| ((e.client.as_str(), e.server.as_str()), *e)).collect();
        let mut keys: BTreeSet<(String, String)> = declared.keys().cloned().collect();
        keys.extend(obs.iter().map(|e| (e.client.clone(), e.server.clone())));
        let edges: Vec<EdgeDto> = keys
            .into_iter()
            .map(|(c, s)| {
                let o = obs_map.get(&(c.as_str(), s.as_str()));
                let d = declared.contains_key(&(c.clone(), s.clone()));
                let (origin, confidence) = match (d, o.is_some()) {
                    (true, true) => (Origin::Both, Confidence::High),
                    (true, false) => (Origin::Declared, Confidence::Low),
                    _ => (Origin::Observed, Confidence::Medium),
                };
                EdgeDto {
                    kind: graph
                        .edge_kind(&ServiceId(c.clone()), &ServiceId(s.clone()))
                        .unwrap_or(DepKind::Hard),
                    health: self.edge_t.get(&(c.clone(), s.clone())).map_or(Impact::None, |t| t.current()),
                    rps: o.map(|o| o.rps),
                    error_ratio: o.map(|o| o.error_ratio),
                    p95_ms: o.and_then(|o| o.p95_ms),
                    client: c,
                    server: s,
                    origin,
                    confidence,
                }
            })
            .collect();
        let confirmed: HashSet<(&str, &str)> = edges
            .iter()
            .filter(|e| e.origin == Origin::Both)
            .map(|e| (e.client.as_str(), e.server.as_str()))
            .collect();

        let svc_deps: HashMap<&ServiceId, &[Dep]> = model.services.iter().map(|s| (&s.id, s.deps.as_slice())).collect();
        let mut feature_dtos: Vec<FeatureDto> = model
            .features
            .iter()
            .map(|f| {
                // Confidence: how many of this feature's declared dependencies telemetry confirms.
                let wanted: HashSet<&str> = f
                    .deps
                    .iter()
                    .chain(svc_deps.get(&f.service).copied().unwrap_or(&[]))
                    .map(|d| d.service.0.as_str())
                    .collect();
                let seen = wanted.iter().filter(|t| confirmed.contains(&(f.service.0.as_str(), **t))).count();
                let confidence = if wanted.is_empty() || seen == wanted.len() {
                    Confidence::High
                } else if seen > 0 {
                    Confidence::Medium
                } else {
                    Confidence::Low
                };
                let r = outcome.features.get(&f.id).cloned().unwrap_or_default();
                FeatureDto {
                    id: f.id.clone(),
                    name: f.name.clone(),
                    tier: f.tier,
                    owner: f.owner.clone(),
                    services: vec![f.service.0.clone()],
                    impact: r.impact,
                    causes: r.causes.iter().map(|c| c.0.clone()).collect(),
                    confidence,
                }
            })
            .collect();
        feature_dtos.sort_by(|a, b| b.impact.cmp(&a.impact).then(a.tier.cmp(&b.tier)).then(a.id.cmp(&b.id)));

        let mut nodes: Vec<NodeDto> = graph
            .services()
            .map(|id| {
                let r = outcome.nodes.get(id).cloned().unwrap_or_default();
                let hosted: Vec<&FeatureDto> =
                    feature_dtos.iter().filter(|f| f.services.contains(&id.0)).collect();
                let agg = inbound.get(id.0.as_str());
                NodeDto {
                    id: id.0.clone(),
                    owner: reg.services.get(&id.0).map(|s| s.owner.clone()),
                    declared: reg.services.contains_key(&id.0),
                    kind: reg.services.get(&id.0).map(|s| s.kind).unwrap_or_default(),
                    impact: r.impact,
                    causes: r.causes.iter().map(|c| c.0.clone()).collect(),
                    tier: hosted.iter().filter(|f| f.impact > Impact::None).map(|f| f.tier).min(),
                    features: hosted.iter().map(|f| f.id.clone()).collect(),
                    rps: agg.map(|a| a.rps),
                    error_ratio: agg.map(ratio),
                    p95_ms: obs
                        .iter()
                        .filter(|e| e.server == id.0)
                        .filter_map(|e| e.p95_ms)
                        .max_by(f64::total_cmp),
                }
            })
            .collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));

        let drift = verify(
            reg,
            observed,
            &VerifyOptions { ignore: cfg.ignore.clone(), min_rps: cfg.thresholds.min_rps },
        );
        Snapshot {
            nodes,
            edges,
            features: feature_dtos,
            drift,
            registry_warnings: reg.warnings.clone(),
            ..Snapshot::default()
        }
    }
}
