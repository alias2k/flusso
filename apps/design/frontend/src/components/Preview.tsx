import { useState } from "react";
import type { DiagnosticDto, DocumentNode, PreviewResponse, SampleResponse } from "../api";
import { useT } from "../i18n";
import { typeClass } from "../theme";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Icon } from "./Icon";

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
  const [copied, setCopied] = useState(false);
  if (!preview) return <div className="text-muted-foreground">{t("preview.empty")}</div>;
  const copy = () => {
    navigator.clipboard?.writeText(preview.yaml).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {
        /* ignore clipboard rejection */
      },
    );
  };
  return (
    <div>
      {!preview.parse_ok && (
        <div className="banner error">
          <strong>{t("preview.parseError")}</strong> {preview.parse_error}
        </div>
      )}
      {diagnostics && diagnostics.length > 0 && (
        <div className="mb-3">
          <h3>{t("preview.dbCheck")}</h3>
          {diagnostics.map((d, i) => (
            <div key={i} className={cn("py-1 text-sm", d.severity === "error" ? "text-destructive" : d.severity === "warning" ? "text-warn" : "")}>
              <span className="font-mono text-muted-foreground">
                {d.index}.{d.field}
              </span>{" "}
              {d.message}
            </div>
          ))}
        </div>
      )}
      <h3>{t("preview.document")}</h3>
      <div className="rounded-md border border-border bg-secondary p-2">
        {preview.preview.document.map((n, i) => (
          <Node key={i} node={n} depth={0} />
        ))}
      </div>
      {onSample && preview.parse_ok && <SampleDoc onSample={onSample} />}
      <h3 className="flex items-center justify-between">
        {t("preview.schemaYml")}
        <Button variant="link" size="sm" className="ml-auto gap-1" onClick={copy} title={t("preview.copyYaml")}>
          <Icon name="copy" size={13} /> {copied ? t("preview.copied") : t("preview.copy")}
        </Button>
      </h3>
      <pre className="yaml">{preview.yaml}</pre>
      <details className="mapping-details">
        <summary>{t("preview.osMapping")}</summary>
        <pre className="yaml">{JSON.stringify(preview.preview.mapping, null, 2)}</pre>
      </details>
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
    <div className="sample-doc mt-2">
      <h3 className="flex items-center gap-2">
        {t("preview.sampleFromDb")}
        {result?.synthetic && <span className="badge object">{t("preview.example")}</span>}
        <Button variant="link" size="sm" className="gap-1" onClick={fetchSample} disabled={loading}>
          <Icon name="play" size={13} /> {loading ? t("preview.building") : result ? t("preview.refresh") : t("preview.fetch")}
        </Button>
      </h3>
      {error && <div className="banner error">{error}</div>}
      {result && !result.db_reachable && <div className="banner error">{result.error}</div>}
      {result?.note && <p className="hint">{result.note}</p>}
      {result?.document !== undefined && result.document !== null && (
        <pre className="yaml">{JSON.stringify(result.document, null, 2)}</pre>
      )}
    </div>
  );
}
