use std::path::PathBuf;

use cascade_core::{DepKind, ObservedEdge, Tier};
use cascade_registry::*;

fn parse(y: &str) -> ServiceFile {
    serde_yaml::from_str(y).unwrap()
}

fn reg(ys: &[&str]) -> Result<Registry, RegistryError> {
    Registry::from_files(ys.iter().map(|y| (PathBuf::from("t.yaml"), parse(y))).collect())
}

fn obs(c: &str, s: &str) -> ObservedEdge {
    ObservedEdge { client: c.into(), server: s.into(), rps: 1.0, error_ratio: 0.0, volume: None, p95_ms: None }
}

const CHECKOUT: &str = r#"
service: checkout
owner: payments-team
features:
  - id: checkout
    name: Checkout
    tier: P0
    entrypoints: ["POST /api/checkout"]
    depends_on:
      - { service: payment, api: "POST /charge", kind: hard }
      - { service: recommendation, kind: soft }
"#;
const PAYMENT: &str = "service: payment\nowner: payments-team\n";

#[test]
fn parses_schema_with_defaults() {
    let f = parse(CHECKOUT);
    assert_eq!(f.features[0].tier, Tier::P0);
    assert_eq!(f.features[0].depends_on[1].kind, DepKind::Soft);
    let g = parse("service: a\nowner: o\nfeatures:\n  - {id: x, tier: P2, depends_on: [{service: b}]}\n");
    assert_eq!(g.features[0].depends_on[0].kind, DepKind::Hard, "kind defaults to hard");
}

#[test]
fn rejects_unknown_fields_and_bad_tier() {
    assert!(serde_yaml::from_str::<ServiceFile>("service: a\nowner: o\nbogus: 1\n").is_err());
    assert!(serde_yaml::from_str::<ServiceFile>(
        "service: a\nowner: o\nfeatures:\n  - {id: x, tier: P9}\n"
    )
    .is_err());
}

#[test]
fn validation_errors() {
    assert!(reg(&[PAYMENT, PAYMENT]).is_err(), "duplicate service");
    let a = "service: a\nowner: o\nfeatures: [{id: f, tier: P0}]\n";
    let b = "service: b\nowner: o\nfeatures: [{id: f, tier: P1}]\n";
    assert!(reg(&[a, b]).is_err(), "duplicate feature id");
    assert!(reg(&["service: a\nowner: ''\n"]).is_err(), "empty owner");
}

#[test]
fn warns_on_undeclared_dependency_target() {
    let r = reg(&[CHECKOUT, PAYMENT]).unwrap();
    assert_eq!(r.warnings, ["`checkout` depends on `recommendation`, which has no cascade.yaml"]);
}

#[test]
fn builds_graph_and_features() {
    let r = reg(&[CHECKOUT, PAYMENT]).unwrap();
    let e = r.declared_edges();
    assert_eq!(e[&("checkout".into(), "payment".into())], DepKind::Hard);
    assert_eq!(e[&("checkout".into(), "recommendation".into())], DepKind::Soft);
    let m = r.model();
    assert_eq!(m.features.len(), 1);
    assert_eq!(m.features[0].owner, "payments-team");
    assert_eq!(m.features[0].deps.len(), 2);
    assert_eq!(m.features[0].deps[0].api.as_deref(), Some("POST /charge"));
    assert_eq!(r.graph().services().count(), 3);
}

#[test]
fn verify_finds_drift() {
    let r = reg(&[CHECKOUT, PAYMENT]).unwrap();
    let opts = VerifyOptions { ignore: ["unknown".to_string()].into(), min_rps: 0.0 };
    let observed = vec![
        obs("checkout", "payment"),
        obs("checkout", "fraud"),       // undeclared dependency
        obs("checkout", "unknown"),     // ignored
        obs("mystery", "payment"),      // caller with no cascade.yaml
    ];
    let d = verify(&r, &observed, &opts);
    assert_eq!(d.undeclared, [EdgeRef { client: "checkout".into(), server: "fraud".into() }]);
    assert_eq!(d.unobserved, [EdgeRef { client: "checkout".into(), server: "recommendation".into() }]);
    assert_eq!(d.undeclared_services, ["mystery"]);
    assert!(!d.is_clean());
}

#[test]
fn verify_clean_when_matching() {
    let r = reg(&[CHECKOUT, PAYMENT]).unwrap();
    let observed = vec![obs("checkout", "payment"), obs("checkout", "recommendation")];
    assert!(verify(&r, &observed, &VerifyOptions::default()).is_clean());
}

#[test]
fn parses_kind_and_metrics() {
    let f = parse(
        "service: orders-db\nowner: o\nkind: database\n",
    );
    assert_eq!(f.kind, cascade_core::NodeKind::Database);
    assert_eq!(parse("service: a\nowner: o\n").kind, cascade_core::NodeKind::Service);
    let g = parse(
        "service: a\nowner: o\nfeatures:\n  - id: x\n    tier: P0\n    metrics:\n      - {name: orders_per_min, query: 'sum(rate(o[5m]))*60', unit: /min}\n      - {name: bare}\n",
    );
    let m = &g.features[0].metrics;
    assert_eq!((m[0].source.as_str(), m[0].unit.as_deref()), ("prometheus", Some("/min")));
    assert_eq!(m[1].query, None);
    let r = Registry::from_files(vec![(PathBuf::from("t"), g)]).unwrap();
    assert_eq!(r.metrics().len(), 2);
    assert_eq!(r.metrics()[0].0, "x");
    assert!(serde_yaml::from_str::<ServiceFile>("service: a\nowner: o\nkind: spaceship\n").is_err());
}

#[test]
fn warns_when_depending_on_a_multi_feature_service_without_api() {
    let multi = "service: web\nowner: o\nfeatures:\n  - {id: a, tier: P0}\n  - {id: b, tier: P1}\n";
    let bare = "service: edge\nowner: o\ndepends_on: [{service: web}]\n";
    let r = reg(&[multi, bare]).unwrap();
    assert_eq!(r.warnings.len(), 1);
    assert!(r.warnings[0].contains("availability only"));
    let explicit = "service: edge\nowner: o\ndepends_on: [{service: web, api: \"*\"}]\n";
    assert!(reg(&[multi, explicit]).unwrap().warnings.is_empty());
    let one = "service: web\nowner: o\nfeatures:\n  - {id: a, tier: P0}\n";
    assert!(reg(&[one, bare]).unwrap().warnings.is_empty(), "single-feature targets are unambiguous");
}
