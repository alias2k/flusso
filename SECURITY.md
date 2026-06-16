# Security Policy

flusso connects to your Postgres database with replication privileges and to your
OpenSearch cluster, and it handles credentials for both. We take security issues
seriously and appreciate responsible disclosure.

## Supported versions

flusso has not yet reached a stable `1.0` release. Until then, security fixes are
applied to the latest released version on the `main` branch only. We recommend
always running the most recent release.

| Version | Supported |
| --- | --- |
| latest `main` / most recent release | ✅ |
| older releases | ❌ |

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues,
discussions, or pull requests.**

Instead, report them privately through GitHub Security Advisories:

1. Go to the [Security Advisories page](https://github.com/alias2k/pgsync_rs/security/advisories).
2. Click **Report a vulnerability**.
3. Provide as much detail as you can (see below).

If you are unable to use GitHub Security Advisories, you may open a draft advisory
or contact the maintainers privately instead of disclosing publicly.

### What to include

To help us triage and fix the issue quickly, please include:

- A description of the vulnerability and its impact.
- Steps to reproduce, or a proof of concept.
- The affected version, configuration, and environment (Postgres/OpenSearch
  versions, deployment mode) where relevant.
- Any suggested mitigation or fix, if you have one.

### What to expect

- We will acknowledge your report as soon as we can.
- We will investigate, keep you informed of progress, and let you know once the
  issue is resolved.
- We will credit you in the advisory once a fix is released, unless you prefer to
  remain anonymous.

Thank you for helping keep flusso and its users safe.
