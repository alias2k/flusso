import { createContext, type ReactNode, useContext, useMemo, useState } from "react";
import { it } from "./locales/it";

/// The languages the designer UI ships. English is the source language — its
/// strings live inline in the components (the message *key* is the English
/// text), so it needs no catalog; other languages map those keys to a
/// translation, falling back to the English key when one is missing.
export const LANGS: Record<string, string> = { en: "English", it: "Italiano" };
export type Lang = keyof typeof LANGS;

const catalogs: Partial<Record<Lang, Record<string, string>>> = { it };

const STORE_KEY = "flusso-design.lang";

/// Fill `{name}` placeholders in a template from `vars`.
function interpolate(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name) => (name in vars ? String(vars[name]) : whole));
}

export type Translate = (key: string, vars?: Record<string, string | number>) => string;

interface I18n {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: Translate;
}

const I18nContext = createContext<I18n | null>(null);

function initialLang(): Lang {
  const saved = localStorage.getItem(STORE_KEY);
  return saved && saved in LANGS ? saved : "en";
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
    const t: Translate = (key, vars) => interpolate(catalogs[lang]?.[key] ?? key, vars);
    return { lang, setLang, t };
  }, [lang]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

/// Access the translator and current language. The returned `t(key, vars?)`
/// takes the English string as its key and fills `{name}` placeholders.
export function useT(): I18n {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useT must be used within I18nProvider");
  return ctx;
}
