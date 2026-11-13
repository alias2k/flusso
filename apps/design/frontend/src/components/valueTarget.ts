// The field a `GenericInput` writes to, provided by the inspector around one
// field. A typed value is applied to that field (by uid) and named by `label`
// when Save has to refuse it; `GenericInput` adds which value (`default`,
// `options.<key>`) to both.

import { createContext } from "react";

export interface ValueTarget {
  /// Index name and field uid: what a typed value is addressed to.
  id: string;
  /// The field as the user reads it: `users.items.total`.
  label: string;
}

export const ValueTargetContext = createContext<ValueTarget>({ id: "", label: "" });
