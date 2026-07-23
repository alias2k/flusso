// A tiny hash router. The embedded SPA is served at `/` with the JSON API
// beside it, so hash routes need no server-side fallback handling:
//
//   #/deployment          the deployment config
//   #/tables              the database catalog browser
//   #/index/users         an index, visual canvas
//   #/index/users/code    an index, Code editor
//
// `Route` is the canonical shape of "what the main area shows"; App syncs it
// both ways — store changes push the hash, hash changes (load, back/forward,
// hand-edited URL) apply to the stores.

export type Route = { view: "deployment" } | { view: "tables" } | { view: "index"; name: string; code: boolean };

export function parseRoute(hash: string): Route | null {
  const parts = hash.replace(/^#\/?/, "").split("/").filter(Boolean).map(decodeURIComponent);
  if (parts[0] === "deployment") return { view: "deployment" };
  if (parts[0] === "tables") return { view: "tables" };
  if (parts[0] === "index" && parts[1]) return { view: "index", name: parts[1], code: parts[2] === "code" };
  return null;
}

export function formatRoute(route: Route): string {
  switch (route.view) {
    case "deployment":
      return "#/deployment";
    case "tables":
      return "#/tables";
    case "index":
      return `#/index/${encodeURIComponent(route.name)}${route.code ? "/code" : ""}`;
  }
}
