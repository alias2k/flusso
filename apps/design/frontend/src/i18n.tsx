import { createContext, type ReactNode, useContext, useMemo, useState } from "react";
import { en } from "./locales/en";
import { it } from "./locales/it";

/// The languages the designer UI ships, by BCP-47 code. `en` is the base
/// catalog; other languages fall back to it for any missing key.
export const LANGS: Record<string, string> = { en: "English", it: "Italiano" };
export type Lang = keyof typeof LANGS;

export type Catalog = Record<string, string>;

const catalogs: Record<Lang, Catalog> = { en, it };

const STORE_KEY = "flusso-design.lang";

type Vars = Record<string, string | number>;

/// Index of the brace that closes the one opened at `open`.
function matchBrace(text: string, open: number): number {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "{") depth++;
    else if (text[i] === "}" && --depth === 0) return i;
  }
  return text.length;
}

/// Split a plural/select body (`one {…} other {…}`) into its keyword→message arms.
function parseArms(body: string): { key: string; message: string }[] {
  const arms: { key: string; message: string }[] = [];
  let i = 0;
  while (i < body.length) {
    while (i < body.length && /\s/.test(body[i] ?? "")) i++;
    let key = "";
    while (i < body.length && body[i] !== "{" && !/\s/.test(body[i] ?? "")) key += body[i++];
    while (i < body.length && /\s/.test(body[i] ?? "")) i++;
    if (body[i] !== "{") break;
    const close = matchBrace(body, i);
    arms.push({ key, message: body.slice(i + 1, close) });
    i = close + 1;
  }
  return arms;
}

/// Evaluate one `{…}` placeholder: a bare argument, or an ICU `plural`/`select`.
function formatArg(inner: string, vars: Vars, lang: Lang): string {
  const c1 = inner.indexOf(",");
  if (c1 === -1) {
    const name = inner.trim();
    return name in vars ? String(vars[name]) : `{${name}}`;
  }
  const name = inner.slice(0, c1).trim();
  const c2 = inner.indexOf(",", c1 + 1);
  const kind = inner.slice(c1 + 1, c2).trim();
  const arms = parseArms(inner.slice(c2 + 1));
  const value = vars[name];

  if (kind === "plural") {
    const n = Number(value);
    const arm =
      arms.find((a) => a.key === `=${n}`) ??
      arms.find((a) => a.key === new Intl.PluralRules(lang).select(n)) ??
      arms.find((a) => a.key === "other");
    return arm ? format(arm.message.replace(/#/g, String(n)), vars, lang) : "";
  }
  if (kind === "select") {
    const arm = arms.find((a) => a.key === String(value)) ?? arms.find((a) => a.key === "other");
    return arm ? format(arm.message, vars, lang) : "";
  }
  return "";
}

/// A small ICU MessageFormat evaluator: named arguments, `plural` and `select`,
/// `#` for the plural number, and `'…'` to quote literal braces. Enough for the
/// catalog's needs; the syntax is standard ICU, so messages stay portable.
function format(message: string, vars: Vars, lang: Lang): string {
  let out = "";
  for (let i = 0; i < message.length;) {
    const ch = message[i];
    if (ch === "'") {
      if (message[i + 1] === "'") {
        out += "'";
        i += 2;
      } else {
        const end = message.indexOf("'", i + 1);
        if (end === -1) {
          out += message.slice(i + 1);
          break;
        }
        out += message.slice(i + 1, end);
        i = end + 1;
      }
    } else if (ch === "{") {
      const close = matchBrace(message, i);
      out += formatArg(message.slice(i + 1, close), vars, lang);
      i = close + 1;
    } else {
      out += ch;
      i++;
    }
  }
  return out;
}

export type Translate = (key: string, vars?: Vars) => string;

interface I18n {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: Translate;
}

const I18nContext = createContext<I18n | null>(null);

function initialLang(): Lang {
  try {
    const saved = localStorage.getItem(STORE_KEY);
    return saved && saved in LANGS ? saved : "en";
  } catch {
    return "en";
  }
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(initialLang);
  const value = useMemo<I18n>(() => {
    const setLang = (next: Lang) => {
      setLangState(next);
      try {
        localStorage.setItem(STORE_KEY, next);
      } catch {
        /* private mode — language just won't persist */
      }
    };
    const t: Translate = (key, vars = {}) => format(catalogs[lang]?.[key] ?? en[key] ?? key, vars, lang);
    return { lang, setLang, t };
  }, [lang]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

/// Access the translator and current language. `t("ns.key", vars?)` resolves the
/// key against the active catalog (falling back to English) and renders its ICU
/// message with `vars`.
export function useT(): I18n {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useT must be used within I18nProvider");
  return ctx;
}
