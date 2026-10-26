# Cascade architecture

Cascade is a self-hosted business-impact layer that complements the service map you already have. Teams declare features, tiers and owners in a `cascade.yaml` per repo. Cascade reads topology and edge health from your existing observability stack, computes the blast radius when something fails, checks declarations against what is actually observed, and publishes impact as a support-facing UI, metrics, and a Grafana datasource.

It does not store logs or traces, and it does not replace your service map.

## High-level overview

```
 ┌──────────────────────────── YOUR SYSTEMS (not Cascade) ────────────────────────────┐
 │                                                                                     │
 │  Microservices (any language, OTel SDK / auto-instrumentation)                      │
 │  [frontend]──►[checkout]──►[payment]──►[(payments-db)]                              │
 │       │            └──────►[cart]──►[(redis)]                                       │
 │       └──►[recommendation]   [shipping]──►[email]   [product-catalog]──►[(pg)]     │
 │                    │ traces · metrics · logs (OTLP)                                 │
 │                    ▼                                                                │
 │           ┌──────────────────┐   spanmetrics / servicegraph connectors              │
 │           │ OTel Collector   │──────────────────────────────┐                       │
 │           └─┬────────┬───────┘                              │                       │
 │             │        │ (optional "tap": 2nd exporter) ┈┈┈┈┈┈┼┈┈┈┈┈┈┈┐               │
 │             ▼        ▼                                      │       ┊               │
 │   ┌───────────────┐ ┌──────────────┐   ┌ ─ ─ ─ ─ ─ ─ ─ ─ ┐│       ┊               │
 │   │ Prometheus    │ │ Tempo/Jaeger │   │ Vendors (nodes): ││       ┊               │
 │   │ (metrics)     │ │ (traces)     │   │ New Relic ·      ││       ┊               │
 │   └──────┬────────┘ └──────┬───────┘   │ Chronosphere ·   ││       ┊               │
 │          │ PromQL          │ search    │ Datadog          ││       ┊               │
 │          │                 │           └ ─ ─ ─┬─ ─ ─ ─ ─ ┘│       ┊               │
 │   ┌──────┴─────────────────┴──┐               │ query API  │       ┊               │
 │   │ Grafana (service map =    │               │ (PromQL/   │       ┊               │
 │   │ Tempo servicegraph)       │               │  NRQL…)    │       ┊               │
 │   └───────────▲───────────────┘               │            │       ┊               │
 └───────────────┼───────────────────────────────┼────────────┼───────┼───────────────┘
                 │ Node Graph frames             │            │       ┊
 ┌───────────────┼───────────────────────────────▼────────────▼───────▼───────────────┐
 │               │             C A S C A D E  (one self-hosted deployment, Rust)       │
 │               │                                                                     │
 │  ┌────────────┴───┐   ┌──────────────────────┐   ┌───────────────────────────┐     │
 │  │ Output adapters│◄──│ Impact engine        │◄──│ Input adapters            │     │
 │  │ • Grafana DS   │   │ • health per edge    │   │ • Prometheus (servicegraph│     │
 │  │ • /metrics     │   │   (err/latency,      │   │   metrics) ← default      │     │
 │  │   (OTLP/remote │   │   hysteresis)        │   │ • Tempo/Jaeger (traces →  │     │
 │  │   write)       │   │ • reverse reachability│  │   dependency suggestions) │     │
 │  │ • REST + SSE   │   │ • tier × hard/soft    │  │ • Vendor APIs (later)     │     │
 │  └──────┬─────────┘   └──────────▲───────────┘   │ • OTLP tap (later)        │     │
 │         │                        │               └───────────────────────────┘     │
 │         ▼               ┌────────┴──────────┐                                      │
 │  ┌──────────────┐       │ Registry          │◄── git sync / API ◄── cascade.yaml   │
 │  │ Support UI   │       │ features·tiers·   │     per service repo                 │
 │  │ (React +     │       │ owners·deps·      │                                      │
 │  │  Cytoscape)  │       │ confidence        │      ┌─────────────────────┐         │
 │  └──────────────┘       └───────────────────┘      │ CLI: cascade verify │◄─ CI    │
 │                                                    │ (declared vs observed)       │
 └─────────────────────────────────────────────────────────────────────────────────────┘
        ▲ Product support / on-call           Solid = MVP   Dotted/dashed = later
```

## Scope

**In:** `cascade.yaml` schema and aggregator; topology/health adapters (Prometheus first); impact engine; `cascade verify` drift check; confidence/freshness labels; REST/SSE API; standalone UI (edge click shows a searchable API list, healthy edges stay quiet, failing edges go red, impacted nodes and features are colored by tier); impact metrics export; Grafana datasource plugin.

**Inferred:** edge health, suggested dependencies from traces (human-confirmed), drift between declared and observed.

**Not inferred:** business tier or feature meaning; hard vs. soft dependency (hint only).

**Out:** log or trace storage; an engineer-facing map as a product; a log-format mandate; custom coloring inside closed vendor maps (tags and metrics only); auto-assigned severity.

## Demo environment

Use the OpenTelemetry Demo (Astronomy Shop) as the noise generator: polyglot microservices, a Collector, Prometheus/Grafana/Jaeger, a load generator, and flagd feature flags that inject faults. Cascade runs as an extra container with a `cascade.yaml` per demo service. Exact flag names and bundled versions to be verified when we stand it up.

## Decisions

- Complement the service map; do not replace it.
- Rust backend; OTel is the standard input.
- Open-source stack first (Prometheus, Tempo, Grafana); vendor adapters later.
- Per-repo `cascade.yaml` with a CI `verify` step, so declarations stay true as code changes.
- Own standalone UI first; Grafana datasource plugin second; no Grafana core PR.
