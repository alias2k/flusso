import { useState } from "react";
import type { DiagnosticDto, DocumentNode, PreviewResponse, SampleResponse } from "../api";
import { useT } from "../i18n";
import { typeClass } from "../theme";
import { Icon } from "./Icon";

function Node({ node, depth }: { node: DocumentNode; depth: number }) {
  return (
    <div>
      <div className="doc-node" style={{ paddingLeft: depth * 16 }}>
        <span className="doc-name">{node.name}</span>
        <span className={`doc-type ${typeClass(node.type)}`}>
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
  if (!preview) return <div className="preview empty">{t("preview.empty")}</div>;
  const copy = () => {
    navigator.clipboard?.writeText(preview.yaml).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  };
  return (
    <div className="preview">
      {!preview.parse_ok && (
        <div className="banner error">
          <strong>{t("preview.parseError")}</strong> {preview.parse_error}
        </div>
      )}
      {diagnostics && diagnostics.length > 0 && (
        <div className="diagnostics">
          <h3>{t("preview.dbCheck")}</h3>
          {diagnostics.map((d, i) => (
            <div key={i} className={`diag ${d.severity}`}>
              <span className="diag-where">
                {d.index}.{d.field}
              </span>{" "}
              {d.message}
            </div>
          ))}
        </div>
      )}
      <h3>{t("preview.document")}</h3>
      <div className="doc-tree">
        {preview.preview.document.map((n, i) => (
          <Node key={i} node={n} depth={0} />
        ))}
      </div>
      {onSample && preview.parse_ok && <SampleDoc onSample={onSample} />}
      <h3 className="yaml-head">
        {t("preview.schemaYml")}
        <button className="link copy" onClick={copy} title={t("preview.copyYaml")}>
          <Icon name="copy" size={13} /> {copied ? t("preview.copied") : t("preview.copy")}
        </button>
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
    <div className="sample-doc">
      <h3 className="yaml-head">
        {t("preview.sampleFromDb")}
        {result?.synthetic && <span className="badge object">{t("preview.example")}</span>}
        <button className="link" onClick={fetchSample} disabled={loading}>
          <Icon name="play" size={13} /> {loading ? t("preview.building") : result ? t("preview.refresh") : t("preview.fetch")}
        </button>
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
