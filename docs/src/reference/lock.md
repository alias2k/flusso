# flusso.lock

The compiled deployment: one deterministic TOML file holding the validated `flusso.toml` and every schema it references, with secrets kept as references.

| Property | Value |
| --- | --- |
| Written by | `flusso build`, and `flusso run` whenever a config is present and the bytes changed |
| Read by | `flusso run` with no config, or with `--locked`; `flusso check` and `flusso show` with no config |
| Checked by | `flusso check`, against what the config compiles to |
| Format | TOML, `format_version = 3`, a fixed two-line comment header |
| Determinism | Same inputs, identical bytes. Nothing in it depends on the flusso version that wrote it. |
| Secrets | `{ env = "VAR" }` references travel as references. A literal in the config travels as a literal. |
| Hand edits | Not a supported input. Like `Cargo.lock`, flusso owns and rewrites it. |

## Why it exists

A deployment ships one file instead of a source tree. The lock needs no database and no toolchain to produce, so it can be built in CI from a monorepo where schemas are scattered, then copied into an image or mounted into a container. Because it's plain TOML, a committed lock diffs meaningfully in review. See [Ship with Docker](../deploy/docker.md).

## Lifecycle with run

| Invocation | Behavior |
| --- | --- |
| `flusso run` with `flusso.toml` present | Recompile, rewrite the lock if changed, run it. The committed lock stays current. |
| `flusso run` with no config | Load `flusso.lock` and run it. |
| `flusso run --locked` | Run the lock as is. No recompile, no rewrite. A lock whose line endings git converted holds other script text, so an index with a script gets another content hash: a new physical index, rebuilt from scratch. Keep the lock `-text` in `.gitattributes`. |
| `flusso run --config other.toml` | Recompile from that file; an explicit path that doesn't exist is an error. |

A lock-write failure is fatal. A lock in an earlier format (the pre-0.16 `format_version = 2`, or the MessagePack binary before that) is rejected with a regenerate hint: run `flusso build` once and commit the result. `flusso run` without `--locked` regenerates it on its own.

## Checked by flusso check

`flusso check` compiles the config exactly as `run` does before rewriting the lock, and compares the result with the file at `--lock` (`FLUSSO_LOCK`, default `flusso.lock` in the working directory). It writes nothing, and it runs with `--offline` too. A stale lock is the one `run --locked`, or an image baked from it, would run instead of the config under review.

| The lock | Report line | Error | `lock.state` in JSON |
| --- | --- | --- | --- |
| holds exactly what the config compiles to | `✓ flusso.lock is current` | | `current` |
| holds the same config in other bytes (Windows line endings, hand formatting) | `` ! flusso.lock holds this config in other bytes than flusso writes (line endings or formatting); `flusso run` would rewrite it `` | | `same_config` |
| isn't there | `! flusso.lock not found; flusso build writes it` | | `missing` |
| holds another config | `` `flusso.lock` was compiled from another config than this one (the index `users` changed): … `` | yes | `lock_stale` |
| can't be read, or is of an older format | `` `flusso.lock` can't be read as a lock, so `flusso run --locked` would fail on it: … `` | yes | `lock_unreadable` |
| can't be read, and there's no config | `check` stops before the report: `no flusso.toml to check, and the lock at flusso.lock can't be read`, then the reason | yes | none: no report |
| was checked alone: no config | `! no config to compare flusso.lock with: the lock is what was checked` | | `not_compared` |

- **A stale lock** names what differs at the top level: which indexes changed, are new or are gone, and which deployment settings (`sinks`, `source`, …).
- **An unreadable lock** names the line, the column and the reason, never the text there, since a secret may sit on that line: ``failed to decode compiled config at line 21, column 24: unexpected key or value, expected newline, `#` ``, or `it is format 2, and this flusso reads format 3`.
- **Line endings inside a string count.** A script's source is a multi-line string in the lock, so a lock whose line endings were converted (a Windows checkout with `core.autocrlf`) holds other scripts and reads as stale. The message names the cause and the fix:

  ```text
  [error] flusso.lock  `flusso.lock` was compiled from another config than this one (the index `products` changed): `flusso run --locked`, or an image baked from this lock, would run the older config. Run `flusso check --offline --fix` or `flusso build` (or `flusso run`, which rewrites it) and commit the result. `flusso.lock` has Windows line endings (CRLF), which change multi-line values such as script sources. Rewrite it as above, and mark it `-text` in `.gitattributes` (`flusso.lock -text`) so git does not convert it again
  ```

  A converted lock with no multi-line value in it holds the same config and is only a note (`same_config`).
- **A lock that can't be read, with no config,** stops every command before it starts, naming the lock and the reason, never its text:

  ```text
  $ flusso check --offline
  Error: no flusso.toml to check, and the lock at flusso.lock can't be read

  Caused by:
      failed to decode compiled config at line 1, column 6: key with no value, expected `=`
  ```

  `flusso show` says `no flusso.toml, and the lock at flusso.lock can't be read`, `flusso run` `no flusso.toml to compile, and the lock at flusso.lock can't be read; create a config or build a lock first`. With no lock at all, the headline is `no flusso.toml to check and no compiled lock at flusso.lock` (and the same for the others).
- **The advice runs as printed.** It names `--config`, `--offline` and the lock's path when they aren't the defaults: `` Run `flusso check --offline --fix` or `flusso build` (or `flusso run`, which rewrites it) and commit the result ``.

**`--fix`** rewrites a lock that is stale, unreadable, written differently or missing, with exactly what `flusso build` writes (a temporary file beside it, renamed over it), once the config has validated. The report says `✓ lock rewritten: flusso.lock`, and JSON keeps the state found before the fix with `"rewritten": true`. It fixes the lock only: every other finding is reported and decides the exit code as without it. With no config there's nothing to write from, and it says `no config to write flusso.lock from: --fix wrote nothing`. A write that fails is an error naming the path; a failure after the write (an unreachable database) says the lock was rewritten. No environment variable turns it on, so a CI job can't rewrite a lock through an inherited variable.

## Format stability

The files a deployment owns are covered by a compatibility guarantee for the rest of the current major:

- **Backwards, always.** Any `flusso.toml` or `*.schema.yml` a release in this major accepts keeps loading on every later release in it, and so does any `flusso.lock` written from 0.16 on (the lock was re-versioned once, to format 3, when adapters took ownership of their options). CI enforces this against a frozen corpus of earlier-release files.
- **Deprecate, don't remove.** A key can be superseded and warned about; it keeps working for the major.
- **Forward is not guaranteed.** A file using newer keys may fail on an older binary. Unknown keys are a loud error, never a silent no-op.

## Example

```toml
# This file is generated by flusso from `flusso.toml` and the schemas it references.
# It is not intended for manual editing.
format_version = 3

[config.source]
type = "postgres"

[config.source.connection_url]
env = "PG_URL"
# …every sink and every index's full schema follows
```
