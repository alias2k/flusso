// A tiny path (History-API) router. The embedded SPA is served with a fallback
// to `index.html` for any non-API path (see `apps/design/src/assets.rs`) and the
// Vite dev server does the same, so real paths deep-link and refresh cleanly —
// no `#` fragment:
//
//   /deployment          the deployment config
//   /tables              the database catalog browser
//   /index/users         an index, visual canvas
//   /index/users/code    an index, Code editor
//
// `Route` is the canonical shape of "what the main area shows"; App syncs it
// both ways — store changes push the path, path changes (load, back/forward,
// hand-edited URL) apply to the stores.

export type Route = { view: "deployment" } | { view: "tables" } | { view: "index"; name: string; code: boolean };

/// Parse `window.location.pathname` into a route (`null` = the default landing).
export function parseRoute(pathname: string): Route | null {
  const parts = pathname.replace(/^\/+/, "").split("/").filter(Boolean).map(decodeURIComponent);
  if (parts[0] === "deployment") return { view: "deployment" };
  if (parts[0] === "tables") return { view: "tables" };
  if (parts[0] === "index" && parts[1]) return { view: "index", name: parts[1], code: parts[2] === "code" };
  return null;
}

/// Render a route to an absolute path for `history.pushState`.
export function formatRoute(route: Route): string {
  switch (route.view) {
    case "deployment":
      return "/deployment";
    case "tables":
      return "/tables";
    case "index":
      return `/index/${encodeURIComponent(route.name)}${route.code ? "/code" : ""}`;
  }
}
