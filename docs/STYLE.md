# Documentation style

How to write flusso's prose docs — the mdBook chapters under `docs/src/` and the
per-crate `README.md`s. This is about *tone and shape*, not what's true; for the
facts, the chapters and READMEs are canonical. (Code-comment tone — `///`/`//!` —
lives in `CLAUDE.md` under "Conventions"; this file is its prose counterpart.)

## TL;DR

- **State facts about the system**, not "you do X" or "we do X". Save direct "you" + imperative for real instructions.
- **Conversational, not stiff.** Contractions are fine. Lively, plain words. Dry wit, sparingly.
- **Write for a competent engineer.** Push newcomer help into callout boxes, not the body.
- **Terse and scannable.** Lead with the point. Short sentences. Bullets/tables for anything enumerable.
- **Every page opens with a one-line TL;DR** before the first heading.
- **`flusso` is always lowercase** — even at the start of a sentence.

## Voice — impersonal, about the system

Default to stating what flusso *does*. The system is the subject, not the reader and
not "we".

> ✅ flusso derives the index mapping from the schema and keeps it current via logical replication.
>
> ❌ You tell flusso about your document and we keep it in sync for you.

Reserve direct address (`you`, and imperative commands) for **genuine instructions** —
the steps a reader actually performs:

> Run `just check` to validate the config before you deploy.

No "we'll walk through…" framing. The docs aren't a guided tour; they're a description
of the system with instructions where the reader has to act.

## Register — conversational, never stiff

Plain and lively. Contractions are welcome. The voice is impersonal but not dry — a
vivid phrase about the *system* is good; chummy hand-holding of the *reader* is not.

> ✅ Generations are cheap: a reindex builds a fresh index beside the live one and flips an alias. No downtime, no holding your breath.
>
> ❌ The reindex operation is performed by constructing a new index generation in parallel with the extant index, followed by an atomic alias repointing procedure.

**Dry wit, sparingly.** An occasional understated aside earns its place. Never force a
joke, never use an emoji in body prose (callouts are the exception — see below).

## Audience — expert body, newcomer callouts

Write the body for someone fluent in Postgres, OpenSearch, and Docker. Explain
flusso's *specifics*; assume the ecosystem.

> Set `REPLICA IDENTITY FULL` on tables you soft-delete from, so the WAL carries the pre-image flusso needs to tombstone.

When a concept deserves a gentler explanation, **don't dilute the body** — put it in a
callout box (mdBook renders blockquotes; lead with an emoji + bold label):

> ℹ️ **Info** — `REPLICA IDENTITY` is a per-table Postgres setting controlling how much
> of a changed row lands in the replication stream. `FULL` includes the complete old
> row, which is what flusso compares against to detect a soft-delete.

> 💡 **Did you know** — flusso only reads the WAL; it never polls your tables, so
> follow-mode load on Postgres stays flat regardless of table size.

Use `ℹ️ **Info**` for prerequisite/background, `💡 **Did you know**` for the welcome
aside, and a plain `> ⚠️ **Warning**` for footguns.

## Density — terse and scannable

Lead with the conclusion. Keep sentences short. Anything enumerable — config keys,
flags, options, steps — is a **bullet list or table**, not a paragraph. Reserve prose
for *reasoning* the reader can't get from a list.

> **Soft-delete needs `REPLICA IDENTITY FULL`.**
>
> - **Why** — the WAL must carry the old row.
> - **Set it** — `ALTER TABLE x REPLICA IDENTITY FULL;`
> - **Skip it** — deletes silently won't tombstone.

If a paragraph runs past ~4 sentences, look for the list hiding inside it.

## Examples — prose-first, example confirms

Explain the concept, then show an example that *confirms* it. The example reinforces;
it doesn't lead.

> `check` validates the config and schemas against the database, then prints the
> mapping flusso would create:
>
> ```sh
> cargo run -- check --config dev/flusso.toml
> ```

Keep code blocks copy-pasteable and minimal. Comment a flag inline only when its
purpose isn't obvious from the command.

## Banned words

These get edited out:

- **Assumed-knowledge** — "obviously", "of course", "clearly", "naturally". They make a
  stuck reader feel stupid.
- **Filler** — "please", "note that" (just state the thing), "in order to" (→ "to").

**On minimizers ("just", "simply").** Not banned — but watch the *target*. Don't
minimize the **reader's action**: "simply run X", "just set the flag" → cut the
minimizer, keep the instruction. Minimizing the **system** is fine — "flusso just
follows the WAL" means "nothing more than", not "this is easy, so you're the problem".
Earned superlatives are fine too; the bar is: would it condescend, or is it padding?
If so, cut it.

## Terminology

- **`flusso`** — always lowercase, including sentence-initial. (`flusso keeps OpenSearch in sync…`)
- **`flusso.toml`**, **`*.schema.yml`**, **`flusso.lock`** — exact casing, in code font.
- Subcommands (`check`, `run`, `build`) and flags (`--config`) in code font.
- "the index mapping", "the WAL", "a generation", "a tombstone" — use the project's
  own nouns consistently; don't invent synonyms.
