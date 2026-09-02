# Recover from a dropped slot

Understand what flusso does when its replication slot is gone, and use that behavior on purpose to force a full rebuild.

## When to use this

Someone dropped the slot, the database was restored from a snapshot or replaced, or WAL was truncated past it. Every change since the last seed is unobservable, so no seeded index can be trusted. flusso handles this on its own; this page says what to expect and how to make it happen deliberately.

## Steps

1. **Recognize it in the logs.** On start, flusso asks the source whether the slot exists. When it doesn't, it warns that the source is fresh and lists the seeded indexes it will rebuild.

2. **Let it run.** For each seeded index, a fresh generation is staged behind the same alias, so rows that no longer exist at the source are dropped on the swap rather than lingering. The old generation keeps serving. Then the slot is created, and only then does the backfill snapshot the root tables, so a write between the snapshot and the first live read is already covered by the slot.

3. **Watch `/status`.** Every index goes `backfilling`, then `seeded` as its rebuild lands and the alias flips. `phase` reaches `live` when all are done.

4. **If it crashes in between**, restart. The rebuilds were staged *before* the slot was created, so a crash there comes back to the same fresh decision and stages them again rather than trusting stale seeds.

## Options and variations

- **Force a full rebuild on purpose.** Stop flusso, drop the slot, start it again:

  ```sql
  SELECT pg_drop_replication_slot('flusso');
  ```

  This is the supported way to rebuild every index at once. To rebuild one index, use [Reindex without downtime](reindex.md) or delete its generation index.

- **`--skip-backfill` only warns.** With it set, a fresh slot doesn't rebuild anything; the indexes serve as they are. Use it knowingly.
- **Unseeded indexes aren't staged twice.** An index the sink already calls unseeded is about to be backfilled anyway; staging another generation for it would orphan one.
- **A missing generation, slot intact**, is the other contradiction: the seed marker names an index that isn't there. The sink retracts the marker and the backfill refills the same generation in place. No new generation.

## Related

- [How flusso works](../start/how-it-works.md#seeding-and-continuity) for why a seed is only as good as its stream.
- [Source: Postgres](../reference/source-postgres.md#capture) for slot and publication behavior.
