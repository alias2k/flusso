import type { DiagnosticDto, DocumentNode, PreviewResponse } from "../api";

function Node({ node, depth }: { node: DocumentNode; depth: number }) {
  return (
    <div>
      <div className="doc-node" style={{ paddingLeft: depth * 16 }}>
        <span className="doc-name">{node.name}</span>
        <span className="doc-type">
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
}: {
  preview: PreviewResponse | null;
  diagnostics: DiagnosticDto[] | null;
}) {
  if (!preview) return <div className="preview empty">Select or edit an index to preview it.</div>;
  return (
    <div className="preview">
      {!preview.parse_ok && (
        <div className="banner error">
          <strong>This schema does not parse:</strong> {preview.parse_error}
        </div>
      )}
      {diagnostics && diagnostics.length > 0 && (
        <div className="diagnostics">
          <h3>Database check</h3>
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
      <h3>Document</h3>
      <div className="doc-tree">
        {preview.preview.document.map((n, i) => (
          <Node key={i} node={n} depth={0} />
        ))}
      </div>
      <h3>schema.yml</h3>
      <pre className="yaml">{preview.yaml}</pre>
    </div>
  );
}
