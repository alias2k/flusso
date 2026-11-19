#!/usr/bin/env bash
# flusso's alert rules and the Helm chart, checked with the real tools from
# their Docker images (no install): the dev Prometheus config as compose
# mounts it, the rule unit tests, `helm lint`, and the chart's rendered
# PrometheusRule through promtool. Run from the repository root.
# The two rule copies are kept equal by test_alert_rule_copies.py.
set -euo pipefail

PROMETHEUS=prom/prometheus:v3.5.0
HELM=alpine/helm:3.16.4
P="$PWD/dev/prometheus"
CHART="$PWD/deploy/helm/flusso"

echo "== the dev Prometheus config and every file under rules/"
docker run --rm --entrypoint promtool \
  -v "$P/prometheus.yml:/etc/prometheus/prometheus.yml:ro" -v "$P/rules:/etc/prometheus/rules:ro" \
  "$PROMETHEUS" check config /etc/prometheus/prometheus.yml

echo "== the rule unit tests"
docker run --rm --entrypoint promtool -v "$P:/p:ro" \
  "$PROMETHEUS" test rules /p/tests/flusso.rules.test.yml

echo "== the chart, PrometheusRule on"
docker run --rm -v "$CHART:/chart:ro" "$HELM" lint /chart --set prometheusRule.enabled=true

echo "== the chart's rendered rules"
docker run --rm -v "$CHART:/chart:ro" "$HELM" \
  template flusso /chart --set prometheusRule.enabled=true \
  --show-only templates/prometheusrule.yaml \
  | sed '1,/^spec:$/d; s/^  //' \
  | docker run --rm -i --entrypoint sh "$PROMETHEUS" \
      -c 'cat > /tmp/rules.yml && promtool check rules /tmp/rules.yml'
