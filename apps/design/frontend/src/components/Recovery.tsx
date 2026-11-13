import { Button } from "@/components/ui/button";
import type { BlockedProblem, FileHolds, HandStep, JournalCause, JournalReport, SaveRecovery } from "../api";
import { useT, type Translate } from "../i18n";

const problemText = (t: Translate, problem: BlockedProblem): string => {
  switch (problem) {
    case "backup_missing":
      return t("recovery.problemBackupMissing");
    case "unreadable":
      return t("recovery.problemUnreadable");
    case "journal_unusable":
      return t("recovery.problemJournalUnusable");
    case "failed":
      return t("recovery.problemFailed");
  }
};

type BlockedFile = NonNullable<SaveRecovery["blocked"]>["problems"][number];

const causeText = (t: Translate, report: JournalReport): string => {
  switch (report.cause) {
    case "no_record":
      return t("recovery.causeNoRecord");
    case "other_folder":
      return t("recovery.causeOtherFolder", { folder: report.recorded_for ?? "" });
    case "no_state":
      return t("recovery.causeNoState");
    case "too_large":
      return t("recovery.causeTooLarge");
    case "unreadable":
      return t("recovery.causeUnreadable");
    case "malformed":
      return t("recovery.causeMalformed");
    case "not_a_save":
      return t("recovery.causeNotASave");
    case "conflict":
      return t("recovery.causeConflict");
  }
};

/// The page's heading for a journal the server does not act on: whether this
/// designer wrote it, has no record of it, or can't tell.
const reportTitle = (t: Translate, cause: JournalCause): string => {
  switch (cause) {
    case "other_folder":
    case "malformed":
    case "not_a_save":
      return t("recovery.blockedTitle");
    case "no_state":
      return t("recovery.uncheckedTitle");
    case "no_record":
    case "too_large":
    case "unreadable":
    case "conflict":
      return t("recovery.foreignTitle");
  }
};

const holdsText = (t: Translate, holds: FileHolds): string => {
  switch (holds) {
    case "before":
      return t("recovery.holdsBefore");
    case "after":
      return t("recovery.holdsAfter");
    case "neither":
      return t("recovery.holdsNeither");
    case "unchecked":
      return t("recovery.holdsUnchecked");
    case "outside":
      return t("recovery.holdsOutside");
  }
};

/// One step by hand, as the server lists it.
const stepText = (t: Translate, step: HandStep): string => {
  switch (step.do) {
    case "copy":
      return t("recovery.stepCopy", { from: step.from, path: step.to });
    case "delete":
      return t("recovery.stepDelete", { path: step.path });
    case "remove_dir":
      return t("recovery.stepRemoveDir", { path: step.path });
  }
};

/// Numbered steps by hand, then reload.
function Steps({ steps }: { steps: HandStep[] }) {
  const { t } = useT();
  return (
    <>
      <ol className="list-decimal space-y-1 pl-5 font-mono">
        {steps.map((step, i) => (
          <li key={i}>{stepText(t, step)}</li>
        ))}
      </ol>
      <p className="pl-5">{t("recovery.thenReload")}</p>
    </>
  );
}

/// The steps that take every file inside the project to one side of the save.
function Route({ steps, back }: { steps: HandStep[]; back: boolean }) {
  const { t } = useT();
  return (
    <div>
      <p>{back ? t("recovery.wayBack") : t("recovery.wayForward")}</p>
      <Steps steps={steps} />
    </div>
  );
}

/// What a journal the server does not act on names, what each file holds
/// now, and the way out by hand: read-only, nothing was changed.
function JournalFiles({ report }: { report: JournalReport }) {
  const { t } = useT();
  const journals = report.journals.join(", ");
  const possible = report.back_route !== undefined || report.forward_route !== undefined;
  return (
    <div className="mt-2 space-y-2">
      <p>{causeText(t, report)}</p>
      {report.files.length > 0 && (
        <>
          <p>{t("recovery.reportIntro")}</p>
          <ul className="list-disc space-y-1 pl-5">
            {report.files.map((file) => (
              <li key={file.path}>
                <code className="font-mono break-all">{file.path}</code>: {holdsText(t, file.holds)}
                {file.backup && (
                  <span className="block text-muted-foreground">
                    {t("recovery.reportBackup", { path: file.backup })}
                  </span>
                )}
                {file.temp && (
                  <span className="block text-muted-foreground">{t("recovery.reportTemp", { path: file.temp })}</span>
                )}
              </li>
            ))}
          </ul>
          {report.more ? <p>{t("recovery.reportMore", { count: String(report.more) })}</p> : null}
          {report.files.some((file) => file.outside) && <p>{t("recovery.outsideNote")}</p>}
        </>
      )}
      <h2 className="pt-2 font-semibold">{t("recovery.wayOutTitle")}</h2>
      <p>{t("recovery.wayLeave", { journals })}</p>
      {report.leftovers.length > 0 && (
        <>
          <p>{t("recovery.wayLeaveKept")}</p>
          <ul className="list-disc space-y-1 pl-5 font-mono">
            {report.leftovers.map((file) => (
              <li key={file}>{file}</li>
            ))}
          </ul>
        </>
      )}
      {possible ? (
        <>
          {report.cause !== "other_folder" && <p>{t("recovery.wayCaution")}</p>}
          {report.back_route && <Route steps={report.back_route} back />}
          {report.forward_route && <Route steps={report.forward_route} back={false} />}
        </>
      ) : (
        report.files.length > 0 && <p>{t("recovery.wayNone")}</p>
      )}
    </div>
  );
}

/// What to do about one file that stops recovery, going `back` (rolling the
/// save back) or forward (finishing it): only steps that lead somewhere true.
const problemHelp = (t: Translate, p: BlockedFile, back: boolean): string[] => {
  const path = p.path;
  switch (p.problem) {
    case "backup_missing": {
      const fix = p.fix;
      if (back) {
        if (fix?.do === "copy") return [t("recovery.helpBackCopy", { path, from: fix.from })];
        if (fix?.do === "delete") return [t("recovery.helpBackDelete", { path })];
        return [t("recovery.helpBackNoCopy", { path })];
      }
      const before = p.before_copy
        ? t("recovery.helpBeforeKept", { path, backup: p.before_copy })
        : t("recovery.helpBeforeGone", { path });
      if (fix?.do === "copy") return [t("recovery.helpForwardCopy", { path, from: fix.from }), before];
      if (fix?.do === "delete") return [t("recovery.helpForwardDelete", { path }), before];
      return [t("recovery.helpForwardNoCopy", { path }), before];
    }
    case "unreadable":
      return [t("recovery.helpUnreadable", { path })];
    case "journal_unusable":
      return [];
    case "failed":
      return [t("recovery.helpFailed")];
  }
};

/// The way to keep the files that changed as they are, and where it ends.
function Keep({ steps, files, back }: { steps: HandStep[]; files: string; back: boolean }) {
  const { t } = useT();
  return (
    <div className="space-y-1">
      <p>{back ? t("recovery.keepBack", { files }) : t("recovery.keepForward", { files })}</p>
      <Steps steps={steps} />
      <p>{back ? t("recovery.keepBackEnd", { files }) : t("recovery.keepForwardEnd", { files })}</p>
    </div>
  );
}

/// What the designer did about an interrupted save, shown once on load: each
/// save rolled back or completed with its files, and leftover files no journal
/// names.
export function RecoveryNotice({ recovery, onDismiss }: { recovery: SaveRecovery; onDismiss: () => void }) {
  const { t, lang } = useT();
  const displaced = [...(recovery.displaced ?? []), ...recovery.recovered.flatMap((s) => s.displaced ?? [])];
  if (recovery.recovered.length === 0 && recovery.leftovers.length === 0 && displaced.length === 0) return null;
  return (
    <div role="status" className="banner warn flex items-center gap-3 bg-warn/10 px-4 py-2 text-xs text-warn">
      <div className="flex-1 space-y-1">
        {recovery.recovered.map((save) => {
          const vars = {
            time: new Date(save.started_at_ms).toLocaleString(lang),
            files: save.files.join(", "),
            n: save.files.length,
          };
          return (
            <div key={save.id} className="space-y-0.5">
              {save.files.length > 0 && (
                <p>{save.outcome === "rolled_back" ? t("recovery.rolledBack", vars) : t("recovery.completed", vars)}</p>
              )}
              {save.kept?.map((k) => (
                <p key={k.path}>
                  {k.backup
                    ? t("recovery.keptBackup", { path: k.path, backup: k.backup })
                    : t("recovery.kept", { path: k.path })}
                </p>
              ))}
            </div>
          );
        })}
        {displaced.map((d) => (
          <p key={d.copy}>{t("recovery.displaced", { path: d.path, copy: d.copy })}</p>
        ))}
        {recovery.leftovers.length > 0 && <p>{t("recovery.leftovers", { files: recovery.leftovers.join(", ") })}</p>}
      </div>
      <Button variant="ghost" size="xs" onClick={onDismiss}>
        {t("recovery.dismiss")}
      </Button>
    </div>
  );
}

/// The page shown instead of the project when an interrupted save can't be
/// settled: which files stop it, the leftovers it keeps, and what to do.
export function RecoveryBlocked({ recovery }: { recovery: SaveRecovery }) {
  const { t } = useT();
  const blocked = recovery.blocked;
  if (!blocked) return null;
  const report = blocked.report;
  const back = blocked.direction !== "forward";
  const title = blocked.busy
    ? t("recovery.busyTitle")
    : report
      ? reportTitle(t, report.cause)
      : t("recovery.blockedTitle");
  return (
    <div role="alert" className="mx-auto max-w-3xl space-y-4 p-10 text-sm">
      <h1 className="text-lg font-semibold text-destructive">{title}</h1>
      {blocked.busy ? (
        <p>{t("recovery.busy")}</p>
      ) : report ? (
        <>
          <p>
            {t("recovery.reportFound", { journals: report.journals.join(", ") })} {t("recovery.nothingChanged")}
          </p>
          <JournalFiles report={report} />
        </>
      ) : (
        <>
          <p>{t("recovery.blockedIntro")}</p>
          {blocked.direction && <p>{back ? t("recovery.directionBack") : t("recovery.directionForward")}</p>}
          <ul className="list-disc space-y-2 pl-5">
            {blocked.problems.map((p) => (
              <li key={`${p.path}-${p.problem}`}>
                <code className="font-mono">{p.path}</code>: {problemText(t, p.problem)}
                {p.detail && <span className="text-muted-foreground"> ({p.detail})</span>}
                {problemHelp(t, p, back).map((help) => (
                  <p key={help} className="mt-1">
                    {help}
                  </p>
                ))}
              </li>
            ))}
          </ul>
          {blocked.problems.some((p) => p.problem === "backup_missing" && (back || p.fix)) && (
            <p>{back ? t("recovery.directionBackEnd") : t("recovery.directionForwardEnd")}</p>
          )}
          {blocked.keep && (
            <Keep steps={blocked.keep} files={blocked.problems.map((p) => p.path).join(", ")} back={back} />
          )}
          <p>{t("recovery.leftoverList")}</p>
          <ul className="list-disc space-y-1 pl-5 font-mono">
            {blocked.leftovers.map((file) => (
              <li key={file}>{file}</li>
            ))}
          </ul>
        </>
      )}
      <Button onClick={() => window.location.reload()}>{t("recovery.reload")}</Button>
    </div>
  );
}
