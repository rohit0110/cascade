use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use cascade_core::{Dep, DepKind, FeatureSpec, Graph, Model, ServiceId, ServiceSpec};

use crate::schema::{DepDecl, ServiceFile};

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{}: {msg}", path.display())]
    File { path: PathBuf, msg: String },
    #[error("{0}")]
    Invalid(String),
}

/// All declared services, validated.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    pub services: BTreeMap<String, ServiceFile>,
    /// Non-fatal findings, e.g. a dependency on a service nobody declared.
    pub warnings: Vec<String>,
}

impl Registry {
    /// Loads every `*.yaml` / `*.yml` under `dir` (recursively). A file holds one service.
    pub fn load_dir(dir: &Path) -> Result<Registry, RegistryError> {
        let mut paths = Vec::new();
        collect(dir, &mut paths)?;
        paths.sort();
        let mut files = Vec::new();
        for p in paths {
            let text = fs::read_to_string(&p)
                .map_err(|e| RegistryError::File { path: p.clone(), msg: e.to_string() })?;
            let f: ServiceFile = serde_yaml::from_str(&text)
                .map_err(|e| RegistryError::File { path: p.clone(), msg: e.to_string() })?;
            files.push((p, f));
        }
        Self::from_files(files)
    }

    pub fn from_files(files: Vec<(PathBuf, ServiceFile)>) -> Result<Registry, RegistryError> {
        let mut reg = Registry::default();
        let mut feature_ids: BTreeMap<String, String> = BTreeMap::new();
        for (path, f) in files {
            let ctx = |msg: String| RegistryError::File { path: path.clone(), msg };
            if f.service.trim().is_empty() {
                return Err(ctx("`service` must not be empty".into()));
            }
            if f.owner.trim().is_empty() {
                return Err(ctx(format!("service `{}` has an empty `owner`", f.service)));
            }
            if reg.services.contains_key(&f.service) {
                return Err(ctx(format!("service `{}` is declared more than once", f.service)));
            }
            for feat in &f.features {
                if feat.id.trim().is_empty() {
                    return Err(ctx("feature `id` must not be empty".into()));
                }
                if let Some(other) = feature_ids.insert(feat.id.clone(), f.service.clone()) {
                    return Err(ctx(format!(
                        "feature id `{}` already used by service `{other}`",
                        feat.id
                    )));
                }
            }
            reg.services.insert(f.service.clone(), f);
        }
        for f in reg.services.values() {
            for d in f.depends_on.iter().chain(f.features.iter().flat_map(|x| &x.depends_on)) {
                if d.service == f.service {
                    reg.warnings.push(format!("`{}` declares a dependency on itself", f.service));
                } else if let Some(t) = reg.services.get(&d.service) {
                    if d.api.is_none() && t.features.len() > 1 {
                        reg.warnings.push(format!(
                            "`{}` depends on `{}` without `api`; `{}` serves several features, so this means its availability only (set `api:` to a feature entrypoint, or `api: \"*\"` to say so explicitly)",
                            f.service, d.service, d.service
                        ));
                    }
                } else {
                    reg.warnings.push(format!(
                        "`{}` depends on `{}`, which has no cascade.yaml",
                        f.service, d.service
                    ));
                }
            }
        }
        reg.warnings.sort();
        reg.warnings.dedup();
        Ok(reg)
    }

    /// Service-level declared edges. If features of one service disagree on a
    /// dependency's kind, `Hard` wins.
    pub fn declared_edges(&self) -> BTreeMap<(String, String), DepKind> {
        let mut out: BTreeMap<(String, String), DepKind> = BTreeMap::new();
        for f in self.services.values() {
            for d in f.depends_on.iter().chain(f.features.iter().flat_map(|x| &x.depends_on)) {
                let e = out.entry((f.service.clone(), d.service.clone())).or_insert(d.kind);
                if d.kind == DepKind::Hard {
                    *e = DepKind::Hard;
                }
            }
        }
        out
    }

    pub fn graph(&self) -> Graph {
        let mut g = Graph::new();
        for s in self.services.keys() {
            g.add_service(s.as_str());
        }
        for ((c, s), kind) in self.declared_edges() {
            g.add_dependency(c.as_str(), s.as_str(), kind);
        }
        g
    }

    /// The feature/service dependency model the impact engine evaluates.
    pub fn model(&self) -> Model {
        let dep = |d: &DepDecl| Dep { service: ServiceId(d.service.clone()), api: d.api.clone(), kind: d.kind };
        let mut m = Model::default();
        for f in self.services.values() {
            m.services.push(ServiceSpec {
                id: ServiceId(f.service.clone()),
                deps: f.depends_on.iter().map(dep).collect(),
            });
            for x in &f.features {
                m.features.push(FeatureSpec {
                    id: x.id.clone(),
                    name: x.name.clone().unwrap_or_else(|| x.id.clone()),
                    tier: x.tier,
                    owner: f.owner.clone(),
                    service: ServiceId(f.service.clone()),
                    entrypoints: x.entrypoints.clone(),
                    deps: x.depends_on.iter().map(dep).collect(),
                });
            }
        }
        m
    }

    /// `(feature id, service, metric)` for every metric declared on a feature.
    pub fn metrics(&self) -> Vec<(String, String, crate::schema::MetricDecl)> {
        self.services
            .values()
            .flat_map(|f| {
                f.features.iter().flat_map(move |x| {
                    x.metrics.iter().map(move |m| (x.id.clone(), f.service.clone(), m.clone()))
                })
            })
            .collect()
    }

    pub fn declared_service_names(&self) -> BTreeSet<&str> {
        self.services.keys().map(String::as_str).collect()
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), RegistryError> {
    let rd = fs::read_dir(dir)
        .map_err(|e| RegistryError::File { path: dir.to_owned(), msg: e.to_string() })?;
    for entry in rd {
        let p = entry
            .map_err(|e| RegistryError::File { path: dir.to_owned(), msg: e.to_string() })?
            .path();
        if p.is_dir() {
            collect(&p, out)?;
        } else if matches!(p.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")) {
            out.push(p);
        }
    }
    Ok(())
}
