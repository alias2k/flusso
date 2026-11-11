#!/usr/bin/env bash
# Run the four in-process (no Docker) Criterion benches, saving their results
# under one Criterion baseline name so two runs can be compared side by side.
#
#   bench-in-process.sh <baseline-name>
#
# The pgoutput and render benches sit behind each crate's `bench` feature, which
# exposes the crate-private decoder / renderer to them. The script bench runs only
# where the crate exists, so a PR base that predates it still yields a baseline.
set -euo pipefail
baseline="${1:?usage: bench-in-process.sh <baseline-name>}"
cargo bench -p flusso-engine --bench engine -- --save-baseline "$baseline"
cargo bench -p flusso-source-postgres --bench pgoutput --features bench -- --save-baseline "$baseline"
cargo bench -p flusso-sink-opensearch --bench render --features bench -- --save-baseline "$baseline"
if [ -d libs/2-script ]; then
  cargo bench -p flusso-script --bench scripted -- --save-baseline "$baseline"
fi
