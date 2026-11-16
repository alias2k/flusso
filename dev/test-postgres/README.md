# `flusso-test-postgres` — the Postgres image tests and benches start

One place names the Postgres image every flusso test and bench starts (issue #342). Unpublished; a dev-dependency only.

- **Default**: `postgres:16-alpine`.
- **Tested**: `TESTED_TAGS`, the majors CI runs; which ones and why: [Testing → Other majors](https://alias2k.github.io/flusso/main/contribute/testing.html#other-majors). `cargo run -q -p flusso-test-postgres -- matrix` prints the tags other than the default as a JSON array, one `postgres-matrix` leg each.
- **Override**: `FLUSSO_TEST_PG_TAG` names another tag of the official image (`14-alpine`, `18-alpine`) for the whole run, with no code change. Unset or empty means the default.
- **Exceptions**: a test that needs one version whatever the run's tag names it with a `Pinned` variant, which carries the reason. `pinned()` prints one line to stderr naming the image it starts instead of the run's, and why.
- **The container type**: `test_postgres::Container`. Only this crate imports `testcontainers_modules::postgres`.

```rust
use testcontainers_modules::testcontainers::ImageExt;

// The run's image, with a server setting on top.
let logical = test_postgres::image().with_cmd(["postgres", "-c", "wal_level=logical"]);
// A test that needs Postgres 18 whatever the run's tag.
let pinned = test_postgres::pinned(test_postgres::Pinned::ByteaMinMax);
# let _ = (logical, pinned);
assert_eq!(test_postgres::Pinned::ByteaMinMax.tag(), "18-alpine");
```

`cargo run -q -p flusso-test-postgres` prints the image the run resolves (`postgres:16-alpine`), which the benchmark job records with the component points so a tag change restarts their regression window.

`testcontainers-modules`' own `Postgres::default()` starts `postgres:11-alpine`, a version flusso does not support. `.github/scripts/check-postgres-image.py` (run by CI's `scripts` job) fails on any import of `testcontainers_modules::postgres` outside this crate (nested, glob and renamed imports included), on `Postgres::default`, `with_tag`/`with_name`, and on a `GenericImage` whose arguments mention `postgres` or whose image name isn't a literal.

## Why a crate of its own

The starts live in four published crates (`flusso-source-postgres`, `flusso-engine`, `flusso-daemon`, `flusso-cli`) and the unpublished `flusso-bench`. No published crate can hold a test helper for the others without a dependency the layering forbids, and a copy per crate would be four homes. A `publish = false` crate taken as a **path-only dev-dependency** is invisible to a published manifest (`cargo publish` drops it), depends on no flusso crate, and so sits outside the numbered layers like the rest of `dev/`.
