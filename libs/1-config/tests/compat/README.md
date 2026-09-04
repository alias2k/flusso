# The frozen compatibility corpus

Every directory here is a snapshot of user-owned files — a `flusso.toml`, the
`*.schema.yml`s it references, and the `flusso.lock` they compiled to — frozen
at the release named by the directory. `tests/compat.rs` asserts each snapshot
still loads, converts, and decodes on the current code.

This is the enforcement half of the format guarantee (issue #109): **any file
valid at the start of the major keeps loading for the rest of it.** A change
that breaks a snapshot fails CI — that is the point. Fix the change, don't
edit the snapshot.

Rules:

- **Snapshots are immutable.** Never edit, regenerate, or delete a file in an
  existing `v*` directory. If a test here fails, the code broke compatibility.
- **Releases append.** When a release adds format surface (a new key, tag, or
  sink option), copy a config/schema set exercising it — plus the lock the
  release built from it — into a new `v<major.minor>/` directory.
- A snapshot's lock is the lock *that release* wrote. Don't re-bless it with a
  newer binary; byte drift in a rewritten lock is invisible here, and the
  golden fixture (`tests/golden_lock.rs`) already pins the current bytes.
