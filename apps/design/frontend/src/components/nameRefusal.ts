// The words for an index name flusso would refuse, shared by the Deployment
// panel's name boxes and the New index wizard.
//
// ```ts
// const refusal = useNameRefusal();
// refusal("my users", []); // "\"my users\" is not a valid index name: …"
// ```

import { useT } from "../i18n";
import { indexName, nameProblem } from "../model/indexRows";

/// Why an index name is refused, in words, or `null` when it can be applied.
export function useNameRefusal(): (name: string, taken: string[]) => string | null {
  const { t } = useT();
  return (name, taken) => {
    switch (nameProblem(name, taken)) {
      case "taken":
        return t("config.nameTaken", { name: indexName(name) });
      case "pattern":
        return t("config.namePattern", { name: indexName(name) });
      case "length":
        return t("config.nameLength");
      case null:
        return null;
    }
  };
}
