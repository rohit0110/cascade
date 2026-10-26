# Cascade Grafana datasource (source only)

Shows Cascade's impact graph in Grafana's Node Graph panel and the feature impact table, by querying a
Cascade server through Grafana's datasource proxy (`/api/grafana/nodegraph`, `/api/features`).

**Status: written, not built, not loaded into Grafana.** Nothing here has been compiled or run. Building a
signed-or-unsigned Grafana plugin needs Grafana's plugin toolchain (`@grafana/create-plugin` webpack config),
which is not set up yet. Until then, the same JSON can be used with a stock JSON datasource: point it at
`http://cascade:7070/api/grafana/nodegraph` and map the `nodes` and `edges` arrays to Node Graph fields.
Alerting does not need the plugin: scrape Cascade's `/metrics` (`cascade_feature_impact`) with Prometheus.
