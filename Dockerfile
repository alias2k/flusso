# syntax=docker/dockerfile:1
#
# Builds the `flusso` binary and bakes the dev config + schemas into a portable
# `flusso.lock`. Used by docker-compose.demo.yml to run flusso in-cluster.
# Connection and sink URLs are NOT baked in — they come from the environment at
# run time (DATABASE_URL, PRIMARY_OPENSEARCH_URL), set by compose.

# ---- builder ----
FROM rust:1-bookworm AS builder
WORKDIR /usr/src/flusso

# The whole workspace (see .dockerignore for what's excluded). The pinned
# toolchain in rust-toolchain.toml is honored by rustup automatically.
COPY . .

# Build the release binary (default-members = apps/cli → the `flusso` binary),
# then compile the dev config + schemas into a portable artifact (no DB needed).
RUN cargo build --release --locked
RUN ./target/release/flusso build --config dev/flusso.toml --out flusso.lock

# ---- runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /usr/src/flusso/target/release/flusso /usr/local/bin/flusso
COPY --from=builder /usr/src/flusso/flusso.lock /app/flusso.lock

# `flusso run` loads /app/flusso.lock by default and serves status/metrics on all
# interfaces so Prometheus (and the host) can reach it.
EXPOSE 9464
ENTRYPOINT ["flusso"]
CMD ["run", "--http-addr", "0.0.0.0:9464"]
