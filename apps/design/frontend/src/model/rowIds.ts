// Stable client ids for the rows of a map edited in place, such as a field's
// `options`: a row is keyed by its id, never by its key, so typing in the key
// box renames the key without remounting the row (which would drop focus and
// the value box's state). A rename moves the id to the new key; a key seen for
// the first time (added, or brought back by undo or Code mode) gets a new one.
//
// ```ts
// const rows = new RowIds();
// const id = rows.id("boost");
// rows.rename("boost", "boost2");
// rows.id("boost2") === id; // true
// rows.key(id); // "boost2"
// ```

export class RowIds {
  private readonly ids = new Map<string, string>();

  /// The id of the row keyed `key` now, made the first time the key is seen.
  id(key: string): string {
    let id = this.ids.get(key);
    if (id === undefined) {
      id = crypto.randomUUID();
      this.ids.set(key, id);
    }
    return id;
  }

  /// The row keyed `from` is keyed `to` from now on.
  rename(from: string, to: string): void {
    const id = this.id(from);
    this.ids.delete(from);
    this.ids.set(to, id);
  }

  /// The key the row `id` has now; `undefined` once the row was removed.
  key(id: string): string | undefined {
    for (const [key, of] of this.ids) if (of === id) return key;
    return undefined;
  }

  /// Forget every row whose key is not in `keys` (it was removed).
  keep(keys: Iterable<string>): void {
    const live = new Set(keys);
    for (const key of [...this.ids.keys()]) if (!live.has(key)) this.ids.delete(key);
  }
}
