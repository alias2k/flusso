# Plugin architecture

How the flusso plugin is built, and the standard any change to it must meet. `plugin/README.md`
is the human/marketplace landing page; this file is the agent-facing contract.

**`writing-for-agents` governs `plugin/**`, not `docs/STYLE.md`.** The mdBook chapters and crate
READMEs follow `docs/STYLE.md` (prose for humans). Everything here is consumed by an agent, so the
standard is the `writing-for-agents` skill from the [aihero.dev/skills](https://www.aihero.dev/skills)
collection: information hierarchy, progressive disclosure, single source of truth, pointer wording.
Read it before editing a `SKILL.md`.

## One corpus, three consumers

The six `skills/*/SKILL.md` files are the only knowledge source. Three consumers read them:

| Consumer | How it reaches the corpus |
| --- | --- |
| The dev's ambient session | Skills auto-trigger on a matching question (model-invoked). |
| `/flusso:expert` | Invokes the relevant skill like any skill. |
| The `flusso-expert` agent | **Reads** `${CLAUDE_PLUGIN_ROOT}/skills/…` by path. Agents cannot invoke skills. |

That third row is the binding constraint on disclosure: **disclosed material must be a plain file
an agent can `Read`**, never a skill only a model can invoke. It is why the migration procedure
lives in `skills/flusso-query/migration.md` and not in a skill of its own.

## Self-containment

An external install has **no flusso checkout and no `mattpocock-skills`**. So nothing under
`plugin/` may point at `CLAUDE.md`, `docs/src/`, or `/grill-with-docs` as a load-bearing target. Where a
pointer is the right answer anyway (the repo owns the meaning), it carries one line saying the
target needs a checkout, and the skill still states what a reader without one can do.

Only `.claude/commands/` may reference the installed collection. The repo declares that dependency
in `.claude/settings.json`, so any checkout gets it.

## The information hierarchy

A `SKILL.md` holds what **every** invocation of that skill needs. Material only **some** branches
reach goes to a sibling file behind a sharp pointer.

- **Branching is the test.** Inline what every branch needs; disclose what one branch reaches.
- **Scatter is a co-location defect, not a disclosure one.** One meaning fragmented across four
  headings gets gathered under one heading. It does not get its own file, because a pointer that
  fails to fire on the common path produces silently wrong output.
- **Descriptions are permanent context load.** Every model-invoked `description` sits in context on
  every turn of every session, flusso-related or not. Front-load the leading word, keep one trigger
  per branch, cut identity the body already carries.

## Who owns which meaning

Duplication in this corpus has already produced a contradiction (see #116). Each meaning has one
home; everything else points at it.

| Meaning | Home |
| --- | --- |
| Read-side migrations (onto flusso; off the removed `FlussoDocument`/`path` form) | `skills/flusso-query/migration.md` |
| Per-type query options, closed param enums, compound/extra query types, `SortBuilder` verbs | `skills/flusso-query/options.md` |
| Dynamic-key `map` fields | `skills/flusso-query/maps.md` |
| Sorting rules (every kind, including ordered enums and map-key fallback) | `skills/flusso-query/SKILL.md`, one `## Sorting` section |
| `*.schema.yml` / `flusso.toml` authoring | `skills/flusso-schema/SKILL.md` |
| Postgres source behavior (slot, publication, row identity, privileges, TLS) | `skills/flusso-postgres/SKILL.md` |
| OpenSearch sink behavior (mapping ownership, generations, analyzers, subfields, refresh) | `skills/flusso-opensearch/SKILL.md` |
| Engine invariants, crate layering detail, lint policy, CI order, the designer alignment rule | the repo's `CLAUDE.md`. `skills/flusso-internals/SKILL.md` keeps only the seams and points here. |
| The plugin's own architecture and standard | this file |

A command or the agent restating any of the above is the defect. Point instead.

**One accepted duplication:** the four-bullet style block is inline in both `commands/expert.md` and
`agents/flusso-expert.md`. They are separate consumers with separate contexts, and a subagent should
not spend a file read on four bullets. Change both together.

## Drift guard

`apps/cli/tests/agent_docs_paths.rs` asserts that every repo path named in `plugin/**` and
`.claude/commands/**` exists. A pointer-heavy corpus rots mostly by moved files, so the cheap check
is the one that pays. It does not verify prose against behavior; that stays a review concern.
