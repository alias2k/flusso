import { useState } from "react";
import type { CatalogResponse } from "../api";
import { typeClass } from "../theme";

/// A read-only browser of the introspected database: every table with its
/// columns (type, pk, nullable), outgoing foreign keys, and the detected
/// junctions — so you can explore the schema independent of the canvas.
export function CatalogBrowser({ catalog, onClose }: { catalog: CatalogResponse; onClose: () => void }) {
  const [q, setQ] = useState("");
  const tables = catalog.catalog.tables.filter((t) => t.name.toLowerCase().includes(q.toLowerCase()));
  const junctions = new Set(catalog.junctions.map((j) => j.table.table));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" role="dialog" aria-modal="true" aria-label="Database tables" onClick={(e) => e.stopPropagation()}>
        <h3>Database ({catalog.catalog.tables.length} tables)</h3>
        {catalog.error ? (
          <p className="banner warn">Database not reachable — {catalog.error}</p>
        ) : (
          <>
            <input className="catalog-filter" placeholder="filter tables…" value={q} onChange={(e) => setQ(e.target.value)} />
            <div className="catalog-list">
              {tables.map((t) => (
                <details key={t.name} className="catalog-table">
                  <summary>
                    {t.name}
                    {junctions.has(t.name) && <span className="badge many_to_many">junction</span>}
                    <span className="muted"> · {t.columns.length} cols</span>
                  </summary>
                  <div className="catalog-cols">
                    {t.columns.map((c) => (
                      <div className="catalog-col" key={c.name}>
                        <span>
                          {c.is_primary_key && <span className="pk-dot" title="primary key" />}
                          {c.name}
                          {c.nullable ? <span className="muted">?</span> : null}
                        </span>
                        <span className={`col-type ${c.suggested_type ? typeClass(String(c.suggested_type)) : "t-other"}`}>
                          {c.sql_type}
                        </span>
                      </div>
                    ))}
                    {t.foreign_keys.map((fk, i) => (
                      <div className="catalog-fk" key={i}>
                        {fk.columns.join(", ")} → {fk.references_table}.{fk.references_columns.join(", ")}
                      </div>
                    ))}
                  </div>
                </details>
              ))}
            </div>
          </>
        )}
        <div className="modal-actions">
          <button onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
