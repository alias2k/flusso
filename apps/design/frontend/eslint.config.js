import js from "@eslint/js";
import globals from "globals";
import react from "eslint-plugin-react";
import jsxA11y from "eslint-plugin-jsx-a11y";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";
import prettier from "eslint-config-prettier";

export default tseslint.config(
  { ignores: ["dist", "node_modules", "playwright-report", "test-results"] },
  // The SPA: type-aware ("production grade") lint — recommended + stylistic
  // type-checked sets, plus React hooks + Fast-Refresh. Type-checked rules need
  // the TS program, so point the parser at the project (src is in tsconfig).
  {
    files: ["src/**/*.{ts,tsx}"],
    extends: [
      js.configs.recommended,
      ...tseslint.configs.recommendedTypeChecked,
      ...tseslint.configs.stylisticTypeChecked,
      react.configs.flat.recommended,
      react.configs.flat["jsx-runtime"],
      jsxA11y.flatConfigs.recommended,
    ],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
    },
    settings: { react: { version: "detect" } },
    plugins: { "react-hooks": reactHooks, "react-refresh": reactRefresh },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // react-hooks 7's new rule flags setState-in-effect, but our uses are the
      // legitimate kind it can't tell apart: debounced preview, one-shot
      // localStorage init, and the controlled-input resync buffer. Keep
      // rules-of-hooks + exhaustive-deps; drop this one.
      "react-hooks/set-state-in-effect": "off",
      // production lints are errors, not warnings.
      "react-refresh/only-export-components": ["error", { allowConstantExport: true }],
      // TS handles prop typing; the new JSX transform needs no React in scope.
      "react/prop-types": "off",
    },
  },
  // Library-style files that intentionally co-export non-components (shadcn ui
  // primitives ship their cva variants; the context modules export providers +
  // hooks): Fast Refresh doesn't fully apply, so silence that one rule.
  {
    files: ["src/components/ui/**/*.tsx", "src/i18n.tsx", "src/state.tsx"],
    rules: { "react-refresh/only-export-components": "off" },
  },
  // The canvas is a pointer-driven node-graph editor (React Flow owns canvas-
  // level keyboard nav), and its rows wrap interactive controls (Radix checkbox,
  // the remove/chevron buttons) — so making the rows themselves role="button"
  // would create nested-interactive a11y bugs. Click-to-select stays a pointer
  // affordance here; switch off the element-interaction rules for these two.
  {
    files: ["src/components/Canvas.tsx", "src/components/DocNodeView.tsx"],
    rules: {
      "jsx-a11y/click-events-have-key-events": "off",
      "jsx-a11y/no-static-element-interactions": "off",
      "jsx-a11y/no-noninteractive-element-interactions": "off",
    },
  },
  // Node-side bits (e2e harness, build/config, the i18n checker) aren't in the
  // TS project, so they get the non-type-checked recommended set + node globals.
  {
    files: ["e2e/**/*.ts", "scripts/**/*.mjs", "*.config.{js,ts}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { ecmaVersion: 2022, globals: globals.node },
  },
  // Last: turn off every ESLint rule that conflicts with Prettier, so formatting
  // is owned solely by Prettier and the two can't fight. Keep this at the end.
  prettier,
);
