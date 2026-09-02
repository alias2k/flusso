# Secure the private surface

Change the control surface's default credentials, keep it off the network, and reach it safely when you need it.

## When to use this

Before the first deployment anywhere other than a laptop. The private surface (`/indexes`, `/reindex`) ships with `admin` / `flusso` so it works out of the box, and flusso logs a warning on every start while the password is unchanged.

## Steps

1. **Set the credentials from the environment.** They are flags and env vars, never config keys, so a committed `flusso.toml` or `flusso.lock` can't leak them.

   ```sh
   FLUSSO_ADMIN_USER=ops FLUSSO_ADMIN_PASSWORD="$(openssl rand -base64 24)" flusso run …
   ```

   In Kubernetes, put both in the Secret the chart mounts with `envFrom`.

2. **Leave the bind address on localhost.** The default `127.0.0.1:9465` is correct for production. The public surface is the one that needs `0.0.0.0` for scrapes and probes; the private one doesn't.

3. **Reach it through a tunnel.** From the host, the client subcommands default to `127.0.0.1:9465`. From outside, forward the port:

   ```sh
   kubectl -n flusso port-forward deploy/flusso 9465:9465
   flusso indexes --admin-user ops --admin-password "$FLUSSO_ADMIN_PASSWORD"
   ```

4. **Confirm the warning is gone.** Startup logs no longer mention the default password.

## Options and variations

- **Expose it deliberately** only behind something that adds TLS and its own authentication (an ingress with client certificates, a service mesh). The surface itself speaks plain HTTP with Basic auth.
- **The public surface is read-only** and unauthenticated by design: `/status` reveals counters and the last error string, nothing more. Gate it by network if that's sensitive.
- **Credential checks are constant-time** on both user and password.

## Related

- [HTTP endpoints](../reference/http.md) for what each surface serves.
- [Deploy with Helm](../deploy/helm.md#options-and-variations) for `http.privatePort`.
