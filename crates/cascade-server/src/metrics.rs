use std::fmt::Write;

use crate::state::Snapshot;

fn esc(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

/// Prometheus text exposition of the current impact picture. Every declared
/// feature and every known edge is emitted (zeros included) so alerts can rely
/// on the series existing.
pub fn render(s: &Snapshot) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# HELP cascade_feature_impact Business feature impact: 0 none, 1 degraded, 2 down.");
    let _ = writeln!(o, "# TYPE cascade_feature_impact gauge");
    for f in &s.features {
        let _ = writeln!(
            o,
            "cascade_feature_impact{{feature=\"{}\",tier=\"{:?}\",owner=\"{}\"}} {}",
            esc(&f.id), f.tier, esc(&f.owner), f.impact.level()
        );
    }
    let _ = writeln!(o, "# HELP cascade_service_impact Service impact: 0 none, 1 degraded, 2 down.");
    let _ = writeln!(o, "# TYPE cascade_service_impact gauge");
    for n in &s.nodes {
        let _ = writeln!(o, "cascade_service_impact{{service=\"{}\"}} {}", esc(&n.id), n.impact.level());
    }
    let _ = writeln!(o, "# HELP cascade_edge_health Debounced call health: 0 ok, 1 degraded, 2 failing.");
    let _ = writeln!(o, "# TYPE cascade_edge_health gauge");
    for e in &s.edges {
        let _ = writeln!(
            o,
            "cascade_edge_health{{client=\"{}\",server=\"{}\"}} {}",
            esc(&e.client), esc(&e.server), e.health.level()
        );
    }
    let _ = writeln!(o, "# HELP cascade_business_metric Latest value of a business metric declared in cascade.yaml.");
    let _ = writeln!(o, "# TYPE cascade_business_metric gauge");
    for m in s.metrics.iter().filter(|m| m.value.is_some()) {
        let _ = writeln!(
            o,
            "cascade_business_metric{{feature=\"{}\",metric=\"{}\"}} {}",
            esc(&m.feature), esc(&m.name), m.value.unwrap_or(0.0)
        );
    }
    let _ = writeln!(o, "# TYPE cascade_drift_undeclared_edges gauge");
    let _ = writeln!(o, "cascade_drift_undeclared_edges {}", s.drift.undeclared.len());
    let _ = writeln!(o, "# TYPE cascade_drift_unobserved_edges gauge");
    let _ = writeln!(o, "cascade_drift_unobserved_edges {}", s.drift.unobserved.len());
    let _ = writeln!(o, "# TYPE cascade_snapshot_timestamp_seconds gauge");
    let _ = writeln!(o, "cascade_snapshot_timestamp_seconds {}", s.as_of_unix);
    let _ = writeln!(o, "# TYPE cascade_poll_errors_total counter");
    let _ = writeln!(o, "cascade_poll_errors_total {}", s.poll_errors_total);
    o
}
