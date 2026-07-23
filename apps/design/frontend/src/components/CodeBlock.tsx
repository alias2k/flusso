import { Check, Copy, RefreshCw, TextSelect } from "lucide-react";
import { Fragment, useRef, useState } from "react";
import { useT } from "../i18n";
import { jsonLine, yamlLine } from "./highlight";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/// Shared style for the small corner action buttons over a code block.
const CORNER =
  "grid size-7 cursor-pointer place-items-center rounded-md border border-border bg-secondary/80 text-muted-foreground backdrop-blur transition-colors hover:text-foreground disabled:opacity-50";

/// A highlighted code block reusing the `pre.yaml` surface, with copy (and an
/// optional refresh) pinned to its top-right corner. Lines are joined by
/// newlines (preserved by `white-space: pre`), so blank lines survive.
export function CodeBlock({
  text,
  lang,
  onRefresh,
  refreshing,
}: {
  text: string;
  lang: "yaml" | "json";
  onRefresh?: () => void;
  refreshing?: boolean;
}) {
  const { t } = useT();
  const [copied, setCopied] = useState(false);
  const preRef = useRef<HTMLPreElement>(null);
  const render = lang === "yaml" ? yamlLine : jsonLine;
  const selectAll = () => {
    const el = preRef.current;
    const sel = window.getSelection();
    if (!el || !sel) return;
    const range = document.createRange();
    range.selectNodeContents(el);
    sel.removeAllRanges();
    sel.addRange(range);
  };
  const copy = () =>
    navigator.clipboard?.writeText(text).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {
        /* ignore clipboard rejection */
      },
    );
  return (
    <div className="relative">
      <div className="absolute top-2 right-2 z-10 flex items-center gap-1.5">
        {onRefresh && (
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type="button"
                aria-label={t("preview.refresh")}
                onClick={onRefresh}
                disabled={refreshing}
                className={CORNER}
              >
                {refreshing ? <span className="spinner" /> : <RefreshCw className="size-3.5" />}
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">{t("preview.refresh")}</TooltipContent>
          </Tooltip>
        )}
        <Tooltip>
          <TooltipTrigger asChild>
            <button type="button" aria-label={t("preview.selectAll")} onClick={selectAll} className={CORNER}>
              <TextSelect className="size-3.5" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="top">{t("preview.selectAll")}</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <button type="button" aria-label={t("preview.copy")} onClick={() => void copy()} className={CORNER}>
              {copied ? <Check className="size-3.5 text-primary" /> : <Copy className="size-3.5" />}
            </button>
          </TooltipTrigger>
          <TooltipContent side="top">{copied ? t("preview.copied") : t("preview.copy")}</TooltipContent>
        </Tooltip>
      </div>
      <pre ref={preRef} className="yaml cursor-text">
        {text.split("\n").map((line, i) => (
          <Fragment key={i}>
            {render(line)}
            {"\n"}
          </Fragment>
        ))}
      </pre>
    </div>
  );
}
