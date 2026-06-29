import { useState } from "react";
import type { CatalogResponse } from "../api";
import { useT } from "../i18n";
import { typeClass } from "../theme";
import { Text } from "./widgets";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

/// A read-only browser of the introspected database: every table with its
/// columns (type, pk, nullable), outgoing foreign keys, and the detected
/// junctions — so you can explore the schema independent of the canvas.
export function CatalogBrowser({ catalog, onClose }: { catalog: CatalogResponse; onClose: () => void }) {
  const { t } = useT();
  const [q, setQ] = useState("");
  const tables = catalog.catalog.tables.filter((tbl) => tbl.name.toLowerCase().includes(q.toLowerCase()));
  const junctions = new Set(catalog.junctions.map((j) => j.table.table));

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex flex-col w-[min(40rem,92vw)] max-w-none max-h-[85vh]" aria-label={t("catalog.aria")}>
        <DialogHeader>
          <DialogTitle>{t("catalog.title", { n: catalog.catalog.tables.length })}</DialogTitle>
        </DialogHeader>
        {catalog.error ? (
          <p className="banner warn">{t("catalog.dbError", { err: catalog.error })}</p>
        ) : (
          <>
            <Text className="catalog-filter" value={q} onChange={setQ} placeholder={t("catalog.filter")} />
            <div className="catalog-list min-h-0">
              {tables.map((tbl) => (
                <details key={tbl.name} className="catalog-table">
                  <summary>
                    {tbl.name}
                    {junctions.has(tbl.name) && <span className="badge many_to_many">{t("catalog.junction")}</span>}
                    <span className="muted"> · {t("catalog.cols", { n: tbl.columns.length })}</span>
                  </summary>
                  <div className="catalog-cols">
                    {tbl.columns.map((c) => (
                      <div className="catalog-col" key={c.name}>
                        <span>
                          {c.is_primary_key && <span className="pk-dot" title={t("catalog.pk")} />}
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
        <DialogFooter>
          <Button variant="secondary" size="sm" onClick={onClose}>
            {t("common.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
