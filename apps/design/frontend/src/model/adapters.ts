import { useEffect, useState } from "react";
import { type AdapterDescription, api } from "../api";

/// The registered adapters, fetched once per session: what the designer renders
/// its source/stream/sink forms from. `null` while loading; empty on failure.
export function useAdapters(): AdapterDescription[] | null {
  const [adapters, setAdapters] = useState<AdapterDescription[] | null>(null);
  useEffect(() => {
    api
      .adapters()
      .then(setAdapters)
      .catch(() => setAdapters([]));
  }, []);
  return adapters;
}
