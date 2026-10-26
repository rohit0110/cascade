# Cascade

**Know which business features break when a service fails, how badly, and who owns them.**

Service maps (Grafana, Tempo, Datadog, New Relic) show *topology*. They don't know that `payment` powers
"Card payment processing", that it is P0, or that the payments team owns it. Cascade adds that missing business
meaning on top of the observability stack you already run, and keeps it honest as code changes.

- Teams declare **features, tiers, owners and dependencies** in a small `cascade.yaml` next to their service.
- Cascade reads **topology and call health** from Prometheus (OpenTelemetry service-graph metrics). It stores no logs or traces.
- When something fails it computes the **blast radius per feature**: "Checkout (P0) is Down because of `cart`, owned by payments-team".
- It **verifies declarations against reality** (`cascade verify`) so the registry doesn't rot.
- It serves a **support-facing UI**, a **Prometheus `/metrics` endpoint** for alerting, and a **JSON API**.

> Status: working prototype, tested against the OpenTelemetry Demo only. See [Known limitations](#known-limitations)
> and the [TODO](#todo--roadmap) before using it for anything that matters.

## Contents

1. [How a company uses it](#how-a-company-uses-it)
2. [Demo: what opens where](#demo-what-opens-where)
3. [Test scenarios (with results)](#test-scenarios-with-results)
4. [`cascade.yaml` reference](#cascadeyaml-reference)
5. [How impact is calculated](#how-impact-is-calculated)
6. [Using Cascade on your own stack (client checklist)](#using-cascade-on-your-own-stack-client-checklist)
7. [Configuration, CLI, API and metrics reference](#configuration-cli-api-and-metrics-reference)
8. [Repository layout](#repository-layout)
9. [Security and operations](#security-and-operations)
10. [Known limitations](#known-limitations)
11. [TODO / roadmap](#todo--roadmap)
12. [Development](#development)
13. [Project status](#project-status)

---

## How a company uses it

The intended model is the one teams already know from Grafana: **one central deployment, connected to the data
you already have, configured as code, used by many people.**

| Role | What they do | Where |
|---|---|---|
| **Platform / SRE team** | Deploys Cascade once (a container), points it at the company Prometheus, makes sure services emit OTel service-graph metrics | `deploy/Dockerfile`, env vars in [Configuration](#configuration-reference) |
| **Service teams** | Add a `cascade.yaml` to their repo (via normal PR review) declaring owner, features, tiers, dependencies, business metrics | [`cascade.yaml` reference](#cascadeyaml-reference), examples in `deploy/demo/registry/` |
| **CI** | Runs `cascade verify` so a new undeclared dependency fails the build | [CLI](#cli) |
| **Product support / on-call** | Opens the UI during an incident: what's affected, how badly (P0..P3), who owns it, what's the likely cause | UI on port 7070 |
| **Alerting** | Prometheus scrapes Cascade's `/metrics`; Alertmanager pages on "P0 feature down" | [Exported metrics](#exported-metrics) |
| **Grafana users** | See impact next to their existing dashboards | Datasource plugin is **TODO**; a JSON endpoint exists today ([details](#grafana)) |

What exists today vs. what is planned is spelled out in the [TODO](#todo--roadmap): there is no SSO, no Helm chart,
and Prometheus is the only data source so far.

---

## Demo: what opens where

The demo runs the OpenTelemetry Demo shop (about 20 microservices), Prometheus, Jaeger, Grafana and Cascade as one
Docker Compose project, with fault injection through feature flags. Nothing is installed on your machine except Docker.

**Requirements:** Docker with about 8 GB of RAM available (what this was tested with). The first start pulls images and
builds Cascade (a cold Rust build takes a few minutes). Ports `7070`, `8080`, `9090` must be free.

### Start / stop

```sh
cd deploy/demo/stack
F="-p cascade-demo -f compose.yaml -f compose.observability.yaml -f compose.cascade.yaml"   # (zsh: use an array)

docker compose $F build cascade        # first time, and after any code change (builds the UI and Rust)
docker compose $F up -d --no-build     # start everything (--no-build: the other images are prebuilt upstream)

docker compose $F down                 # stop and remove containers and the network (images/volumes are kept)
```

Give it 1-2 minutes: Cascade needs a few polls of data before edges and health appear.

### Where everything is

| What | URL | Notes |
|---|---|---|
| **Cascade UI** | http://localhost:7070 | The product. Graph, impact list, expandable nodes, edge drill-down. Bound to localhost only |
| Cascade API | http://localhost:7070/api/graph, `/api/impact`, `/api/features`, `/api/metrics`, `/api/drift` | JSON |
| Cascade metrics | http://localhost:7070/metrics | Prometheus text format, for alerting |
| Demo shop | http://localhost:8080 | Click around / place orders to generate traffic |
| Fault flags UI | http://localhost:8080/feature | Toggle faults in the browser (or use `fault.sh`) |
| Grafana | http://localhost:8080/grafana/ | Demo's own dashboards (Cascade's plugin is not loaded) |
| Jaeger (traces) | http://localhost:8080/jaeger/ui/ | |
| Prometheus | http://localhost:9090 | Query `traces_service_graph_request_total` to see what Cascade reads |
| Cascade logs | `docker logs -f cascade` | |

### Using the UI

- **Graph.** Left-to-right layout, one shape per kind of component: circle = service, hexagon = gateway,
  cylinder = database, cylinder with a bolt = cache, pill = queue, diamond = config/feature flags.
  Curved arrows are calls. Healthy calls stay quiet; failing calls are red (down) or amber (degraded), animated and
  labelled with their error rate. Dashed = soft dependency, dotted = declared but not seen in traffic, purple = seen
  in traffic but not declared.
- **Node colour = severity** (amber degraded, red down). The small badge on a node is the tier (P0..P3) of the worst
  affected feature on it.
- **Click a node** to expand it in place into a card: owner, kind, each feature it serves with its tier and status,
  that feature's business metrics (live values), and inbound traffic. Click again to collapse.
- **Click a call (edge)** for its health, hard/soft, confidence, and a searchable list of APIs on the callee.
- **Impact panel (right)** lists affected features, P0 first, with owner, cause and confidence. Click one to dim
  everything unrelated.
- **Registry health** (right, bottom) lists drift: calls seen but undeclared, declared but not seen, callers without
  a `cascade.yaml`.
- The header shows the freshness of the data ("as of 6s ago", "STALE" if polling has failed).

### Breaking things on purpose

```sh
deploy/demo/fault.sh list                        # all fault flags and their variants
deploy/demo/fault.sh paymentFailure 100%         # fail every payment charge
deploy/demo/fault.sh paymentUnreachable on
deploy/demo/fault.sh paymentFailure off          # recover (do this for every flag you set)
```

Flags persist in `deploy/demo/stack/src/flagd/demo.flagd.json`, so they survive a restart. Check `fault.sh list`
after a break: a flag you forgot about will keep showing up as an incident.

**Generate traffic through the broken path.** The demo's load generator places very few orders, so checkout and
payment edges are nearly idle and Cascade has nothing to judge. Place orders yourself (use the shop at :8080, or a
loop of `POST localhost:8080/api/checkout?currencyCode=USD`). Orders with an empty cart also make `shipping`
error; Cascade will correctly report that too.

Stopping a container also works as a fault (see the Redis scenario below).

---

## Test scenarios (with results)

Each scenario was run against the live demo or replayed against the real demo registry in automated tests
(`crates/cascade-server/tests/engine.rs`). "Live" means a human-observable run on the running demo.

### 1. Failing ads: only the features that depend on ads are affected

*Setup:* flags `adFailure=on` and `adHighCpu=on`. Roughly 6-14% of `frontend -> ad` calls fail.

*Result (live and in tests):* exactly **two** features, both **Degraded**, both caused by `ad`:

| Feature | Tier | Status | Why |
|---|---|---|---|
| Contextual ads | P3 | Degraded | The `ad` service itself is erroring |
| Browse products | P1 | Degraded | Declares `ad` as a **soft** dependency, so it can only degrade |

Not affected, and correctly so: Place order (P0), View and edit cart, Storefront entry (P0). None declares a dependency on ads.

*Why this scenario matters:* an earlier version of the engine computed impact per **service**, so everything hosted
on `frontend` (including three P0/P1 features that never touch ads) lit up. Impact is now computed per **feature**
from that feature's own declared dependencies. See [How impact is calculated](#how-impact-is-calculated).

### 2. Payment outage: the checkout path goes down, and only that

*Setup:* `paymentFailure=100%` plus orders being placed.

*Expected (verified in automated tests with the demo registry; the live run was done before the per-feature fix and
exposed the bugs that fix addresses, so it was not repeated live afterwards):*

- **Down:** Card payment processing (P0), Checkout (P0), Place order (P0). All three name `payment` as the cause, and nothing else.
- **Not affected:** Browse products, View and edit cart, Storefront entry.
- Noise is ignored: `payment -> flagd` failing on 2 requests is below the minimum-request threshold.

### 3. Redis (cart store) down: carts and checkout break, browsing and the storefront stay up

*Setup (live, run once):* `docker stop valkey-cart` (the demo's Redis-compatible cart store), with orders being
placed. Then `docker start valkey-cart` to recover.

*What the registry predicts* (`cascade simulate --down redis`, i.e. the declared model alone):

| Tier | Feature | Why it goes Down |
|---|---|---|
| P0 | Checkout | Hard-depends on the `cart` service's feature |
| P0 | Place order | Hard-depends on Checkout's `PlaceOrder` entrypoint |
| P1 | View and edit cart | Hard-depends on `cart` |
| P1 | Shopping cart storage | Hard-depends on `redis` directly |

*What happened live:*

| Time after stop | Cascade showed |
|---|---|
| +20 s | No change (needs 2 bad polls in a row, plus enough requests in the window) |
| **+40 s** | **4 features Down**: Checkout P0, Place order P0, View and edit cart P1, Shopping cart storage P1 |
| +40 s .. +120 s | Stable. Browse, Payment, Storefront entry never affected |
| restart `valkey-cart` | |
| +60 s after | View and edit cart clearing (Degraded) |
| +80 s after | Cart features cleared |

*The cause was reported as `cart`, not `redis`. Why:* Cascade can only blame what telemetry shows failing. The
demo's tracing records `cart -> redis` calls (738 in 15 minutes) but **never marks them failed**, even with the store
stopped. The failures show up on the calls *into* `cart` (checkout -> cart: 69 failed, frontend -> cart: 79 failed).
So `cart` is the first service whose failure is visible, and it has no failing callee to push the blame down to.
This is a property of the demo's instrumentation (a database client span that isn't flagged as an error), not of
the engine. In a system whose database client spans record errors, the cause would be the database.
Takeaway for adopters: **root-cause accuracy is bounded by how well your services record failed calls.**

*Test artifact to ignore:* shortly after the restart, `shipping` appears Down. That is the empty-cart order script:
once the cart works again, checkout reaches `shipping`, which errors on an empty cart. It is unrelated to Redis.

### Other things checked

- A single failing call path while the server is healthy overall blames that call only (engine test).
- Observed-but-undeclared calls are reported as drift and treated as hard dependencies (engine test, CLI test).
- `cascade verify` exits 1 on an undeclared dependency, 0 when matching (CLI tests).

---

## `cascade.yaml` reference

One file per service (any filename ending `.yaml`/`.yml` in the registry directory; subdirectories are scanned).
Unknown fields are rejected. A complete example is `deploy/demo/registry/checkout.yaml`:

```yaml
service: checkout            # must match the service name in your telemetry (service.name)
owner: payments-team         # who to call. Required, non-empty
kind: service                # optional: service | gateway | database | cache | queue | external | config
depends_on:                  # optional: dependencies of the service as a whole (affect ALL its features)
  - { service: flagd, kind: soft }
features:
  - id: checkout             # unique across the whole registry
    name: Checkout           # display name (defaults to id)
    tier: P0                 # P0 (most critical) .. P3
    entrypoints: ["oteldemo.CheckoutService/PlaceOrder"]   # what callers use to reach this feature
    depends_on:
      - { service: cart,    kind: hard }
      - { service: payment, api: "oteldemo.PaymentService/Charge", kind: hard }
      - { service: email,   kind: soft }          # soft: can only degrade this feature, never take it down
    metrics:                 # business metrics shown on the node card
      - name: "orders placed"
        unit: "/min"
        query: 'sum(rate(traces_span_metrics_calls_total{service_name="checkout",span_name="oteldemo.CheckoutService/PlaceOrder",status_code!="STATUS_CODE_ERROR"}[5m])) * 60'
```

### Dependencies: `kind` and `api`

- **`kind: hard`** (default): if the target fails, this feature fails the same way.
- **`kind: soft`**: the dependency is optional; its failure can only make this feature *Degraded*.
- **`api` says what part of the target you depend on:**
  - `api: "<entrypoint>"` = the feature in the target service whose `entrypoints` contains that string.
  - `api: "*"` = only that the target service is **reachable**, not any particular feature of it
    (e.g. an edge proxy depends on the frontend being up, not on every frontend feature; see `frontend-proxy.yaml`).
  - no `api`: if the target serves exactly one feature, that feature. If it serves several, the service's availability
    only, and the registry prints a warning so it is a conscious choice.

### Business metrics

`metrics[].query` is an instant PromQL query that returns one number. It is evaluated every poll against the
configured Prometheus. A failing query blanks only that metric (the card shows `–`, hover for the reason).
Values are also exported as `cascade_business_metric`. Note: **whoever can edit the registry can run arbitrary
read queries against your Prometheus**, so review registry changes like code.

### Component kinds

`kind` only changes how a node is drawn (cylinder for a database, and so on). Components you don't declare are drawn as plain services.

---

## How impact is calculated

Every poll (default 15 s; demo 10 s):

1. **Read edges** from Prometheus: for each caller -> callee pair, request rate, failed rate, p95 latency, and request
   count over the window.
2. **Classify health.** Error ratio >= 5% is *Degraded*, >= 50% is *Down*. Edges with fewer than 10 requests in the window
   (or under 0.02 req/s) are ignored: "1 of 2 failed" is not "50% errors". Thresholds are configurable.
3. **Debounce.** A change must hold for 2 consecutive polls to be flagged, and 3 good polls to clear. One bad window is noise.
4. **Service health.** A service's health is its error ratio over everything that calls it.
5. **Find root causes.** A service that has a failing callee is a *symptom*, not a cause. An edge that fails while its
   server looks healthy overall blames that one call only.
6. **Compute impact per feature.** A feature is affected by (a) its service's own failure and service-level
   `depends_on`, and (b) the specific dependencies it declares. A hard dependency passes severity through, a soft one
   caps it at Degraded. Other features on the same service are not affected unless they share the cause.
7. **Name causes honestly.** A feature's cause list contains only the roots at the severity that produced its status.
   A root behind a soft edge is not blamed for something that is Down.
8. **Label confidence.** *High* = declared and seen in traffic. *Medium* = seen but not declared. *Low* = declared but
   not seen. The UI always shows when the data was last refreshed.

Calls seen in telemetry but missing from the registry are added to the model as hard, service-wide dependencies of
the caller (the conservative reading) and reported as drift.

Source: `crates/cascade-core/src/reach.rs` (impact), `crates/cascade-server/src/engine.rs` (health, causes, snapshot).

---

## Using Cascade on your own stack (client checklist)

### What you need to provide

| # | What | Required? | Example in this repo |
|---|---|---|---|
| 1 | **Services instrumented with OpenTelemetry** (traces), with a stable `service.name`, and failed calls recorded as errors | Yes | the OTel Demo services |
| 2 | **An OpenTelemetry Collector** (or Tempo metrics-generator) that turns traces into **service-graph** metrics | Yes | `deploy/demo/stack/cascade-collector.yml` |
| 3 | **A Prometheus-compatible query API** holding those metrics | Yes | Prometheus in `deploy/demo/stack/compose.observability.yaml` |
| 4 | **A `cascade.yaml` per service**, collected into one directory Cascade can read | Yes | `deploy/demo/registry/*.yaml` |
| 5 | **A place to run one container** (Docker, Kubernetes, VM) with network access to Prometheus | Yes | `deploy/Dockerfile`, `deploy/demo/stack/compose.cascade.yaml` |
| 6 | Span metrics (for the per-API list) | Optional | same collector file |
| 7 | Business metrics already in Prometheus | Optional | `metrics:` in `deploy/demo/registry/*.yaml` |
| 8 | A CI step running `cascade verify` | Recommended | [CLI](#cli) |
| 9 | Prometheus scraping Cascade's `/metrics` + an alert rule | Recommended | [Alerting](#alerting) |

### Step 1-3: telemetry

Cascade reads these Prometheus series (names as produced by the OpenTelemetry Collector connectors):

| Series | Labels used | Needed for |
|---|---|---|
| `traces_service_graph_request_total` | `client`, `server` | topology, request rate, volume |
| `traces_service_graph_request_failed_total` | `client`, `server` | error ratio |
| `traces_service_graph_request_server_seconds_bucket` | `client`, `server`, `le` | p95 latency |
| `traces_span_metrics_calls_total` | `service_name`, `span_name`, `span_kind`, `status_code` | per-API list (optional) |
| `traces_span_metrics_duration_milliseconds_bucket` | `service_name`, `span_name`, `span_kind`, `le` | per-API latency (optional) |

The collector configuration that produces them (full file: `deploy/demo/stack/cascade-collector.yml`):

```yaml
connectors:
  spanmetrics: {}
  servicegraph:
    latency_histogram_buckets: [100ms, 250ms, 1s, 5s]
    store: { ttl: 5s, max_items: 10000 }
    metrics_flush_interval: 10s
exporters:
  otlp_http/prometheus:
    endpoint: http://prometheus:9090/api/v1/otlp     # Prometheus needs --web.enable-otlp-receiver
service:
  pipelines:
    traces:  { receivers: [otlp], exporters: [otlp_grpc/jaeger, spanmetrics, servicegraph] }
    metrics: { receivers: [otlp, spanmetrics, servicegraph], exporters: [otlp_http/prometheus] }
```

Any other route that lands the same series in Prometheus works (remote write, Mimir/Thanos/Cortex, VictoriaMetrics).
Tempo's metrics-generator produces `traces_service_graph_*` with the same labels, but its span metrics use different
names, so the per-API list and any business metric that uses span metrics would need their queries adapted.

**Instrumentation quality limits accuracy.** Cascade can only blame what telemetry shows failing (see the Redis
scenario). Calls to databases and caches should be recorded as client spans with an error status when they fail.

### Step 4: the registry

Create the directory Cascade reads (one file per service, any layout inside it):

```
registry/
  checkout.yaml          # see deploy/demo/registry/checkout.yaml
  payment.yaml           # see deploy/demo/registry/payment.yaml
  redis.yaml             # infrastructure: service + owner + kind: cache, no features
  ...
```

Bootstrap it from live traffic instead of writing from scratch (generates `depends_on` for every observed caller,
with placeholder owners and no features, all dependencies `hard`):

```sh
docker run --rm -v "$PWD/registry:/out" --entrypoint /usr/local/bin/cascade cascade:local \
  init --prometheus http://prometheus:9090 --out /out --ignore unknown
```

(Tested on Docker Desktop. On Linux the container runs as a non-root user, so the mounted `registry/` directory must be
writable by it, or add `--user $(id -u)`.)

Then humans fill in `owner`, add `features` with `tier`, and mark optional dependencies `kind: soft`.
In a real company each service's file lives in that service's repo, and a pipeline copies them into the directory
Cascade reads (git sync is a TODO).

### Step 5: run it

```sh
docker build -f deploy/Dockerfile -t cascade:local .       # no published image yet

docker run -d --name cascade --restart unless-stopped \
  -p 127.0.0.1:7070:7070 \
  -v "$PWD/registry:/registry:ro" \
  -e CASCADE_PROMETHEUS=http://your-prometheus:9090 \
  -e CASCADE_IGNORE=unknown,load-generator \
  cascade:local
```

The image is read-only-friendly, non-root, about 70 MB. A Compose example with hardening options is the `cascade`
service in `deploy/demo/stack/compose.cascade.yaml`. There is no Helm chart yet.

`CASCADE_IGNORE` is for names that appear in service-graph data but are not business components: the OTel
`unknown` virtual node, load generators, browsers.

### Step 8: CI

```sh
cascade verify --registry ./registry --prometheus http://your-prometheus:9090 --ignore unknown
#  exit 0: matches   exit 1: a service calls something its cascade.yaml doesn't declare   exit 2: error
cascade verify ... --strict      # also fails on declared-but-unseen calls and callers without a cascade.yaml
cascade verify --registry ./registry --observed edges.json   # offline, from a saved JSON file
```

### Alerting

Add a scrape job for `cascade:7070/metrics`, then alert on business impact rather than on a service:

```yaml
- alert: P0FeatureDown
  expr: cascade_feature_impact{tier="P0"} == 2
  for: 2m
  labels: { severity: page }
  annotations:
    summary: "{{ $labels.feature }} is down (owner: {{ $labels.owner }})"
```

(`/metrics` output is covered by tests. Scraping it with a real Prometheus and Alertmanager has **not** been tested.)

### Grafana

- **Today:** Cascade has its own UI. `GET /api/grafana/nodegraph` returns nodes and edges using the field names of
  Grafana's Node Graph panel, so a generic JSON datasource can feed that panel. **Untested in a real Grafana.**
  You can also chart `cascade_feature_impact` from Prometheus in any dashboard.
- **Planned:** a Cascade datasource plugin (source in `plugins/grafana-datasource/`, **never built or loaded**) so impact
  appears inside existing Grafana dashboards. See the TODO.

---

## Configuration, CLI, API and metrics reference

### Configuration reference

Flags or environment variables on `cascade-server`:

| Env var | Default | Meaning |
|---|---|---|
| `CASCADE_PROMETHEUS` | `http://prometheus:9090` | Prometheus-compatible base URL |
| `CASCADE_REGISTRY` | `/registry` | Directory of `cascade.yaml` files, re-read every poll |
| `CASCADE_LISTEN` | `0.0.0.0:7070` | Bind address |
| `CASCADE_POLL_SECS` | `15` | Poll interval |
| `CASCADE_WINDOW` | `2m` | PromQL range for rates. Must cover several metric push intervals |
| `CASCADE_IGNORE` | `unknown` | Comma-separated names to disregard |
| `CASCADE_DEGRADED_ERROR_RATIO` | `0.05` | Error ratio at which a call is Degraded |
| `CASCADE_DOWN_ERROR_RATIO` | `0.5` | Error ratio at which a call is Down |
| `CASCADE_MIN_RPS` | `0.02` | Ignore calls below this request rate |
| `CASCADE_MIN_REQUESTS` | `10` | Ignore error ratios computed from fewer requests in the window |
| `CASCADE_RAISE_AFTER` | `2` | Consecutive bad polls before flagging |
| `CASCADE_CLEAR_AFTER` | `3` | Consecutive good polls before clearing |
| `RUST_LOG` | `info` | Log level |

The demo uses `POLL_SECS=10`, `WINDOW=1m` and ignores `unknown,user,frontend-web,load-generator`
(see `deploy/demo/stack/compose.cascade.yaml`).

### CLI

`cascade` (installed in the image at `/usr/local/bin/cascade`):

| Command | What it does |
|---|---|
| `cascade verify` | Compare the registry with observed calls. Exit 1 on undeclared dependencies |
| `cascade init` | Write starter `cascade.yaml` files from observed traffic. Won't overwrite without `--force` |
| `cascade simulate --down a,b --degraded c` | What breaks if these services fail? Uses the declared registry only, no telemetry needed |

### HTTP API

| Endpoint | Returns |
|---|---|
| `GET /api/graph` | Nodes (kind, owner, impact, causes, traffic), edges (health, kind, confidence), data freshness |
| `GET /api/impact` | Currently affected features, worst first |
| `GET /api/features` | All declared features with current status |
| `GET /api/metrics` | Declared business metrics with latest values (or the reason there is none) |
| `GET /api/drift` | Registry drift and warnings |
| `GET /api/edges/{client}/{server}/apis?q=` | Searchable APIs on the callee (callee-wide, not per caller) |
| `GET /api/events` | Server-sent events: nudges clients to refetch on each new snapshot |
| `GET /api/grafana/nodegraph` | Node Graph panel shaped JSON |
| `GET /metrics` | Prometheus exposition |
| `GET /healthz` | `ok` |

Every data response carries `as_of_unix`, `age_seconds`, `stale` (true if older than 3 poll intervals) and `last_error`.

### Exported metrics

| Metric | Labels | Value |
|---|---|---|
| `cascade_feature_impact` | `feature`, `tier`, `owner` | 0 none, 1 degraded, 2 down (every declared feature is emitted, zeros included) |
| `cascade_service_impact` | `service` | 0/1/2 |
| `cascade_edge_health` | `client`, `server` | 0 ok, 1 degraded, 2 failing |
| `cascade_business_metric` | `feature`, `metric` | latest value of a declared business metric |
| `cascade_drift_undeclared_edges`, `cascade_drift_unobserved_edges` | | counts |
| `cascade_snapshot_timestamp_seconds` | | unix time of the last good poll |
| `cascade_poll_errors_total` | | counter |

---

## Repository layout

```
Cargo.toml                      Rust workspace
crates/
  cascade-core/                 pure logic, no I/O
    src/reach.rs                  per-feature impact evaluation (the heart of it)
    src/model.rs                  Dep / FeatureSpec / ServiceSpec
    src/health.rs                 debounce (hysteresis)
    src/observed.rs               observed edge types, thresholds
    src/graph.rs, types.rs
    tests/impact.rs               14 tests
  cascade-registry/             cascade.yaml schema, loader, validation, drift check (verify)
  cascade-adapters/             data sources. TopologySource trait + Prometheus implementation
  cascade-server/               the service
    src/engine.rs                 health, root-cause pruning, snapshot assembly
    src/poller.rs                 poll loop, business-metric evaluation
    src/api.rs                    REST, SSE, Grafana JSON, embedded UI
    src/metrics.rs                /metrics exposition
    tests/engine.rs               engine tests incl. scenarios replayed against the demo registry
    tests/api.rs
  cascade-cli/                  `cascade verify | init | simulate`
web/                            UI: React + TypeScript + Vite + React Flow
  src/SvcNode.tsx                 node and expanded card
  src/Glyph.tsx                   shape per component kind
  src/GraphView.tsx, layout.ts, model.ts, Panels.tsx, App.tsx
plugins/grafana-datasource/     Grafana datasource plugin source (NOT built or tested)
deploy/
  Dockerfile                    builds UI, then Rust, into a ~70 MB non-root image
  dev/compose.yaml              hardened Rust and Node toolchain containers (nothing installed on the host)
  demo/
    registry/*.yaml               example cascade.yaml for every demo service
    stack/                        config-only copy of the OTel Demo + Cascade overlay
      compose.cascade.yaml          adds Cascade, removes risky mounts, disables opensearch/opamp
      cascade-collector.yml         slim collector: spanmetrics + servicegraph
    fault.sh                      toggle fault flags
docs/architecture.md            original design document
```

---

## Security and operations

- **Cascade has no authentication.** Bind it to localhost, or put it behind a VPN or an authenticating reverse proxy.
  It reveals your service names, owners and health.
- **It is read-only toward your systems:** it only issues Prometheus queries. It stores nothing on disk; state is
  in memory and rebuilt every poll. Restarting loses only the debounce state.
- **Registry = trusted input.** `metrics[].query` is run against your Prometheus. Review registry changes.
- **Demo hardening.** The upstream OTel Demo collector mounts the host root filesystem and `docker.sock` as root.
  This repo's overlay replaces that: the demo never touches the Docker socket or host filesystem. Cascade runs
  read-only, non-root, all capabilities dropped. Upstream demo source stays in a Docker volume (`otel-demo-src`), not
  on the host. Dev toolchains also run in locked-down containers.
- **Stale data is labelled**, not hidden: on poll failure Cascade keeps the last good snapshot and shows its age.

---

## Known limitations

1. **Root cause is bounded by instrumentation.** If a failed call isn't recorded as failed (Redis scenario), the blame
   lands one hop up.
2. **Root-cause pruning uses debounced state**, so a service can briefly look like a cause before its failing callee is
   flagged.
3. **Idle failed edges linger.** A call with no traffic is treated as healthy, so a failed one stays flagged until the
   clear window passes.
4. **The API list on an edge is per callee, not per caller→callee**, because service-graph metrics have no route label.
5. **Prometheus with OTel-collector metric names only.** Series names are fixed in code.
6. **Not tested beyond the OTel Demo.** No large-graph performance test, no multi-replica Prometheus setups tested.
7. The UI was verified in headless Chromium (dark and light screenshots loaded, dark inspected). Other browsers untested.
8. Declared calls that low traffic hides show as "declared, not seen" under Registry health. Benign.

---

## TODO / roadmap

**Data sources / connectors (planned for later)**
- [ ] Vendor connectors: New Relic, Datadog, Chronosphere (query APIs)
- [ ] Tempo / Jaeger as topology source (and dependency *suggestions* from traces, human-confirmed)
- [ ] OTLP "tap": a second exporter on the collector, for stacks without a Prometheus
- [ ] Configurable metric/label names (Tempo span metrics, other conventions)
- [ ] Per-caller API lists (needs route dimensions on the service graph)

**Integration**
- [ ] Grafana datasource plugin: build, sign, load, test (source exists in `plugins/grafana-datasource/`)
- [ ] Verify scraping `/metrics` with a real Prometheus + Alertmanager, ship an example rules file
- [ ] Notifications (Slack, PagerDuty webhooks) for P0/P1 impact
- [ ] Backstage catalog sync (owners/tiers from Backstage, or push to it)

**Registry**
- [ ] Git sync of per-repo `cascade.yaml` (today: a directory re-read each poll)
- [ ] GitHub Action / CI template for `cascade verify`
- [ ] JSON Schema for `cascade.yaml` (editor completion and validation)
- [ ] Map `server="unknown"` virtual nodes (databases, third parties) instead of ignoring them

**Product**
- [ ] Authentication: OIDC/SSO, roles (viewer / editor)
- [ ] Incident history: persisted timeline, "what was affected at 14:05"
- [ ] Persistence option (SQLite/Postgres) for history and debounce state across restarts
- [ ] SLO / error-budget links per feature
- [ ] Better root-cause: prune on raw state, stability window before naming a cause

**Packaging and community**
- [ ] `git init`, LICENSE file (workspace declares Apache-2.0, no LICENSE file yet), CONTRIBUTING, CODE_OF_CONDUCT
- [ ] CI (build, test, lint) and a published multi-architecture image (amd64/arm64)
- [ ] Helm chart and Kubernetes example manifests
- [ ] "Bring your own Prometheus" quickstart (compose file with an external Prometheus)
- [ ] Pin container images by digest (TODO in `deploy/dev/compose.yaml`)

**Quality**
- [ ] Playwright test suite (inject a fault with `fault.sh`, assert the right features change, assert recovery)
- [ ] Large-graph performance test (500+ nodes)
- [ ] Light-mode and cross-browser visual checks
- [ ] Verify the other fault flags (cart, product catalog, email) and document each

---

## Development

Nothing needs installing on the host: toolchains run in containers (`deploy/dev/compose.yaml`: non-root, all
capabilities dropped, read-only root, only the repo mounted, no Docker socket).

```sh
docker compose -f deploy/dev/compose.yaml run --rm rust cargo test --workspace      # 48 tests
docker compose -f deploy/dev/compose.yaml run --rm node sh -c 'cd web && npm test'  # 8 tests
docker compose -f deploy/dev/compose.yaml run --rm node sh -c 'cd web && npm run build'
docker compose -f deploy/dev/compose.yaml run --rm rust cargo run -q -p cascade-cli -- simulate \
    --registry deploy/demo/registry --down redis
```

First-run gotchas:
- The cargo and npm cache volumes are created root-owned. Fix once:
  `docker run --rm -v dev_cargo-cache:/cache --cap-drop=ALL --cap-add=CHOWN alpine chown 1000:1000 /cache` (same for `dev_npm-cache`).
- In zsh, keep compose `-f` flags in an array, not a string variable.
- UI development against the demo: `cd web && npm run dev` proxies `/api` to `localhost:7070`.

Visual checks without a browser tool: screenshot the running UI from a throwaway Playwright container
(`mcr.microsoft.com/playwright:v1.48.0-jammy`, target `host.docker.internal:7070`).

---

## Project status

**Verified**
- 48 Rust tests and 8 UI tests pass. They include engine scenarios replayed against the real demo registry
  (failing ads, payment outage, shipping outage, low-volume noise).
- Live on the OTel Demo: topology and health discovery, business metrics, failing-ads scenario, Redis outage
  scenario (detection in about 40 s, recovery in about 60-80 s), UI rendering (shapes, curves, expandable cards, fit-to-view).

**Not verified**
- Grafana plugin (never built), Grafana node-graph JSON in a real Grafana, Prometheus scraping Cascade's `/metrics`.
- Anything outside the OTel Demo. No Playwright suite, no performance test.
- The post-fix payment outage was verified by tests only, not re-run live.

License: not finalised. The Cargo workspace declares Apache-2.0, but there is no LICENSE file yet.
