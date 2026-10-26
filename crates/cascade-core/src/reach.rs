use std::collections::{BTreeSet, HashMap};

use crate::model::{Dep, FeatureSpec, Model};
use crate::types::{DepKind, Impact, ServiceId};

/// Observed failures to propagate.
#[derive(Debug, Default, Clone)]
pub struct Faults {
    /// A service that is itself failing.
    pub services: HashMap<ServiceId, Impact>,
    /// A failing call `(client, server)`; the client suffers, the server may be fine.
    pub edges: HashMap<(ServiceId, ServiceId), Impact>,
}

/// How badly something is affected, and by which root failures. `causes` holds only
/// the roots that produce `impact` itself: a root that could only degrade is not
/// blamed for something that is down.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reach {
    pub impact: Impact,
    pub causes: BTreeSet<ServiceId>,
}

impl Reach {
    fn new(impact: Impact, cause: &ServiceId) -> Reach {
        if impact == Impact::None {
            return Reach::default();
        }
        Reach { impact, causes: [cause.clone()].into() }
    }

    /// Passes a failure through a dependency: a soft dependency caps it at `Degraded`.
    fn through(&self, kind: DepKind) -> Reach {
        let impact = self.impact.through(kind);
        if impact == Impact::None {
            return Reach::default();
        }
        Reach { impact, causes: self.causes.clone() }
    }
}

fn combine(candidates: impl IntoIterator<Item = Reach>) -> Reach {
    let mut out = Reach::default();
    for c in candidates {
        if c.impact > out.impact {
            out = c;
        } else if c.impact == out.impact && c.impact > Impact::None {
            out.causes.extend(c.causes);
        }
    }
    out
}

#[derive(Debug, Default)]
pub struct Outcome {
    /// A service's own state: its own failure plus its service-level dependencies.
    pub services: HashMap<ServiceId, Reach>,
    pub features: HashMap<String, Reach>,
    /// What a graph node should show: the worst of the service and the features it hosts.
    pub nodes: HashMap<ServiceId, Reach>,
}

/// Which feature of `dep.service` a dependency points at, if any.
fn resolve<'a>(by_service: &'a HashMap<&ServiceId, Vec<&FeatureSpec>>, dep: &Dep) -> Option<&'a FeatureSpec> {
    let feats = by_service.get(&dep.service)?;
    match dep.api.as_deref() {
        Some("*") => None,
        Some(api) => feats.iter().copied().find(|f| f.entrypoints.iter().any(|e| e == api)),
        None if feats.len() == 1 => Some(feats[0]),
        None => None,
    }
}

/// Computes feature-level impact. Each feature is affected by its service's own state
/// and by the specific things it depends on, not by everything else its service does.
/// Dependencies that form cycles are fine: values only grow and are bounded.
pub fn evaluate(model: &Model, faults: &Faults) -> Outcome {
    let mut all: Vec<ServiceId> = Vec::new();
    let add = |s: &ServiceId, all: &mut Vec<ServiceId>| {
        if !all.contains(s) {
            all.push(s.clone());
        }
    };
    for s in &model.services {
        add(&s.id, &mut all);
        for d in &s.deps {
            add(&d.service, &mut all);
        }
    }
    for f in &model.features {
        add(&f.service, &mut all);
        for d in &f.deps {
            add(&d.service, &mut all);
        }
    }
    for s in faults.services.keys() {
        add(s, &mut all);
    }

    let svc_deps: HashMap<&ServiceId, &[Dep]> = model.services.iter().map(|s| (&s.id, s.deps.as_slice())).collect();
    let mut by_service: HashMap<&ServiceId, Vec<&FeatureSpec>> = HashMap::new();
    for f in &model.features {
        by_service.entry(&f.service).or_default().push(f);
    }

    let mut own: HashMap<ServiceId, Reach> = all.iter().map(|s| (s.clone(), Reach::default())).collect();
    let mut feat: HashMap<String, Reach> =
        model.features.iter().map(|f| (f.id.clone(), Reach::default())).collect();

    // What `client` suffers from one dependency, given the previous round's values.
    let from_dep = |client: &ServiceId, d: &Dep, own: &HashMap<ServiceId, Reach>, feat: &HashMap<String, Reach>| {
        let target = match resolve(&by_service, d) {
            Some(f) => feat.get(&f.id).cloned().unwrap_or_default(),
            None => own.get(&d.service).cloned().unwrap_or_default(),
        };
        let mut c = vec![target.through(d.kind)];
        if let Some(&i) = faults.edges.get(&(client.clone(), d.service.clone())) {
            c.push(Reach::new(i, &d.service).through(d.kind));
        }
        combine(c)
    };

    for _ in 0..(4 * (all.len() + model.features.len()) + 8) {
        let new_own: HashMap<ServiceId, Reach> = all
            .iter()
            .map(|s| {
                let mut c = vec![Reach::new(faults.services.get(s).copied().unwrap_or_default(), s)];
                for d in svc_deps.get(s).copied().unwrap_or(&[]) {
                    c.push(from_dep(s, d, &own, &feat));
                }
                (s.clone(), combine(c))
            })
            .collect();
        let new_feat: HashMap<String, Reach> = model
            .features
            .iter()
            .map(|f| {
                let mut c = vec![new_own.get(&f.service).cloned().unwrap_or_default()];
                for d in &f.deps {
                    c.push(from_dep(&f.service, d, &new_own, &feat));
                }
                (f.id.clone(), combine(c))
            })
            .collect();
        let done = new_own == own && new_feat == feat;
        own = new_own;
        feat = new_feat;
        if done {
            break;
        }
    }

    let nodes = all
        .iter()
        .map(|s| {
            let mut c = vec![own.get(s).cloned().unwrap_or_default()];
            for f in by_service.get(s).into_iter().flatten() {
                c.push(feat.get(&f.id).cloned().unwrap_or_default());
            }
            (s.clone(), combine(c))
        })
        .collect();
    Outcome { services: own, features: feat, nodes }
}
