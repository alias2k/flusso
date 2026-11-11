#!/usr/bin/env bash
# Run the four in-process (no Docker) Criterion benches, saving their results
# under one Criterion baseline name so two runs can be compared side by side.
#
#   bench-in-process.sh <baseline-name> [<bench-id>]
#
# With a bench id (a Criterion full id such as `live_drain/sinks1/64`), only
# that one benchmark runs, matched exactly: `--exact`, so `live_drain/sinks1/6`
# never picks up `live_drain/sinks1/64`. Each bench target is still invoked;
# the ones that don't hold the id run nothing.
#
# The pgoutput and render benches sit behind each crate's `bench` feature, which
# exposes the crate-private decoder / renderer to them. The script bench runs only
# where the crate exists, so a PR base that predates it still yields a baseline.
set -euo pipefail
baseline="${1:?usage: bench-in-process.sh <baseline-name> [<bench-id>]}"
args=(--save-baseline "$baseline")
if [ -n "${2:-}" ]; then
  args+=(--exact "$2")
fi
cargo bench -p flusso-engine --bench engine -- "${args[@]}"
cargo bench -p flusso-source-postgres --bench pgoutput --features bench -- "${args[@]}"
cargo bench -p flusso-sink-opensearch --bench render --features bench -- "${args[@]}"
if [ -d libs/2-script ]; then
  cargo bench -p flusso-script --bench scripted -- "${args[@]}"
fi
