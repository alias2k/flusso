"""The two copies of flusso's alerts stay one: the dev Prometheus rules and the
Helm chart's PrometheusRule.

The chart's template is rendered by hand, with the chart's default values, so
no helm is needed: Helm's literal braces (`{{ "{{" }}`) become Prometheus's,
and `{{ .Values.prometheusRule.<key> }}` becomes that key's default. Both
copies' `flusso_alerts` rules are then compared line by line, comments and
blank lines aside and each block dedented, so an alert changed in one copy
and not the other fails here.

Run: python3 -m unittest discover -s .github/scripts -p 'test_*.py'
"""

import re
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEV_RULES = ROOT / "dev/prometheus/rules/flusso.rules.yml"
TEMPLATE = ROOT / "deploy/helm/flusso/templates/prometheusrule.yaml"
VALUES = ROOT / "deploy/helm/flusso/values.yaml"


def chart_defaults(values: str) -> dict:
    """The `prometheusRule:` block's scalar defaults."""
    defaults = {}
    inside = False
    for line in values.splitlines():
        if line.startswith("prometheusRule:"):
            inside = True
            continue
        if inside and line and not line.startswith(" "):
            break
        match = re.match(r"^  (\w+): *([^#]*?) *(#.*)?$", line)
        if inside and match:
            defaults[match.group(1)] = match.group(2).strip('"')
    return defaults


def render(template: str, defaults: dict) -> str:
    """The template's text as Helm would render it with `defaults`."""
    text = template.replace('{{ "{{" }}', "{{").replace('{{ "}}" }}', "}}")

    def value(match: re.Match) -> str:
        key = match.group(1)
        if key not in defaults:
            raise KeyError(f"no default for prometheusRule.{key} in values.yaml")
        return defaults[key]

    return re.sub(r"\{\{ \.Values\.prometheusRule\.(\w+)(?: \| int64)? \}\}", value, text)


def alert_rules(text: str) -> list:
    """The rules of the `flusso_alerts` group, without comments or blank lines,
    dedented to the rules list."""
    lines = text.splitlines()
    start = next(i for i, line in enumerate(lines) if line.strip() == "- name: flusso_alerts")
    rules = next(i for i in range(start, len(lines)) if lines[i].strip() == "rules:")
    group_indent = len(lines[start]) - len(lines[start].lstrip())
    body = []
    for line in lines[rules + 1 :]:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("{{-"):
            break
        indent = len(line) - len(line.lstrip())
        if indent <= group_indent:
            break
        body.append(line)
    return textwrap.dedent("\n".join(body)).splitlines()


class AlertRuleCopies(unittest.TestCase):
    def test_the_chart_renders_the_dev_rules_with_its_defaults(self):
        dev = alert_rules(DEV_RULES.read_text())
        chart = alert_rules(render(TEMPLATE.read_text(), chart_defaults(VALUES.read_text())))
        self.assertTrue(dev, "no flusso_alerts rules found in the dev rules")
        self.assertEqual(dev, chart, "the chart's PrometheusRule differs from dev/prometheus")

    def test_every_alert_links_its_runbook(self):
        for path in (DEV_RULES, TEMPLATE):
            rules = alert_rules(path.read_text())
            alerts = sum(1 for line in rules if line.lstrip().startswith("- alert:"))
            links = sum(1 for line in rules if line.lstrip().startswith("runbook_url:"))
            self.assertEqual(alerts, links, f"{path.name}: an alert without a runbook_url")

    def test_a_drifted_copy_is_caught(self):
        dev = DEV_RULES.read_text()
        drifted = dev.replace("> 600", "> 900", 1)
        self.assertNotEqual(alert_rules(dev), alert_rules(drifted))


if __name__ == "__main__":
    unittest.main()
