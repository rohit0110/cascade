# Cascade demo

The demo (OpenTelemetry Demo shop + Prometheus + Jaeger + Grafana + Cascade) is documented in the root
[README](../../README.md#demo-what-opens-where): what opens where, how to start and stop it, how to break
things on purpose, and the test scenarios with results.

What is in this directory:

| Path | What |
|---|---|
| `stack/` | Config-only copy of the OpenTelemetry Demo compose files, plus `compose.cascade.yaml` (adds Cascade, removes the upstream collector's host-root and `docker.sock` mounts, disables opensearch/opamp) and `cascade-collector.yml` (slim collector: spanmetrics + servicegraph) |
| `registry/` | Example `cascade.yaml` for every demo service. The best reference for writing your own |
| `fault.sh` | Toggle the demo's fault-injection flags (`fault.sh list`) |

Upstream demo source is never cloned to the host. It lives in the Docker volume `otel-demo-src`; only the config
files the stack needs were copied out into `stack/`. To refresh from upstream:

    docker run --rm -v otel-demo-src:/src alpine/git -C /src pull
    docker run --rm -v otel-demo-src:/src:ro alpine sh -c 'cd /src; cat compose.yaml' | less   # review before copying anything out
