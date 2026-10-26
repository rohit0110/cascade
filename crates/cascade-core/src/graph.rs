use std::collections::HashMap;

use petgraph::graph::{DiGraph, NodeIndex};

use crate::types::{DepKind, ServiceId};

/// Service dependency graph. Edges point from caller (client) to callee (server).
#[derive(Debug, Default)]
pub struct Graph {
    pub(crate) g: DiGraph<ServiceId, DepKind>,
    idx: HashMap<ServiceId, NodeIndex>,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_service(&mut self, id: impl Into<ServiceId>) -> NodeIndex {
        let id = id.into();
        if let Some(&n) = self.idx.get(&id) {
            return n;
        }
        let n = self.g.add_node(id.clone());
        self.idx.insert(id, n);
        n
    }

    /// Adds `client -> server`, creating services as needed. If the edge already
    /// exists, `Hard` wins: the conservative reading when sources disagree.
    pub fn add_dependency(
        &mut self,
        client: impl Into<ServiceId>,
        server: impl Into<ServiceId>,
        kind: DepKind,
    ) {
        let c = self.add_service(client);
        let s = self.add_service(server);
        match self.g.find_edge(c, s) {
            Some(e) => {
                if kind == DepKind::Hard {
                    self.g[e] = DepKind::Hard;
                }
            }
            None => {
                self.g.add_edge(c, s, kind);
            }
        }
    }

    pub fn node(&self, id: &ServiceId) -> Option<NodeIndex> {
        self.idx.get(id).copied()
    }

    pub fn edge_kind(&self, client: &ServiceId, server: &ServiceId) -> Option<DepKind> {
        let e = self.g.find_edge(self.node(client)?, self.node(server)?)?;
        Some(self.g[e])
    }

    pub fn services(&self) -> impl Iterator<Item = &ServiceId> {
        self.g.node_weights()
    }
}
