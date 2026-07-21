import { useState } from "react";
import { ChevronDown, Database, Play, RefreshCw } from "lucide-react";
import type { DiagnosticDto, DocumentNode, PreviewResponse, SampleResponse } from "../api";
import { useT } from "../i18n";
import { typeClass } from "../theme";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { CodeBlock } from "./CodeBlock";

type Tab = "document" | "yaml" | "mapping" | "sample";

/// A dotted leader that fills the gap between a field name and its type, so the
/// two read together across a wide panel.
const Leader = () => <span className="h-0 flex-1 self-center border-b border-dotted border-border/60" />;

/// One document field: a leaf renders a coloured type chip; a container (object /
/// nested) renders a group header (chevron + muted type tag) with its children
/// indented under a nesting guide line.
function Node({ node }: { node: DocumentNode }) {
  const suffix = `${node.array ? "[]" : ""}${node.nullable ? "?" : ""}`;
  if (node.children) {
    return (
      <div>
        <div className="flex items-center gap-2 rounded-md px-2 py-1 hover:bg-accent">
          <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
          <span className="font-medium whitespace-nowrap text-foreground">{node.name}</span>
          <Leader />
          <span className="shrink-0 rounded border border-border bg-secondary px-1.5 py-0.5 font-mono text-2xs text-muted-foreground">
            {node.type}
            {suffix}
          </span>
        </div>
        <div className="ml-2.5 border-l border-border/50 pl-2.5">
          {node.children.map((c, i) => (
            <Node key={i} node={c} />
          ))}
        </div>
      </div>
    );
  }
  return (
    <div className="flex items-center gap-3 rounded-md px-2 py-1 hover:bg-accent">
      <span className="whitespace-nowrap text-foreground">{node.name}</span>
      <Leader />
      <span
        className={cn(
          "shrink-0 rounded border border-current/30 px-1.5 py-0.5 font-mono text-2xs",
          typeClass(node.type),
        )}
      >
        {node.type}
        {suffix}
      </span>
    </div>
  );
}

/// The schema preview: what the current index compiles to, split into tabs —
/// the shaped Document tree, the `*.schema.yml` output, the OpenSearch mapping,
/// and a Sample document built from a live row. Fills its container (the drawer).
export function Preview({
  preview,
  diagnostics,
  onSample,
}: {
  preview: PreviewResponse | null;
  diagnostics: DiagnosticDto[] | null;
  onSample?: () => Promise<SampleResponse>;
}) {
  const { t } = useT();
  const [tab, setTab] = useState<Tab>("document");
  if (!preview) return <div className="p-4 text-sm text-muted-foreground">{t("preview.empty")}</div>;

  const diagCount = diagnostics?.length ?? 0;
  const tabs: { id: Tab; label: string; badge?: number }[] = [
    { id: "document", label: t("preview.document"), badge: diagCount || undefined },
    { id: "yaml", label: t("preview.tabYaml") },
    { id: "mapping", label: t("preview.tabMapping") },
    ...(onSample ? [{ id: "sample" as Tab, label: t("preview.tabSample") }] : []),
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {!preview.parse_ok && (
        <div className="banner error mx-3 mt-3">
          <strong>{t("preview.parseError")}</strong> {preview.parse_error}
        </div>
      )}

      <div className="flex gap-1 border-b border-border px-3 pt-2">
        {tabs.map((tb) => (
          <button
            key={tb.id}
            type="button"
            onClick={() => setTab(tb.id)}
            className={cn(
              "-mb-px flex cursor-pointer items-center gap-1.5 rounded-sm border-b-2 px-2.5 py-1.5 text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
              tab === tb.id
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {tb.label}
            {tb.badge ? (
              <span className="rounded-full bg-warn/20 px-1.5 text-3xs font-semibold text-warn tabular-nums">
                {tb.badge}
              </span>
            ) : null}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-3">
        {tab === "document" && (
          <>
            {diagCount > 0 && (
              <div className="mb-3 rounded-md border border-warn/30 bg-warn/5 p-2">
                {diagnostics?.map((d, i) => (
                  <div
                    key={i}
                    className={cn(
                      "py-0.5 text-sm",
                      d.severity === "error" ? "text-destructive" : d.severity === "warning" ? "text-warn" : "",
                    )}
                  >
                    <span className="font-mono text-muted-foreground">
                      {d.index}.{d.field}
                    </span>{" "}
                    {d.message}
                  </div>
                ))}
              </div>
            )}
            <div className="text-sm">
              {preview.preview.document.map((n, i) => (
                <Node key={i} node={n} />
              ))}
            </div>
          </>
        )}

        {tab === "yaml" && <CodeBlock text={preview.yaml} lang="yaml" />}

        {tab === "mapping" && <CodeBlock text={JSON.stringify(preview.preview.mapping, null, 2)} lang="json" />}

        {tab === "sample" && onSample && <SampleDoc onSample={onSample} />}
      </div>
    </div>
  );
}

/// Fetches a real document built from one live row — exactly what the sink would
/// write — on demand, so you can sanity-check the schema against actual data.
function SampleDoc({ onSample }: { onSample: () => Promise<SampleResponse> }) {
  const { t } = useT();
  const [result, setResult] = useState<SampleResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchSample = () => {
    setLoading(true);
    setError(null);
    onSample()
      .then(setResult)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  };

  const doc = result?.document;
  const errorText = error ?? (result && !result.db_reachable ? result.error : null);

  // Loaded: the built document, with a refresh + the example marker.
  if (doc !== undefined && doc !== null && result?.db_reachable !== false) {
    return (
      <div>
        <div className="mb-2 flex items-center gap-2">
          {result?.synthetic && <span className="badge object">{t("preview.example")}</span>}
          {result?.note && <span className="text-2xs text-muted-foreground">{result.note}</span>}
          <Button variant="secondary" size="sm" className="ml-auto gap-1.5" onClick={fetchSample} disabled={loading}>
            {loading ? <span className="spinner" /> : <RefreshCw className="size-3.5" />}
            {t("preview.refresh")}
          </Button>
        </div>
        <CodeBlock text={JSON.stringify(doc, null, 2)} lang="json" />
      </div>
    );
  }

  // Empty / error: a centred prompt with the primary "build" action.
  return (
    <div className="flex flex-col items-center gap-3 py-12 text-center">
      <span className="grid size-12 place-items-center rounded-full border border-border bg-secondary text-accent2">
        <Database className="size-5" />
      </span>
      <div>
        <p className="text-sm font-medium text-foreground">{t("preview.sampleTitle")}</p>
        <p className="mx-auto mt-1 max-w-xs text-xs text-muted-foreground">{t("preview.sampleHint")}</p>
      </div>
      {errorText && <p className="max-w-sm text-xs text-destructive">{errorText}</p>}
      <Button size="sm" className="gap-1.5" onClick={fetchSample} disabled={loading}>
        {loading ? <span className="spinner" /> : <Play className="size-3.5" />}
        {loading ? t("preview.building") : t("preview.fetch")}
      </Button>
    </div>
  );
}
