use std::collections::BTreeSet;

use cascade_core::ObservedEdge;
use serde::Serialize;

use crate::registry::Registry;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct EdgeRef {
    pub client: String,
    pub server: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DriftReport {
    /// Seen in telemetry from a declared service, but not declared. Fails `verify`.
    pub undeclared: Vec<EdgeRef>,
    /// Declared but not seen in the window. A warning: the window may simply be quiet.
    pub unobserved: Vec<EdgeRef>,
    /// Services seen in telemetry as callers that have no `cascade.yaml`.
    pub undeclared_services: Vec<String>,
}

impl DriftReport {
    pub fn is_clean(&self) -> bool {
        self.undeclared.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    /// Service names to disregard entirely (e.g. `unknown`, load generators).
    pub ignore: BTreeSet<String>,
    /// Observed edges below this rate are not trusted as evidence.
    pub min_rps: f64,
}

pub fn verify(reg: &Registry, observed: &[ObservedEdge], opts: &VerifyOptions) -> DriftReport {
    let declared = reg.declared_edges();
    let mut report = DriftReport::default();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut undeclared_services: BTreeSet<String> = BTreeSet::new();

    for e in observed {
        if opts.ignore.contains(&e.client) || opts.ignore.contains(&e.server) {
            continue;
        }
        if e.rps < opts.min_rps {
            continue;
        }
        seen.insert((e.client.clone(), e.server.clone()));
        if !reg.services.contains_key(&e.client) {
            undeclared_services.insert(e.client.clone());
        } else if !declared.contains_key(&(e.client.clone(), e.server.clone())) {
            report.undeclared.push(EdgeRef { client: e.client.clone(), server: e.server.clone() });
        }
    }
    for (c, s) in declared.keys() {
        if opts.ignore.contains(c) || opts.ignore.contains(s) {
            continue;
        }
        if !seen.contains(&(c.clone(), s.clone())) {
            report.unobserved.push(EdgeRef { client: c.clone(), server: s.clone() });
        }
    }
    report.undeclared.sort();
    report.unobserved.sort();
    report.undeclared_services = undeclared_services.into_iter().collect();
    report
}
