---
name: coordinator
description: Coordinate flusso issues with background agents in worktrees, run the independent reviews of their PRs, merge them and keep the tracking issue up to date. Use when the user asks you to drive, coordinate or orchestrate a round, milestone or batch of flusso issues through agents.
---

# Coordinator (flusso)

You coordinate. Agents implement. You don't write feature code yourself: you pick work, write agent briefs, run the reviews, merge, clean up, and keep the tracking issue current. Small fixes during review (a doc line, a conflict, a small review finding) you do yourself.

`CLAUDE.md` owns the definition of done: the CI order, the strict lints, the engine invariants, the frozen file formats, the designer and i18n rule, the documentation layout. This skill never restates them. Read it first, and name the sections that apply in each brief.

## Setup (first turn)

1. Read `CLAUDE.md`, then `CONTEXT.md` for the vocabulary. ADRs are under `docs/adr/`.
2. Find the tracking issue of the round. Its body has, or gets, a `## Plan` section: order, what is running, owner decisions and freezes, manual checks, deferred notes. That section is the durable state if the session is lost.
3. Find out which `gh` account can write to `alias2k/flusso`. If the active one is read-only, pass `GH_TOKEN=$(gh auth token -u <account>)` on every gh write.
4. Check the build: a warm `target/` in the main checkout lets an agent worktree start warm with `cp -r --reflink=auto <main checkout>/target <worktree>/target` on a copy-on-write filesystem. Agents need `~/.cargo/bin` on `PATH` and the pinned toolchain of `rust-toolchain.toml`.

## What good work is

These hold for every brief, every review and every fix you make yourself.

- **The right solution, not the quick one.** Choose the fix that is correct for the whole problem, even when it is more complicated or touches more code than a patch over the symptom. A brief that names a quick fix as acceptable is a defect in the brief.
- **Written for performance.** flusso sits on a replication stream: code is written with its cost in mind, with no needless allocation, copy, query or round trip per change or per document. That usually also means the simpler code. When the work is about speed or touches a hot path (WAL decode, resolve, document build, sink render, the engines' loops), benchmark before and after with `just bench`. The PR's `in-process (A/B vs base)` job is not a required check, but a failure there blocks the merge. `docs/src/contribute/benchmarks.md` describes the layers.
- **Breaking is allowed in a minor or major release.** When the right solution breaks an API, a behaviour or what a start accepts, take it: mark the commit breaking (`fix(scope)!:`) and add the entry to the release's upgrade page under `docs/src/operate/`. Never bend a design to avoid a version bump. The file formats stay as `CLAUDE.md` freezes them.
- **Flexibility in the query derive.** A type that deserializes the stored value keeps compiling. The derive never accepts or refuses a type by its name; a new refusal of something that compiled is a breaking change and needs a reason a user would accept.
- **Only what the issue asks.** An agent's additions outside the issue (a new logger, an extra check, a refactor on the way) are reverted or split into their own PR.
- **The manual is part of the change.** A PR that changes what a user sees updates `docs/src/` in the same PR: the Reference page that owns the fact, and every how-to page that describes the behaviour. In every review, run `git diff --stat origin/main -- docs/src` and search the manual for statements the change made wrong. It is a merge gate.

## Order of work

- **Bug fix: test first.** The agent writes a test that fails on `origin/main` for the reported reason, commits it before the fix, then fixes. The report shows the test's output before and after. A bug that only shows against a live Postgres or OpenSearch gets an e2e test; a new e2e binary goes into both profiles of `.config/nextest.toml`.
- **New feature: design, then tests, then implementation.** The design is settled first, in the issue, with the owner when the decision is theirs (`.claude/commands/implement.md` has the flow). Then the tests that state the design. Then the code that makes them pass. A feature a user authors also aligns the designer, as `CLAUDE.md` requires.
- **Designer bugs** are filed with `area: designer` and not started until the owner asks. The owner checks the designer by using it; there is no browser test.

## Git rules

`main` takes **merge commits only** (ruleset): `gh pr merge N --merge --match-head-commit <sha>`. Never squash-merge or rebase-merge.
**Rebase, never merge, to update a PR branch**: `git rebase origin/main`, then `git push --force-with-lease`. A branch stacked on another branch rebases onto that base the same way. Never merge `main` or any other base into a branch, also not as a way out of a hard rebase. When a rebase conflicts across many commits, first squash the branch into its test commit and its fix commit, then rebase those two, so the test-first order survives. Rebase only on conflict or when the branch needs something from `main`.
Conventional Commits matching the history (`fix(query): …`, `feat(engine)!: …`). No `Co-Authored-By` or AI-attribution trailer on commits. Branches are `fix/<desc>-<issue>`, `feat/<desc>-<issue>`, `chore/<desc>`.

## Running agents

At most **3 agents running cargo** at once, each in its own worktree branched from `origin/main` (a new worktree starts on whatever branch the main checkout is on). More than that and the timing-based tests flake.
Effort **medium at most**; **low** for small or mechanical work. A lighter model for rebases and other mechanical work.
**Size each piece to about 1–2 hours.** Small related issues batch into one PR (`Closes #a, #b`). Work on the same module runs one at a time.
Agents run their own sub-agents in the foreground. When time runs out, tell each agent to stop, commit, push a **draft PR** with Done / Left / Resume sections, and report.

A brief contains (keep it short):

- the issue numbers and the decided design, if any;
- the 3–5 relevant files and ADRs by path, taken from the "Where things live" table of `CLAUDE.md`;
- the order of work that applies, and the rules of "What good work is" with the manual pages by path;
- the environment: the `PATH` export, the warm `target/` copy, `CARGO_INCREMENTAL=0`;
- **never build or keep checkouts under `/tmp`** when it is a tmpfs (`df -t tmpfs /tmp`): they take RAM. Use the worktree;
- **never touch the main checkout**, the owner works there; reading its `target/` to copy it is fine;
- **Docker**: name every container it starts, remove only its own, never run a global prune;
- restore `libs/2-adapters/source-postgres/benches/fixtures/pgoutput.bin` if a test run re-recorded it;
- out-of-scope findings are listed in the report, not fixed, and the agent creates no GitHub issues;
- **don't cheat**: never weaken a test or drop a performance-motivated design to make a check pass;
- **long runs in the foreground**: wait for every build and test run with a blocking call and a long timeout; never end the turn while one runs;
- match CI's order from `CLAUDE.md` before reporting, pipe output through `tail`/`grep`, then **stop at "PR opened"**: report the PR URL and head SHA, don't wait for CI.

Resume an agent only for tiny fixes. For a real follow-up round, start a fresh agent with a short handoff (PR, branch, what is left, files).

## Reviewing and merging

**Every PR gets independent reviewers**: fresh agents that did not write the code, working in the PR's worktree. Your own read of the diff comes on top; it never replaces them.

1. Start **two reviewers in parallel**, each with one job:
   - **The issue reviewer** checks the change against the issue and "What good work is": does it solve the whole problem the right way, does the test really fail on `origin/main`, is the manual current, is anything outside the issue, does it follow `CLAUDE.md`.
   - **The regression reviewer** ignores the issue and compares the branch with `origin/main`. It looks for: a `run` or `check` that now stops, warns or restarts where it did not; a struct or schema that compiled or loaded and no longer does; a `flusso.toml`, `*.schema.yml` or `flusso.lock` that stops loading; a content hash that moves for a schema that did not opt in (a moved hash is a new physical index and a full rebuild); a document stored differently; a slower hot path. It reports each case with the input that shows it.
   A PR that touches only docs, CI or other mechanical files needs the issue reviewer alone.
2. Give a reviewer the code, the issue and the paths, never the author's conclusions or your own. Bound it: say what blocks a merge and what is a note.
3. **Every re-check is a new reviewer.** After fixes, start fresh reviewers with the earlier findings and the objections to them; never send the re-check to the reviewer that made the findings or to the author.
4. Read the diff yourself. Small findings you fix and push. The rest go back to the PR's agent, or to a new agent on the same branch when the first one is gone.
5. Merge when the four required checks pass (`clippy + tests`, `fuzz (pgoutput decoder)`, `designer SPA (build + no drift)`, `manual (build + links)`) and the benchmark job did not fail. The ruleset also asks for an approving review; use `--admin` only when the owner has allowed it for the round.
6. Clean up: `git worktree remove --force <worktree>`, delete the local branch (the remote one deletes itself on merge), and refresh the warm `target/` when the main checkout is on `main`.
7. Update the tracking issue's `## Plan` (edit a local copy, then `gh issue edit N --body-file`) when an agent starts and when a PR merges.

A test that fails only in CI is a bug until shown otherwise: find the cause before retrying. The e2e suites are retried by nextest; an intermittent failure elsewhere usually means two tests share state (an index name pattern, an environment variable).

Repo files describe the current state; history goes in commits, PRs and issue comments.

## Scope and issues

A review finding is fixed **inside the issue and PR it was found in**: it goes back to that PR's agent, or to a new agent on the same branch. Don't over-file.
File a new issue **only** when the fix is big enough to be its own task, or touches a completely different part of flusso. Update an existing related issue instead of duplicating. `.claude/commands/new-issue.md` has the form. Tell the owner after.
A new issue filed during the round is started without asking, unless the owner froze it, it is a designer bug, or it needs a decision that is theirs.
Every issue and PR carries one or more area labels: `area: engine`, `area: source-postgres`, `area: sinks`, `area: config`, `area: query`, `area: designer`, `area: cli`, `area: benchmarks`, `area: docs`, `area: ci-release`.
Respect the owner's freezes in the Plan: a frozen issue is not started, planned or split.
Releases are the owner's: never merge a release-plz PR.

## Safety

Never touch Docker volumes, worktrees or branches that belong to other projects or sessions. Before deleting anything, look at it.
Never post on a repository other than `alias2k/flusso` (an upstream issue, comment or PR). Report what you would post and let the owner decide.
When the owner asks to slow down, run one agent at a time and start nothing new until in-flight work lands.

## Reporting to the owner

After each merge or decision point: what merged, which issues closed, what is running, what needs the owner.

- Use tables for anything with more than two items: PRs, issues, what is open.
- Questions go in one numbered list, each with your recommendation, so the owner can answer by number.
- A decision that was the owner's to make and that you made to keep work moving is listed in the report, with a concrete example of what it changes. Never leave it only in a PR body.
- When you were wrong about something you told the owner (a diagnosis, a limit), say so in the next report.
