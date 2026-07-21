import { Check, Copy } from "lucide-react";
import { Fragment, type ReactNode, useState } from "react";
import { useT } from "../i18n";
import { Hint } from "./Hint";

// A tiny, dependency-free syntax highlighter for the preview's YAML and JSON,
// themed to the flusso palette (keys cyan; strings amber, numbers blue, bools
// teal — the type-family hues; punctuation/comments muted). The input is
// machine-generated, so a line/token regex is enough; it never needs to parse
// arbitrary YAML/JSON.

/// Colour a bare YAML scalar by its apparent type.
function yamlValue(v: string): ReactNode {
  if (v === "true" || v === "false" || v === "null") return <span className="t-bool">{v}</span>;
  if (/^-?\d+(\.\d+)?$/.test(v)) return <span className="t-number">{v}</span>;
  return <span className="t-string">{v}</span>;
}

function yamlLine(line: string): ReactNode {
  const trimmed = line.trimStart();
  const indent = line.slice(0, line.length - trimmed.length);
  if (trimmed.startsWith("#")) return <span className="text-muted-foreground/60 italic">{line}</span>;

  let bullet = "";
  let rest = trimmed;
  if (rest.startsWith("- ")) {
    bullet = "- ";
    rest = rest.slice(2);
  }
  const kv = /^([^:#\s][^:]*?):(\s*)(.*)$/.exec(rest);
  return (
    <>
      {indent}
      {bullet && <span className="text-muted-foreground">{bullet}</span>}
      {kv ? (
        <>
          <span className="text-accent2">{kv[1]}</span>
          <span className="text-muted-foreground">:</span>
          {kv[2]}
          {kv[3] && yamlValue(kv[3])}
        </>
      ) : (
        rest && yamlValue(rest)
      )}
    </>
  );
}

// One token: a string (optionally a key when followed by `:`), a number, a
// literal, or a punctuation char. Whitespace between tokens is emitted verbatim.
const JSON_RE = /("(?:\\.|[^"\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b|([{}[\],])/g;

function jsonLine(line: string): ReactNode {
  const out: ReactNode[] = [];
  let last = 0;
  JSON_RE.lastIndex = 0;
  for (let m = JSON_RE.exec(line); m; m = JSON_RE.exec(line)) {
    if (m.index > last) out.push(line.slice(last, m.index));
    if (m[1] !== undefined) {
      if (m[2] !== undefined) {
        out.push(<span className="text-accent2">{m[1]}</span>, <span className="text-muted-foreground">{m[2]}</span>);
      } else {
        out.push(<span className="t-string">{m[1]}</span>);
      }
    } else if (m[3] !== undefined) {
      out.push(<span className="t-number">{m[3]}</span>);
    } else if (m[4] !== undefined) {
      out.push(<span className="t-bool">{m[4]}</span>);
    } else if (m[5] !== undefined) {
      out.push(<span className="text-muted-foreground">{m[5]}</span>);
    }
    last = JSON_RE.lastIndex;
  }
  if (last < line.length) out.push(line.slice(last));
  return out.map((t, i) => <Fragment key={i}>{t}</Fragment>);
}

/// A highlighted code block reusing the `pre.yaml` surface, with a copy button
/// pinned to its top-right corner. Lines are joined by newlines (preserved by
/// `white-space: pre`), so blank lines survive.
export function CodeBlock({ text, lang }: { text: string; lang: "yaml" | "json" }) {
  const { t } = useT();
  const [copied, setCopied] = useState(false);
  const render = lang === "yaml" ? yamlLine : jsonLine;
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
      <Hint label={copied ? t("preview.copied") : t("preview.copy")} side="left">
        <button
          type="button"
          aria-label={t("preview.copy")}
          onClick={() => void copy()}
          className="absolute top-2 right-2 z-10 grid size-7 cursor-pointer place-items-center rounded-md border border-border bg-secondary/80 text-muted-foreground backdrop-blur transition-colors hover:text-foreground"
        >
          {copied ? <Check className="size-3.5 text-primary" /> : <Copy className="size-3.5" />}
        </button>
      </Hint>
      <pre className="yaml">
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
