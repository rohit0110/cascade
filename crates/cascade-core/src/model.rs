use crate::types::{DepKind, ServiceId, Tier};

/// What a feature or service relies on. `api` narrows it to one feature of the target:
/// the feature whose `entrypoints` contains it. `api: "*"` means the target service's
/// availability only. With no `api`, a target that serves exactly one feature means that
/// feature; a target serving several means availability only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    pub service: ServiceId,
    pub api: Option<String>,
    pub kind: DepKind,
}

impl Dep {
    pub fn new(service: &str, kind: DepKind) -> Self {
        Dep { service: ServiceId(service.to_owned()), api: None, kind }
    }

    pub fn api(mut self, api: &str) -> Self {
        self.api = Some(api.to_owned());
        self
    }
}

/// A business capability served by one service.
#[derive(Debug, Clone)]
pub struct FeatureSpec {
    pub id: String,
    pub name: String,
    pub tier: Tier,
    pub owner: String,
    pub service: ServiceId,
    pub entrypoints: Vec<String>,
    pub deps: Vec<Dep>,
}

/// A service and the dependencies that affect everything it serves.
#[derive(Debug, Clone)]
pub struct ServiceSpec {
    pub id: ServiceId,
    pub deps: Vec<Dep>,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub services: Vec<ServiceSpec>,
    pub features: Vec<FeatureSpec>,
}
