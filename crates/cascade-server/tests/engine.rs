use std::path::PathBuf;

use cascade_core::{DepKind, Impact, ObservedEdge};
use cascade_registry::{Registry, ServiceFile};
use cascade_server::engine::{Engine, EngineConfig};
use cascade_server::state::{Confidence, Origin, Snapshot};

fn reg() -> Registry {
    let y = [
        "service: frontend\nowner: web\nfeatures:\n  - {id: browse, name: Browse, tier: P1, depends_on: [{service: checkout}, {service: recommendation, kind: soft}]}\n",
        "service: checkout\nowner: payments\nfeatures:\n  - {id: checkout, name: Checkout, tier: P0, depends_on: [{service: payment}]}\n",
        "service: payment\nowner: payments\nfeatures:\n  - {id: charge, tier: P0}\n",
        "service: recommendation\nowner: ml\n",
    ];
    Registry::from_files(
        y.iter().map(|y| (PathBuf::from("t"), serde_yaml::from_str::<ServiceFile>(y).unwrap())).collect(),
    )
    .unwrap()
}

fn e(c: &str, s: &str, rps: f64, err: f64) -> ObservedEdge {
    ObservedEdge { client: c.into(), server: s.into(), rps, error_ratio: err, volume: None, p95_ms: None }
}

fn engine() -> Engine {
    Engine::new(EngineConfig { raise_after: 1, clear_after: 2, ..EngineConfig::default() })
}

fn healthy() -> Vec<ObservedEdge> {
    vec![
        e("frontend", "checkout", 5.0, 0.0),
        e("checkout", "payment", 5.0, 0.0),
        e("frontend", "recommendation", 5.0, 0.0),
    ]
}

fn feature<'a>(s: &'a Snapshot, id: &str) -> &'a cascade_server::state::FeatureDto {
    s.features.iter().find(|f| f.id == id).unwrap()
}

fn node<'a>(s: &'a Snapshot, id: &str) -> &'a cascade_server::state::NodeDto {
    s.nodes.iter().find(|n| n.id == id).unwrap()
}

#[test]
fn healthy_system_has_no_impact() {
    let s = engine().evaluate(&reg(), &healthy());
    assert!(s.features.iter().all(|f| f.impact == Impact::None));
    assert!(s.nodes.iter().all(|n| n.impact == Impact::None));
    assert!(s.drift.is_clean());
}

#[test]
fn payment_outage_blames_payment_only_and_hits_upstream_features() {
    // Errors bubble up: every caller edge looks bad, but only payment is the cause.
    let obs = vec![
        e("frontend", "checkout", 5.0, 1.0),
        e("checkout", "payment", 5.0, 1.0),
        e("frontend", "recommendation", 5.0, 0.0),
    ];
    let s = engine().evaluate(&reg(), &obs);
    assert_eq!(node(&s, "payment").causes, ["payment"]);
    assert_eq!(node(&s, "checkout").causes, ["payment"], "checkout is a symptom");
    assert_eq!(node(&s, "frontend").causes, ["payment"]);
    let checkout = feature(&s, "checkout");
    assert_eq!((checkout.impact, checkout.owner.as_str()), (Impact::Down, "payments"));
    assert_eq!(feature(&s, "browse").impact, Impact::Down);
    assert_eq!(feature(&s, "charge").impact, Impact::Down);
    assert_eq!(s.features[0].tier, cascade_core::Tier::P0, "worst and most critical first");
    assert_eq!(node(&s, "checkout").tier, Some(cascade_core::Tier::P0));
}

#[test]
fn soft_dependency_failure_only_degrades() {
    let mut obs = healthy();
    obs[2] = e("frontend", "recommendation", 5.0, 1.0);
    let s = engine().evaluate(&reg(), &obs);
    assert_eq!(feature(&s, "browse").impact, Impact::Degraded);
    assert_eq!(feature(&s, "checkout").impact, Impact::None);
}

#[test]
fn hysteresis_delays_flagging_and_clearing() {
    let mut eng = Engine::new(EngineConfig { raise_after: 2, clear_after: 2, ..EngineConfig::default() });
    let r = reg();
    let bad = vec![e("checkout", "payment", 5.0, 1.0)];
    let ok = vec![e("checkout", "payment", 5.0, 0.0)];
    assert_eq!(feature(&eng.evaluate(&r, &bad), "charge").impact, Impact::None, "one bad window is noise");
    assert_eq!(feature(&eng.evaluate(&r, &bad), "charge").impact, Impact::Down);
    assert_eq!(feature(&eng.evaluate(&r, &ok), "charge").impact, Impact::Down, "one good window isn't recovery");
    assert_eq!(feature(&eng.evaluate(&r, &ok), "charge").impact, Impact::None);
}

#[test]
fn one_bad_call_path_blames_the_edge_not_the_server() {
    let r = Registry::from_files(
        ["service: a\nowner: o\nfeatures: [{id: fa, tier: P1, depends_on: [{service: s}]}]\n",
         "service: b\nowner: o\nfeatures: [{id: fb, tier: P1, depends_on: [{service: s}]}]\n",
         "service: s\nowner: o\nfeatures: [{id: fs, tier: P1}]\n"]
            .iter()
            .map(|y| (PathBuf::from("t"), serde_yaml::from_str::<ServiceFile>(y).unwrap()))
            .collect(),
    )
    .unwrap();
    let obs = vec![e("a", "s", 1.0, 1.0), e("b", "s", 100.0, 0.0)];
    let s = engine().evaluate(&r, &obs);
    assert_eq!(feature(&s, "fa").impact, Impact::Down);
    assert_eq!(feature(&s, "fb").impact, Impact::None);
    assert_eq!(feature(&s, "fs").impact, Impact::None, "the server itself is fine overall");
    assert_eq!(feature(&s, "fa").causes, ["s"]);
}

#[test]
fn undeclared_edge_is_hard_medium_confidence_and_drift() {
    let mut obs = healthy();
    obs.push(e("checkout", "fraud", 2.0, 0.0));
    let s = engine().evaluate(&reg(), &obs);
    let edge = s.edges.iter().find(|x| x.server == "fraud").unwrap();
    assert_eq!((edge.origin, edge.kind, edge.confidence), (Origin::Observed, DepKind::Hard, Confidence::Medium));
    assert_eq!(s.drift.undeclared.len(), 1);
    assert!(s.nodes.iter().any(|n| n.id == "fraud" && !n.declared));
}

#[test]
fn declared_but_unseen_edge_is_low_confidence() {
    let obs = vec![e("frontend", "checkout", 5.0, 0.0), e("frontend", "recommendation", 5.0, 0.0)];
    let s = engine().evaluate(&reg(), &obs);
    let edge = s.edges.iter().find(|x| x.server == "payment").unwrap();
    assert_eq!((edge.origin, edge.confidence), (Origin::Declared, Confidence::Low));
    assert_eq!(feature(&s, "checkout").confidence, Confidence::Low);
    assert_eq!(feature(&s, "browse").confidence, Confidence::High);
}

#[test]
fn ignored_services_are_dropped() {
    let mut obs = healthy();
    obs.push(e("checkout", "unknown", 50.0, 1.0));
    let s = engine().evaluate(&reg(), &obs);
    assert!(s.nodes.iter().all(|n| n.id != "unknown"));
    assert!(s.features.iter().all(|f| f.impact == Impact::None));
}

#[test]
fn nodes_carry_kind_and_worst_inbound_latency() {
    let r = Registry::from_files(
        ["service: web\nowner: o\n", "service: db\nowner: o\nkind: database\n"]
            .iter()
            .map(|y| (PathBuf::from("t"), serde_yaml::from_str::<ServiceFile>(y).unwrap()))
            .collect(),
    )
    .unwrap();
    let mut a = e("web", "db", 5.0, 0.0);
    a.p95_ms = Some(40.0);
    let mut b = e("api", "db", 5.0, 0.0);
    b.p95_ms = Some(90.0);
    let s = engine().evaluate(&r, &[a, b]);
    let db = node(&s, "db");
    assert_eq!(db.kind, cascade_core::NodeKind::Database);
    assert_eq!(db.p95_ms, Some(90.0));
    assert_eq!(node(&s, "web").kind, cascade_core::NodeKind::Service);
    assert_eq!(node(&s, "api").kind, cascade_core::NodeKind::Service, "undeclared defaults to service");
}

// ---- scenarios replayed against the real demo registry ----

fn demo() -> Registry {
    Registry::load_dir(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../deploy/demo/registry")))
        .expect("demo registry loads")
}

fn vol(c: &str, s: &str, rps: f64, err: f64) -> ObservedEdge {
    ObservedEdge { client: c.into(), server: s.into(), rps, error_ratio: err, volume: Some(rps * 60.0), p95_ms: None }
}

/// Every edge the demo shows, healthy, so only the injected fault stands out.
fn demo_traffic(overrides: &[ObservedEdge]) -> Vec<ObservedEdge> {
    let base = [
        ("frontend-proxy", "frontend"), ("frontend-proxy", "image-provider"), ("frontend-proxy", "flagd"),
        ("frontend", "product-catalog"), ("frontend", "currency"), ("frontend", "recommendation"),
        ("frontend", "ad"), ("frontend", "cart"), ("frontend", "checkout"), ("frontend", "shipping"),
        ("checkout", "cart"), ("checkout", "payment"), ("checkout", "shipping"), ("checkout", "currency"),
        ("checkout", "product-catalog"), ("checkout", "email"), ("cart", "redis"), ("cart", "flagd"),
        ("payment", "flagd"), ("ad", "flagd"), ("recommendation", "product-catalog"), ("recommendation", "flagd"),
        ("shipping", "quote"),
    ];
    base.iter()
        .map(|(c, s)| {
            overrides
                .iter()
                .find(|o| o.client == *c && o.server == *s)
                .cloned()
                .unwrap_or_else(|| vol(c, s, 5.0, 0.0))
        })
        .collect()
}

fn run(overrides: &[ObservedEdge]) -> Snapshot {
    // Two windows, so the debounce (raise_after = 2) has flagged the fault.
    let mut eng = Engine::new(EngineConfig { ignore: ["unknown".into()].into(), ..EngineConfig::default() });
    let r = demo();
    eng.evaluate(&r, &demo_traffic(overrides));
    eng.evaluate(&r, &demo_traffic(overrides))
}

fn affected(s: &Snapshot) -> Vec<(String, Impact)> {
    let mut v: Vec<_> = s.features.iter().filter(|f| f.impact > Impact::None).map(|f| (f.id.clone(), f.impact)).collect();
    v.sort();
    v
}

#[test]
fn demo_healthy_traffic_affects_nothing() {
    assert_eq!(affected(&run(&[])), vec![]);
}

#[test]
fn demo_failing_ads_degrade_only_ads_and_browse() {
    // adFailure: about 14% of frontend -> ad calls fail. Ad is behind a soft dependency.
    let s = run(&[vol("frontend", "ad", 0.28, 0.14)]);
    assert_eq!(
        affected(&s),
        vec![("ads".into(), Impact::Degraded), ("browse-catalog".into(), Impact::Degraded)],
        "P0 place-order / storefront-entry and cart-view must not be touched"
    );
    assert_eq!(feature(&s, "browse-catalog").causes, ["ad"]);
}

#[test]
fn demo_payment_outage_hits_the_checkout_path_only_and_blames_payment_alone() {
    // paymentFailure=100% plus flagd noise: payment -> flagd failing on 2 requests must be ignored.
    let s = run(&[
        vol("checkout", "payment", 0.5, 1.0),
        vol("frontend", "checkout", 0.5, 1.0),
        vol("frontend-proxy", "frontend", 8.0, 0.06),
        ObservedEdge { client: "payment".into(), server: "flagd".into(), rps: 0.06, error_ratio: 0.5, volume: Some(2.0), p95_ms: None },
    ]);
    assert_eq!(
        affected(&s),
        vec![
            ("checkout".into(), Impact::Down),
            ("payment-processing".into(), Impact::Down),
            ("place-order".into(), Impact::Down),
        ]
    );
    for id in ["checkout", "payment-processing", "place-order"] {
        assert_eq!(feature(&s, id).causes, ["payment"], "{id}: flagd and others are not causes");
    }
}

#[test]
fn demo_shipping_outage_hits_place_order_but_not_browse_or_cart() {
    let s = run(&[vol("checkout", "shipping", 2.0, 0.95), vol("frontend", "checkout", 2.0, 1.0)]);
    let hit: Vec<String> = affected(&s).into_iter().map(|(id, _)| id).collect();
    assert_eq!(hit, ["checkout", "place-order", "shipping-quote"]);
    assert!(!hit.contains(&"storefront-entry".to_string()), "proxy only needs the frontend reachable");
}

#[test]
fn low_volume_error_spikes_are_ignored() {
    let one = ObservedEdge { client: "frontend".into(), server: "ad".into(), rps: 0.05, error_ratio: 1.0, volume: Some(3.0), p95_ms: None };
    assert_eq!(affected(&run(&[one])), vec![]);
}
