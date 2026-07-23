import { useState } from "react";
import { ChevronDown, Database, Play } from "lucide-react";
import type { DiagnosticDto, DocumentNode, PreviewResponse, SampleResponse } from "../api";
import { useT } from "../i18n";
import { typeClass } from "../theme";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { CodeBlock } from "./CodeBlock";

type Tab = "document" | "yaml" | "mapping" | "sample";

/// Row grid for the document tree: the name column flexes to its content (11rem
/// floor), so the type sits in a shared column right after the longest name in
/// its group instead of being pushed to the drawer's far edge.
const ROW = "grid grid-cols-[minmax(11rem,max-content)_1fr] items-center gap-x-5 rounded-md px-2 py-1";

/// One document field: a leaf renders its name with an adjacent coloured type
/// chip; a container (object / nested) renders a card — header band + children
/// inside the body — with a kind-coloured stripe down its full height: has_many
/// teal for `nested`, object slate for `object`, the same hues as the canvas
/// edges. Containment, not indentation, is what shows the nesting.
function Node({ node }: { node: DocumentNode }) {
  const suffix = `${node.array ? "[]" : ""}${node.nullable ? "?" : ""}`;
  const [open, setOpen] = useState(true);
  if (node.children) {
    const nested = node.type.startsWith("nested");
    return (
      <div
        className={cn(
          "my-1.5 overflow-hidden rounded-md border border-border border-l-2 bg-secondary/30",
          nested ? "border-l-kind-has_many" : "border-l-kind-object",
        )}
      >
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className={cn(ROW, "w-full cursor-pointer rounded-none bg-secondary py-1.5 text-left hover:bg-accent")}
        >
          <span className="flex items-center gap-2 whitespace-nowrap">
            <ChevronDown
              className={cn("size-3 shrink-0 text-muted-foreground transition-transform", !open && "-rotate-90")}
            />
            <span className="font-semibold text-foreground">{node.name}</span>
          </span>
          <span className="justify-self-start font-mono text-2xs whitespace-nowrap text-muted-foreground">
            {node.type}
            {suffix} · {node.children.length}
          </span>
        </button>
        {/* height-to-auto animation: the grid row interpolates 0fr ⇄ 1fr; the
            min-h-0 child is what lets the track actually clamp the content.
            The header/body divider rides inside the clipped content (border-t
            on the padded div, not border-b on the button) so it slides away
            with the body instead of popping. */}
        <div
          className={cn(
            "grid transition-[grid-template-rows] duration-200 ease-out motion-reduce:transition-none",
            open ? "grid-rows-[1fr]" : "grid-rows-[0fr]",
          )}
        >
          <div className="min-h-0 overflow-hidden">
            <div className="border-t border-border p-1">
              {node.children.map((c, i) => (
                <Node key={i} node={c} />
              ))}
            </div>
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className={cn(ROW, "hover:bg-accent")}>
      <span className="flex items-center gap-2 whitespace-nowrap">
        <span className="w-3 shrink-0" />
        <span className="text-foreground">{node.name}</span>
      </span>
      <span
        className={cn(
          "justify-self-start rounded border border-current/30 bg-current/5 px-1.5 py-0.5 font-mono text-2xs whitespace-nowrap",
          typeClass(node.type),
        )}
      >
        {node.type}
        {suffix && <span className="opacity-60">{suffix}</span>}
      </span>
    </div>
  );
}

/// The schema preview: what the current index compiles to, split into tabs —
/// the shaped Document tree, the `*.schema.yml` output, the OpenSearch mapping,
/// and a Sample document built from a live row. Fills its container (the drawer).
export function Preview({
  index,
  preview,
  diagnostics,
  onSample,
}: {
  /// The active index name — keys the sample so it resets when the index changes.
  index: string;
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
    // `data-vaul-no-drag` + `select-text`: the preview is mostly text the user
    // wants to select, so opt the whole panel out of Vaul's pointer-drag (a
    // right-drawer drag is horizontal — indistinguishable from a text
    // selection, so it was dismissing the drawer) and re-enable selection,
    // which Vaul otherwise suppresses with `user-select: none`.
    <div className="flex min-h-0 flex-1 flex-col select-text" data-vaul-no-drag>
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

        {/* Kept mounted (hidden) so a fetched sample survives tab switches;
            keyed by index so it resets when you switch indexes. */}
        {onSample && (
          <div className={cn(tab !== "sample" && "hidden")}>
            <SampleDoc key={index} onSample={onSample} />
          </div>
        )}
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

  // Loaded: the built document, with refresh in the code corner and, when the
  // sample is synthetic or carries a note, a small caption above it.
  if (doc !== undefined && doc !== null && result?.db_reachable !== false) {
    const caption = Boolean(result?.synthetic) || Boolean(result?.note);
    return (
      <div>
        {caption && (
          <div className="mb-2 flex items-center gap-2 text-2xs text-muted-foreground">
            {result?.synthetic && <span className="badge object">{t("preview.example")}</span>}
            {result?.note && <span>{result.note}</span>}
          </div>
        )}
        <CodeBlock text={JSON.stringify(doc, null, 2)} lang="json" onRefresh={fetchSample} refreshing={loading} />
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
