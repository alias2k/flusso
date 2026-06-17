# syntax=docker/dockerfile:1
#
# Registry-ready image for the `flusso` binary.
#
# The default (`runtime`) target is a generic, config-less image meant to be
# published to a container registry: it bakes in NO config and NO secrets. You
# supply a `flusso.toml` (+ schemas) or a compiled `flusso.lock` at run time —
# mount one in and pass `--config`, or bake your own lock into a child image —
# and connection/sink URLs come from the environment via `{ env = "VAR" }`
# secret references resolved where the pipeline runs.
#
#   docker build -t ghcr.io/OWNER/flusso:VERSION .
#
# The `demo` target extends the same runtime with the repo's dev config compiled
# into `/app/flusso.lock`; it is what docker-compose.demo.yml builds to run
# flusso in-cluster with no host toolchain (see that file).
#
#   docker build --target demo -t flusso:dev .

# ---- builder ----
FROM rust:1-bookworm AS builder
WORKDIR /usr/src/flusso

# The whole workspace (see .dockerignore for what's excluded). The pinned
# toolchain in rust-toolchain.toml is honored by rustup automatically.
COPY . .

# Build the release binary (default-members = apps/cli → the `flusso` binary).
# Cache the cargo registry and target dir across builds; the binary lives inside
# the cached target mount, so copy it out to a real path in the same RUN.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/flusso/target \
    cargo build --release --locked \
    && cp target/release/flusso /usr/local/bin/flusso

# ---- runtime (the published image) ----
FROM debian:bookworm-slim AS runtime

# ca-certificates for TLS to Postgres/OpenSearch; a non-root system user to run as.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 65532 flusso \
    && useradd --system --uid 65532 --gid flusso --no-create-home --shell /usr/sbin/nologin flusso

COPY --from=builder /usr/local/bin/flusso /usr/local/bin/flusso

# Apache-2.0 §4(a): ship the license text with the distributed binary.
COPY --from=builder /usr/src/flusso/LICENSE /usr/share/doc/flusso/LICENSE

WORKDIR /app
USER 65532:65532

# Operational HTTP surface (/healthz /readyz /status /metrics).
EXPOSE 9464

# Bare `flusso run` loads /app/flusso.lock when no flusso.toml is present (the
# config-less runtime image, or a child image with a baked lock). Pass `--config`
# to a mounted flusso.toml and it recompiles + rewrites the lock on start, like
# `cargo run`. Bind to all interfaces so a sidecar/Prometheus can reach the surface.
ENTRYPOINT ["flusso"]
CMD ["run", "--public-address", "0.0.0.0:9464"]

# OCI metadata — CI stamps the dynamic values: --build-arg VERSION=… REVISION=… CREATED=…
ARG VERSION=0.0.0-dev
ARG REVISION=unknown
ARG CREATED=unknown
LABEL org.opencontainers.image.title="flusso" \
      org.opencontainers.image.description="Keep OpenSearch in sync with Postgres from declarative config." \
      org.opencontainers.image.source="https://github.com/OWNER/flusso" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${CREATED}"

# ---- demo lock (compile the repo's dev config → flusso.lock; no DB, no secrets) ----
FROM builder AS demo-lock
RUN flusso build --config dev/flusso.toml --out /flusso.lock

# ---- demo (runtime + baked dev lock; built by docker-compose.demo.yml) ----
FROM runtime AS demo
COPY --from=demo-lock /flusso.lock /app/flusso.lock
