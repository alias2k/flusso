import { useState } from "react";
import type { DiagnosticDto, DocumentNode, PreviewResponse, SampleResponse } from "../api";
import { useT } from "../i18n";
import { typeClass } from "../theme";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Icon } from "./Icon";

type Tab = "document" | "yaml" | "mapping" | "sample";

function Node({ node, depth }: { node: DocumentNode; depth: number }) {
  return (
    <div>
      <div className="flex justify-between py-0.5" style={{ paddingLeft: depth * 16 }}>
        <span className="text-foreground">{node.name}</span>
        <span className={cn("font-mono text-xs", typeClass(node.type))}>
          {node.type}
          {node.array ? "[]" : ""}
          {node.nullable ? "?" : ""}
        </span>
      </div>
      {node.children?.map((c, i) => (
        <Node key={i} node={c} depth={depth + 1} />
      ))}
    </div>
  );
}

/// Copy-to-clipboard button that flips to a "copied" label for a moment.
function CopyButton({ text }: { text: string }) {
  const { t } = useT();
  const [copied, setCopied] = useState(false);
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
    <Button variant="secondary" size="sm" className="gap-1.5" onClick={() => void copy()}>
      <Icon name="copy" size={13} /> {copied ? t("preview.copied") : t("preview.copy")}
    </Button>
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
              "-mb-px flex cursor-pointer items-center gap-1.5 border-b-2 px-2.5 py-1.5 text-xs transition-colors",
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
            <div className="rounded-md border border-border bg-secondary p-2">
              {preview.preview.document.map((n, i) => (
                <Node key={i} node={n} depth={0} />
              ))}
            </div>
          </>
        )}

        {tab === "yaml" && (
          <>
            <div className="mb-2 flex justify-end">
              <CopyButton text={preview.yaml} />
            </div>
            <pre className="yaml">{preview.yaml}</pre>
          </>
        )}

        {tab === "mapping" && (
          <>
            <div className="mb-2 flex justify-end">
              <CopyButton text={JSON.stringify(preview.preview.mapping, null, 2)} />
            </div>
            <pre className="yaml">{JSON.stringify(preview.preview.mapping, null, 2)}</pre>
          </>
        )}

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

  return (
    <div>
      <div className="mb-2 flex items-center gap-2">
        {result?.synthetic && <span className="badge object">{t("preview.example")}</span>}
        <Button variant="secondary" size="sm" className="ml-auto gap-1.5" onClick={fetchSample} disabled={loading}>
          <Icon name="play" size={13} />{" "}
          {loading ? t("preview.building") : result ? t("preview.refresh") : t("preview.fetch")}
        </Button>
      </div>
      {error && <div className="banner error">{error}</div>}
      {result && !result.db_reachable && <div className="banner error">{result.error}</div>}
      {result?.note && <p className="hint">{result.note}</p>}
      {result?.document !== undefined && result.document !== null ? (
        <pre className="yaml">{JSON.stringify(result.document, null, 2)}</pre>
      ) : (
        !loading && !error && <p className="text-sm text-muted-foreground">{t("preview.sampleHint")}</p>
      )}
    </div>
  );
}
