// The words for a name flusso would refuse: an index or sink name (the
// Deployment panel's name boxes and the New index wizard), a field name (the
// Inspector's name box and the canvas's inline rename), and a table or column
// name typed by hand.
//
// ```ts
// const refusal = useNameRefusal("sink");
// refusal("my sink", []); // "\"my sink\" is not a valid sink name: …"
// useFieldNameRefusal()("email", ["email"]); // "Another field here is already named …"
// useSqlNameRefusal()("my table"); // "“my table” is not a valid table or column name: …"
// ```

import { useT } from "../i18n";
import { fieldNameProblem, identifier, identifierProblem, sqlNameProblem, trimmed } from "../model/names";
import type { RenameProblem } from "../model/renames";

/// What a name box names: an `[[index]]` entry or a `[sinks.<name>]` table.
export type NameKind = "index" | "sink";

/// Why a name is refused, in words, or `null` when it can be applied.
export function useNameRefusal(kind: NameKind = "index"): (name: string, taken: string[]) => string | null {
  const { t } = useT();
  return (name, taken) => {
    const at = { name: identifier(name) };
    switch (identifierProblem(name, taken)) {
      case "taken":
        return kind === "index" ? t("config.nameTaken", at) : t("config.sinkNameTaken", at);
      case "pattern":
        return kind === "index" ? t("config.namePattern", at) : t("config.sinkNamePattern", at);
      case "length":
        return kind === "index" ? t("config.nameLength") : t("config.sinkNameLength");
      case null:
        return null;
    }
  };
}

/// Why a field can't take a name at a level whose other fields are `siblings`,
/// in words, or `null` when it can.
export function useFieldNameRefusal(): (name: string, siblings: string[]) => string | null {
  const { t } = useT();
  return (name, siblings) => {
    switch (fieldNameProblem(name, siblings)) {
      case "taken":
        return t("inspector.fieldNameTaken", { name: trimmed(name) });
      case "pattern":
      case "length":
        return t("inspector.fieldNameInvalid");
      case null:
        return null;
    }
  };
}

/// Why flusso would refuse a table or column name, in words, or `null`.
export function useSqlNameRefusal(): (name: string) => string | null {
  const { t } = useT();
  return (name) => {
    switch (sqlNameProblem(name)) {
      case "quoted":
        return t("names.sqlQuoted");
      case "pattern":
        return t("names.sqlPattern", { name: identifier(name) });
      case "length":
        return t("names.sqlLength");
      case null:
        return null;
    }
  };
}

/// The words for a rename `problem` from `model/renames.ts` on the name `text`
/// (`null` for none, and for a thing that is gone: there is nothing to say).
export function useRenameProblemText(
  kind: NameKind | "field",
): (problem: RenameProblem | null, text: string) => string | null {
  const { t } = useT();
  return (problem, text) => {
    const at = { name: kind === "field" ? trimmed(text) : identifier(text) };
    switch (problem) {
      case "taken":
        return kind === "field"
          ? t("inspector.fieldNameTaken", at)
          : kind === "index"
            ? t("config.nameTaken", at)
            : t("config.sinkNameTaken", at);
      case "pattern":
        return kind === "field"
          ? t("inspector.fieldNameInvalid")
          : kind === "index"
            ? t("config.namePattern", at)
            : t("config.sinkNamePattern", at);
      case "length":
        return kind === "field"
          ? t("inspector.fieldNameInvalid")
          : kind === "index"
            ? t("config.nameLength")
            : t("config.sinkNameLength");
      case "gone":
      case null:
        return null;
    }
  };
}
