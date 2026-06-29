import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "node_modules", "playwright-report", "test-results"] },
  // The SPA: browser globals, React hooks + Fast-Refresh rules.
  {
    files: ["src/**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { ecmaVersion: 2022, globals: globals.browser },
    plugins: { "react-hooks": reactHooks, "react-refresh": reactRefresh },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // react-hooks 7's new rule flags setState-in-effect, but our uses are the
      // legitimate kind it can't tell apart: debounced preview, one-shot
      // localStorage init, and the controlled-input resync buffer. Keep
      // rules-of-hooks + exhaustive-deps; drop this one.
      "react-hooks/set-state-in-effect": "off",
      // shadcn ui components export a component + its cva variants alongside it.
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
    },
  },
  // Library-style files that intentionally co-export non-components (shadcn ui
  // primitives ship their cva variants; the context modules export providers +
  // hooks): Fast Refresh doesn't fully apply, so silence that one rule.
  {
    files: ["src/components/ui/**/*.tsx", "src/i18n.tsx", "src/state.tsx"],
    rules: { "react-refresh/only-export-components": "off" },
  },
  // Node-side bits: e2e harness, build/config, the i18n checker.
  {
    files: ["e2e/**/*.ts", "scripts/**/*.mjs", "*.config.{js,ts}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { ecmaVersion: 2022, globals: globals.node },
  },
);
