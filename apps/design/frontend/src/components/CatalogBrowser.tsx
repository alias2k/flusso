import { useState } from "react";
import type { CatalogResponse } from "../api";
import { useT } from "../i18n";
import { typeClass } from "../theme";

/// A read-only browser of the introspected database: every table with its
/// columns (type, pk, nullable), outgoing foreign keys, and the detected
/// junctions — so you can explore the schema independent of the canvas.
export function CatalogBrowser({ catalog, onClose }: { catalog: CatalogResponse; onClose: () => void }) {
  const { t } = useT();
  const [q, setQ] = useState("");
  const tables = catalog.catalog.tables.filter((tbl) => tbl.name.toLowerCase().includes(q.toLowerCase()));
  const junctions = new Set(catalog.junctions.map((j) => j.table.table));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={t("Database tables")} onClick={(e) => e.stopPropagation()}>
        <h3>{t("Database ({n} tables)", { n: catalog.catalog.tables.length })}</h3>
        {catalog.error ? (
          <p className="banner warn">{t("Database not reachable — {err}", { err: catalog.error })}</p>
        ) : (
          <>
            <input className="catalog-filter" placeholder={t("filter tables…")} value={q} onChange={(e) => setQ(e.target.value)} />
            <div className="catalog-list">
              {tables.map((tbl) => (
                <details key={tbl.name} className="catalog-table">
                  <summary>
                    {tbl.name}
                    {junctions.has(tbl.name) && <span className="badge many_to_many">{t("junction")}</span>}
                    <span className="muted"> · {t("{n} cols", { n: tbl.columns.length })}</span>
                  </summary>
                  <div className="catalog-cols">
                    {tbl.columns.map((c) => (
                      <div className="catalog-col" key={c.name}>
                        <span>
                          {c.is_primary_key && <span className="pk-dot" title={t("primary key")} />}
                          {c.name}
                          {c.nullable ? <span className="muted">?</span> : null}
                        </span>
                        <span className={`col-type ${c.suggested_type ? typeClass(String(c.suggested_type)) : "t-other"}`}>
                          {c.sql_type}
                        </span>
                      </div>
                    ))}
                    {tbl.foreign_keys.map((fk, i) => (
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
          <button onClick={onClose}>{t("Close")}</button>
        </div>
      </div>
    </div>
  );
}
