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
/// and what is left to do.
export const noticeLines = (t: Translate, recovery: SaveRecovery, time: (ms: number) => string): NoticeLine[] => {
  const lines: NoticeLine[] = [];
  for (const save of recovery.recovered) {
    const vars = { time: time(save.started_at_ms), files: save.files.join(", "), n: save.files.length };
    if (save.files.length > 0) {
      lines.push({
        key: save.id,
        text: save.outcome === "rolled_back" ? t("recovery.rolledBack", vars) : t("recovery.completed", vars),
      });
    }
    for (const k of save.kept ?? []) {
      lines.push({
        key: `${save.id}:${k.path}`,
        text: k.backup
          ? t("recovery.keptBackup", { path: k.path, backup: k.backup })
          : t("recovery.kept", { path: k.path }),
      });
    }
  }
  const displaced = [...(recovery.displaced ?? []), ...recovery.recovered.flatMap((s) => s.displaced ?? [])];
  for (const d of displaced) {
    lines.push({ key: d.copy, text: t("recovery.displaced", { path: d.path, copy: d.copy }) });
  }
  if (recovery.leftovers.length > 0) {
    lines.push({ key: "leftovers", text: t("recovery.leftovers", { files: recovery.leftovers.join(", ") }) });
  }
  return lines;
};

/// The lead-in of each way out of a journal the server does not act on.
export const routeLeads = (t: Translate, report: JournalReport): { back?: string; forward?: string } => ({
  back: report.back_route ? t("recovery.wayBack") : undefined,
  forward: report.forward_route ? t("recovery.wayForward") : undefined,
});
