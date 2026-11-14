//! What the recovery notice and the blocked page say, as text: pure, so
//! `scripts/recovery.test.mjs` checks the wording each case picks without a
//! browser.
//!
//! ```ts
//! const lines = noticeLines(t, recovery, (ms) => new Date(ms).toLocaleString());
//! lines.map((line) => line.text); // one sentence or two per line
//! ```

import type { JournalReport, SaveRecovery } from "../api";
import type { Translate } from "../i18n";

/// One line of the notice, with a key that tells it from the others.
export interface NoticeLine {
  key: string;
  text: string;
}

/// Every line of the notice about earlier saves: what happened to which file,
/// and what is left to do. Each file is named once: a copy a line explains is
/// left out of the leftovers no save explains.
export const noticeLines = (t: Translate, recovery: SaveRecovery, time: (ms: number) => string): NoticeLine[] => {
  const lines: NoticeLine[] = [];
  const named = new Set<string>();
  for (const save of recovery.recovered) {
    const back = save.outcome === "rolled_back";
    const vars = { time: time(save.started_at_ms), files: save.files.join(", "), n: save.files.length };
    lines.push({
      key: save.id,
      text:
        save.files.length === 0
          ? t("recovery.interrupted", { ...vars, outcome: save.outcome })
          : back
            ? t("recovery.rolledBack", vars)
            : t("recovery.completed", vars),
    });
    for (const k of save.kept ?? []) {
      const kept = { path: k.path, backup: k.backup ?? "" };
      if (k.backup) named.add(k.backup);
      let text: string;
      if (!back) text = k.deleted ? t("recovery.keptDeletedAfter", kept) : t("recovery.keptAfter", kept);
      else if (k.deleted) text = k.backup ? t("recovery.keptDeletedBackup", kept) : t("recovery.keptDeleted", kept);
      else text = k.backup ? t("recovery.keptBackup", kept) : t("recovery.kept", kept);
      lines.push({ key: `${save.id}:${k.path}`, text });
    }
  }
  const displaced = [...(recovery.displaced ?? []), ...recovery.recovered.flatMap((s) => s.displaced ?? [])];
  for (const d of displaced) {
    named.add(d.copy);
    lines.push({ key: d.copy, text: t("recovery.displaced", { path: d.path, copy: d.copy }) });
  }
  for (const c of recovery.copies ?? []) {
    if (named.has(c.copy)) continue;
    named.add(c.copy);
    const vars = { copy: c.copy, path: c.path, time: time(c.started_at_ms) };
    lines.push({
      key: c.copy,
      text: c.holds === "before" ? t("recovery.copyBefore", vars) : t("recovery.copyDisplaced", vars),
    });
  }
  for (const copy of recovery.leftovers.filter((file) => !named.has(file))) {
    const path = copyOwner(copy);
    lines.push({
      key: copy,
      text: path ? t("recovery.leftover", { copy, path }) : t("recovery.leftoverUnknown", { copy }),
    });
  }
  return lines;
};

/// The file a save's copy belongs to: `.<name>.flusso-save.<…>` beside it is
/// a copy of `<name>`. `undefined` for a name of another form.
export const copyOwner = (copy: string): string | undefined => {
  const slash = copy.lastIndexOf("/");
  const dir = copy.slice(0, slash + 1);
  const name = copy.slice(slash + 1);
  const at = name.indexOf(".flusso-save.");
  if (!name.startsWith(".") || at <= 1) return undefined;
  return dir + name.slice(1, at);
};

/// The lead-in of each way out of a journal the server does not act on: the
/// way forward offers itself as the other choice only after a way back.
export const routeLeads = (t: Translate, report: JournalReport): { back?: string; forward?: string } => ({
  back: report.back_route ? t("recovery.wayBack") : undefined,
  forward: report.forward_route
    ? report.back_route
      ? t("recovery.wayForward")
      : t("recovery.wayForwardOnly")
    : undefined,
});
