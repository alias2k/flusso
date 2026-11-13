// The words for an index or sink name flusso would refuse, shared by the
// Deployment panel's name boxes and the New index wizard.
//
// ```ts
// const refusal = useNameRefusal("sink");
// refusal("my sink", []); // "\"my sink\" is not a valid sink name: …"
// ```

import { useT } from "../i18n";
import { identifier, identifierProblem } from "../model/names";

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
