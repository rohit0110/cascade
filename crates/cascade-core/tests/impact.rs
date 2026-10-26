use cascade_core::*;
use DepKind::{Hard, Soft};

fn sid(s: &str) -> ServiceId {
    s.into()
}

fn feature(id: &str, tier: Tier, service: &str, entry: &[&str], deps: Vec<Dep>) -> FeatureSpec {
    FeatureSpec {
        id: id.into(),
        name: id.into(),
        tier,
        owner: "team".into(),
        service: sid(service),
        entrypoints: entry.iter().map(|s| s.to_string()).collect(),
        deps,
    }
}

fn svc(id: &str, deps: Vec<Dep>) -> ServiceSpec {
    ServiceSpec { id: sid(id), deps }
}

fn down(s: &str) -> Faults {
    let mut f = Faults::default();
    f.services.insert(sid(s), Impact::Down);
    f
}

fn impact(o: &Outcome, feature: &str) -> Impact {
    o.features[feature].impact
}

fn causes(o: &Outcome, feature: &str) -> Vec<String> {
    o.features[feature].causes.iter().map(|c| c.0.clone()).collect()
}

/// A shop: the frontend serves three features, only one of which uses checkout.
fn shop() -> Model {
    Model {
        services: vec![],
        features: vec![
            feature("storefront", Tier::P0, "proxy", &[], vec![Dep::new("frontend", Hard).api("*")]),
            feature(
                "browse",
                Tier::P1,
                "frontend",
                &["GET /products"],
                vec![Dep::new("catalog", Hard), Dep::new("ads", Soft)],
            ),
            feature("cart-view", Tier::P1, "frontend", &["GET /cart"], vec![Dep::new("cart", Hard)]),
            feature(
                "place-order",
                Tier::P0,
                "frontend",
                &["POST /checkout"],
                vec![Dep::new("checkout", Hard).api("PlaceOrder")],
            ),
            feature(
                "checkout",
                Tier::P0,
                "checkout",
                &["PlaceOrder"],
                vec![Dep::new("payment", Hard), Dep::new("shipping", Hard)],
            ),
            feature("payment", Tier::P0, "payment", &["Charge"], vec![]),
            feature("shipping", Tier::P1, "shipping", &[], vec![]),
            feature("catalog", Tier::P1, "catalog", &[], vec![]),
            feature("cart", Tier::P1, "cart", &[], vec![]),
            feature("ads", Tier::P3, "ads", &[], vec![]),
        ],
    }
}

#[test]
fn soft_dependency_failure_touches_only_what_declares_it() {
    // The reported bug: failing ads must not hit place-order, cart-view or the storefront.
    let o = evaluate(&shop(), &down("ads"));
    assert_eq!(impact(&o, "ads"), Impact::Down);
    assert_eq!(impact(&o, "browse"), Impact::Degraded, "soft caps at degraded");
    for f in ["place-order", "cart-view", "storefront", "checkout", "payment"] {
        assert_eq!(impact(&o, f), Impact::None, "{f} does not depend on ads");
    }
}

#[test]
fn hard_chain_reaches_only_features_on_the_path() {
    let o = evaluate(&shop(), &down("shipping"));
    assert_eq!(impact(&o, "checkout"), Impact::Down);
    assert_eq!(impact(&o, "place-order"), Impact::Down);
    assert_eq!(causes(&o, "place-order"), ["shipping"]);
    assert_eq!(impact(&o, "browse"), Impact::None);
    assert_eq!(impact(&o, "cart-view"), Impact::None);
    assert_eq!(impact(&o, "storefront"), Impact::None, "frontend's own availability is fine");
}

#[test]
fn own_failure_hits_every_feature_of_the_service_and_availability_dependents() {
    let o = evaluate(&shop(), &down("frontend"));
    for f in ["browse", "cart-view", "place-order", "storefront"] {
        assert_eq!(impact(&o, f), Impact::Down, "{f}");
    }
    assert_eq!(impact(&o, "checkout"), Impact::None);
}

#[test]
fn causes_only_name_roots_at_the_severity_that_produced_the_impact() {
    let m = Model {
        services: vec![],
        features: vec![feature("f", Tier::P0, "s", &[], vec![Dep::new("hard-root", Hard), Dep::new("soft-root", Soft)])],
    };
    let mut f = down("hard-root");
    f.services.insert(sid("soft-root"), Impact::Down);
    let o = evaluate(&m, &f);
    assert_eq!(impact(&o, "f"), Impact::Down);
    assert_eq!(causes(&o, "f"), ["hard-root"], "soft-root can only degrade, so it is not blamed");

    let only_soft = evaluate(&m, &down("soft-root"));
    assert_eq!((impact(&only_soft, "f"), causes(&only_soft, "f")), (Impact::Degraded, vec!["soft-root".to_string()]));
}

#[test]
fn equal_severity_roots_are_all_named() {
    let m = Model {
        services: vec![],
        features: vec![feature("f", Tier::P0, "s", &[], vec![Dep::new("a", Hard), Dep::new("b", Hard)])],
    };
    let mut f = down("a");
    f.services.insert(sid("b"), Impact::Down);
    assert_eq!(causes(&evaluate(&m, &f), "f"), ["a", "b"]);
}

#[test]
fn dependency_resolution_rules() {
    let m = Model {
        services: vec![],
        features: vec![
            // one feature on `solo`, three on `multi`
            feature("solo-f", Tier::P1, "solo", &["x"], vec![Dep::new("db", Hard)]),
            feature("m1", Tier::P1, "multi", &["a"], vec![Dep::new("db", Hard)]),
            feature("m2", Tier::P1, "multi", &["b"], vec![]),
            feature("m3", Tier::P1, "multi", &["c"], vec![]),
            feature("via-solo", Tier::P1, "c1", &[], vec![Dep::new("solo", Hard)]),
            feature("via-multi-no-api", Tier::P1, "c2", &[], vec![Dep::new("multi", Hard)]),
            feature("via-multi-api", Tier::P1, "c3", &[], vec![Dep::new("multi", Hard).api("a")]),
            feature("via-multi-other", Tier::P1, "c4", &[], vec![Dep::new("multi", Hard).api("b")]),
            feature("via-unmatched-api", Tier::P1, "c5", &[], vec![Dep::new("multi", Hard).api("zzz")]),
        ],
    };
    let o = evaluate(&m, &down("db"));
    assert_eq!(impact(&o, "via-solo"), Impact::Down, "single-feature service: depend on that feature");
    assert_eq!(impact(&o, "via-multi-api"), Impact::Down, "api picks the feature");
    assert_eq!(impact(&o, "via-multi-other"), Impact::None, "a different feature of the same service is unaffected");
    assert_eq!(impact(&o, "via-multi-no-api"), Impact::None, "no api on a multi-feature service means availability only");
    assert_eq!(impact(&o, "via-unmatched-api"), Impact::None);
}

#[test]
fn service_level_dependencies_affect_all_its_features_and_featureless_services_pass_through() {
    let m = Model {
        services: vec![svc("api", vec![Dep::new("db", Hard)]), svc("db", vec![Dep::new("disk", Hard)])],
        features: vec![
            feature("one", Tier::P0, "api", &["1"], vec![]),
            feature("two", Tier::P1, "api", &["2"], vec![]),
            feature("elsewhere", Tier::P1, "other", &[], vec![]),
        ],
    };
    let o = evaluate(&m, &down("disk"));
    assert_eq!((impact(&o, "one"), impact(&o, "two")), (Impact::Down, Impact::Down));
    assert_eq!(causes(&o, "one"), ["disk"]);
    assert_eq!(impact(&o, "elsewhere"), Impact::None);
}

#[test]
fn failing_call_hurts_only_features_that_make_it() {
    let m = Model {
        services: vec![],
        features: vec![
            feature("uses", Tier::P0, "client", &["u"], vec![Dep::new("server", Hard)]),
            feature("ignores", Tier::P0, "client", &["i"], vec![]),
            feature("server-f", Tier::P0, "server", &[], vec![]),
        ],
    };
    let mut f = Faults::default();
    f.edges.insert((sid("client"), sid("server")), Impact::Down);
    let o = evaluate(&m, &f);
    assert_eq!(impact(&o, "uses"), Impact::Down);
    assert_eq!(causes(&o, "uses"), ["server"]);
    assert_eq!(impact(&o, "ignores"), Impact::None);
    assert_eq!(impact(&o, "server-f"), Impact::None, "the server itself is fine");
}

#[test]
fn nodes_show_the_worst_of_the_service_and_its_features() {
    let o = evaluate(&shop(), &down("shipping"));
    assert_eq!(o.nodes[&sid("frontend")].impact, Impact::Down, "place-order lives here");
    assert_eq!(o.services[&sid("frontend")].impact, Impact::None, "but the service itself is fine");
    assert_eq!(o.nodes[&sid("proxy")].impact, Impact::None);
}

#[test]
fn cycles_and_self_dependencies_terminate() {
    let m = Model {
        services: vec![svc("a", vec![Dep::new("a", Hard)])],
        features: vec![
            feature("fa", Tier::P0, "a", &["a"], vec![Dep::new("b", Hard).api("b")]),
            feature("fb", Tier::P0, "b", &["b"], vec![Dep::new("a", Hard).api("a"), Dep::new("c", Hard)]),
        ],
    };
    let o = evaluate(&m, &down("c"));
    assert_eq!((impact(&o, "fa"), impact(&o, "fb")), (Impact::Down, Impact::Down));
    assert_eq!(causes(&o, "fa"), ["c"]);
}

#[test]
fn unknown_faults_are_ignored_and_healthy_model_is_all_none() {
    let o = evaluate(&shop(), &down("ghost"));
    assert!(o.features.values().all(|r| r.impact == Impact::None));
    let o = evaluate(&shop(), &Faults::default());
    assert!(o.nodes.values().all(|r| r.impact == Impact::None && r.causes.is_empty()));
}

#[test]
fn hard_wins_when_edge_declared_twice() {
    let mut g = Graph::new();
    g.add_dependency("a", "b", Soft);
    g.add_dependency("a", "b", Hard);
    g.add_dependency("a", "b", Soft);
    assert_eq!(g.edge_kind(&sid("a"), &sid("b")), Some(Hard));
}

#[test]
fn hysteresis_debounces_both_directions() {
    let mut h = HealthTracker::new(2, 3);
    assert_eq!(h.observe(Impact::Down), Impact::None);
    assert_eq!(h.observe(Impact::None), Impact::None); // blip resets
    assert_eq!(h.observe(Impact::Down), Impact::None);
    assert_eq!(h.observe(Impact::Down), Impact::Down);
    assert_eq!(h.observe(Impact::None), Impact::Down);
    assert_eq!(h.observe(Impact::None), Impact::Down);
    assert_eq!(h.observe(Impact::Down), Impact::Down); // relapse resets clearing
    assert_eq!(h.observe(Impact::None), Impact::Down);
    assert_eq!(h.observe(Impact::None), Impact::Down);
    assert_eq!(h.observe(Impact::None), Impact::None);
}

#[test]
fn thresholds_classify() {
    let t = Thresholds::default();
    assert_eq!(t.classify(0.0, 5.0, None), Impact::None);
    assert_eq!(t.classify(0.1, 5.0, None), Impact::Degraded);
    assert_eq!(t.classify(0.9, 5.0, None), Impact::Down);
    assert_eq!(t.classify(1.0, 0.001, None), Impact::None, "too little traffic to judge");
    assert_eq!(t.classify(f64::NAN, 5.0, None), Impact::None);
    assert_eq!(t.classify(0.5, 0.06, Some(2.0)), Impact::None, "2 requests prove nothing");
    assert_eq!(t.classify(0.5, 0.5, Some(30.0)), Impact::Down);
}
