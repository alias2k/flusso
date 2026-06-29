import { expect, test } from "@playwright/test";

// Every test starts on a fresh load (the server re-reads the files each request,
// and unsaved edits live only in the browser), and fails on any uncaught page
// error — these flows are exactly the interaction bugs that slipped past
// build/curl checks (the delete crash, the dark sidebar, clipped/overlapping
// nodes).
let pageErrors: string[] = [];

test.beforeEach(async ({ page }) => {
  pageErrors = [];
  page.on("pageerror", (e) => pageErrors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error") pageErrors.push(m.text());
  });
  await page.goto("/");
  await page.locator(".flow-node").first().waitFor();
});

test.afterEach(() => {
  expect(pageErrors, "no console/page errors").toEqual([]);
});

test("loads the project as a node graph", async ({ page }) => {
  await expect(page.locator(".flow-node")).not.toHaveCount(0);
  await expect(page.locator(".flow-node.kind-root")).toHaveCount(1);
});

test("adds a relation from an FK suggestion", async ({ page }) => {
  const before = await page.locator(".flow-node").count();
  await page.locator(".flow-node.kind-root .suggest").first().click();
  await expect(page.locator(".flow-node")).toHaveCount(before + 1);
});

test("selecting a column opens the inspector", async ({ page }) => {
  await page.locator(".flow-node.kind-root .col-row.on").first().click();
  await expect(page.locator(".inspector")).toBeVisible();
  await expect(page.locator(".inspector h3")).toContainText("Field");
});

test("deleting a node does not crash (regression)", async ({ page }) => {
  const before = await page.locator(".flow-node").count();
  await page.locator(".flow-node:not(.kind-root) .x").first().click();
  await expect(page.locator(".flow-node")).toHaveCount(before - 1);
  // afterEach asserts no page error — this used to throw on stale paths.
});

test("collapsing the sidebar keeps the canvas visible (regression)", async ({ page }) => {
  await expect(page.locator(".sidebar")).toBeVisible();
  await page.getByRole("button", { name: "Hide sidebar" }).click();
  await expect(page.locator(".sidebar")).toHaveCount(0);
  // the canvas (and its nodes) must still render, not go dark
  await expect(page.locator(".react-flow")).toBeVisible();
  await expect(page.locator(".flow-node")).not.toHaveCount(0);
});

test("undo reverts a structural edit", async ({ page }) => {
  const before = await page.locator(".flow-node").count();
  await page.locator(".flow-node.kind-root .suggest").first().click();
  await expect(page.locator(".flow-node")).toHaveCount(before + 1);
  await page.keyboard.press("ControlOrMeta+z");
  await expect(page.locator(".flow-node")).toHaveCount(before);
});

test("editing marks the index unsaved", async ({ page }) => {
  await expect(page.locator(".sidebar .dirty-dot")).toHaveCount(0);
  await page.locator(".flow-node.kind-root .col-row:not(.on) input[type=checkbox]").first().click();
  await expect(page.locator(".sidebar .dirty-dot")).not.toHaveCount(0);
});

test("required columns carry a marker on the row", async ({ page }) => {
  // the users root has a required column (email), so at least one dot renders.
  await expect(page.locator(".flow-node.kind-root .col-req")).not.toHaveCount(0);
});

test("collapsing a node hides its columns", async ({ page }) => {
  await expect(page.locator(".flow-node.kind-root .node-cols")).toBeVisible();
  await page.locator(".flow-node.kind-root .chevron").click();
  await expect(page.locator(".flow-node.kind-root .node-cols")).toHaveCount(0);
});

test("include-all checks every column", async ({ page }) => {
  const root = page.locator(".flow-node.kind-root");
  const boxes = root.locator('.col-row input[type="checkbox"]');
  const n = await boxes.count();
  await root.getByRole("button", { name: "all", exact: true }).click();
  await expect(root.locator('.col-row input[type="checkbox"]:checked')).toHaveCount(n);
});

test("node search jumps to a node", async ({ page }) => {
  await page.locator(".node-search input").fill("users");
  await page.locator(".node-search li").first().click();
  await expect(page.locator(".inspector")).toBeVisible();
});

test("Delete key removes the selected node", async ({ page }) => {
  const before = await page.locator(".flow-node").count();
  await page.locator(".flow-node:not(.kind-root) header").first().click();
  await page.keyboard.press("Delete");
  await expect(page.locator(".flow-node")).toHaveCount(before - 1);
});

test("Escape clears the selection", async ({ page }) => {
  await page.locator(".flow-node.kind-root .col-row.on").first().click();
  await expect(page.locator(".inspector")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator(".inspector")).toHaveCount(0);
});

test("inspector shows a breadcrumb for the selection", async ({ page }) => {
  await page.locator(".flow-node.kind-root .col-row.on").first().click();
  await expect(page.locator(".crumbs")).toBeVisible();
});

test("the sidebar shows a kind legend", async ({ page }) => {
  await expect(page.locator(".legend")).toBeVisible();
  await expect(page.locator(".legend-row")).toHaveCount(6);
});

test("save shows a diff before writing", async ({ page }) => {
  await page.locator(".flow-node.kind-root .col-row:not(.on) input[type=checkbox]").first().click();
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".modal")).toBeVisible();
  await expect(page.locator(".diff-file")).not.toHaveCount(0);
});

test("raw YAML mode opens an editor", async ({ page }) => {
  await page.getByRole("button", { name: "Raw YAML" }).click();
  await expect(page.locator(".raw-editor")).toBeVisible();
});

test("config panel edits sinks", async ({ page }) => {
  await page.getByRole("button", { name: /Deployment/ }).click();
  await expect(page.locator(".sink-editor")).not.toHaveCount(0);
});

test("connection editor switches to env mode", async ({ page }) => {
  await page.getByRole("button", { name: /Deployment/ }).click();
  await page.locator(".connection-editor").getByRole("combobox").click();
  await page.locator("[data-slot=select-content]").getByRole("option", { name: "env", exact: true }).click();
  await expect(page.getByText("env var")).toBeVisible();
});

test("renaming an index keeps its schema (regression)", async ({ page }) => {
  await page.getByRole("button", { name: /Deployment/ }).click();
  await page.locator(".index-entry input").first().fill("renamed_idx");
  await page.locator(".sidebar .nav", { hasText: "renamed_idx" }).click();
  // schema preserved → the root node still has its column list (not the empty state)
  await expect(page.locator(".flow-node.kind-root .node-cols")).toBeVisible();
});

test("the DB chip re-tests the connection", async ({ page }) => {
  await page.locator(".db-chip").click();
  await expect(page.locator(".toast")).toBeVisible();
});

test("preview drawer shows the OpenSearch mapping", async ({ page }) => {
  await page.getByRole("button", { name: "YAML", exact: true }).click();
  await expect(page.locator(".mapping-details")).toBeVisible();
});

test("sample document builds a real row from the database", async ({ page }) => {
  await page.getByRole("button", { name: "YAML", exact: true }).click();
  await expect(page.locator(".sample-doc")).toBeVisible();
  await page.locator(".sample-doc").getByRole("button", { name: /fetch/ }).click();
  // DB is seeded → a JSON document is rendered (a note/banner would mean empty).
  await expect(page.locator(".sample-doc pre")).toBeVisible();
});

test("duplicating a node adds a copy", async ({ page }) => {
  const before = await page.locator(".flow-node").count();
  await page.locator(".flow-node:not(.kind-root) header").first().click();
  await page.getByRole("button", { name: "Duplicate" }).click();
  await expect(page.locator(".flow-node")).toHaveCount(before + 1);
});

test("duplicating an index adds a sidebar entry", async ({ page }) => {
  const before = await page.locator(".sidebar .nav").count();
  await page.getByRole("button", { name: /Deployment/ }).click();
  await page.locator(".index-entry .link", { hasText: "dup" }).first().click();
  await expect(page.locator(".sidebar .nav")).toHaveCount(before + 1);
});

test("catalog browser lists the database tables", async ({ page }) => {
  await page.getByRole("button", { name: "Tables" }).click();
  await expect(page.locator(".catalog-table")).not.toHaveCount(0);
});

test("marking a nullable source column required demands a default", async ({ page }) => {
  // fullName maps to users.full_name, which is nullable in the source.
  await page.locator(".flow-node.kind-root .col-row", { hasText: "full_name" }).first().click();
  await expect(page.locator(".inspector")).toBeVisible();
  const required = page.locator(".inspector").getByRole("checkbox", { name: "required" });
  await required.click();
  // required over a nullable column → the default becomes mandatory.
  await expect(page.locator(".inspector .error-hint")).toBeVisible();
  await expect(page.locator(".inspector input.invalid")).toBeVisible();
  // the node highlights it as an error too.
  await expect(page.locator(".flow-node.kind-root .col-row.diag-error")).not.toHaveCount(0);
  // setting a default clears the requirement.
  await page.locator(".inspector").getByPlaceholder(/e\.g\. 0/).fill('"n/a"');
  await expect(page.locator(".inspector .error-hint")).toHaveCount(0);
  await expect(page.locator(".inspector input.invalid")).toHaveCount(0);
});

test("a belongs_to join is steered by its FK column", async ({ page }) => {
  // orders has an outgoing FK (user_id → users), which is NOT NULL in the source.
  await page.locator(".sidebar .nav", { hasText: "orders" }).click();
  await page.locator(".flow-node.kind-root").first().waitFor();
  await page.locator(".flow-node.kind-root .suggest", { hasText: "belongs_to" }).first().click();
  await page.locator(".flow-node.kind-belongs_to header").first().click();
  await expect(page.locator(".inspector")).toContainText("FK column");
  await expect(page.locator(".inspector")).toContainText("NOT NULL");
});

test("the type dropdown nudges toward the source-suggested type", async ({ page }) => {
  await page.locator(".flow-node.kind-root .col-row", { hasText: "full_name" }).first().click();
  await expect(page.locator(".inspector")).toBeVisible();
  // a text column never suggests boolean → picking it shows the nudge
  await page.locator(".inspector .field", { hasText: "type" }).getByRole("combobox").click();
  await page.locator("[data-slot=select-content]").getByRole("option", { name: "boolean", exact: true }).click();
  await expect(page.locator(".inspector .nudge")).toContainText("suggested");
  await page.locator(".inspector").getByRole("button", { name: "use", exact: true }).click();
  await expect(page.locator(".inspector .nudge")).toHaveCount(0);
});

test("rename chips offer source-derived names", async ({ page }) => {
  // the field is named 'fullName' over column 'full_name', so the chip back to
  // the raw column name is offered; clicking it renames the document field.
  await page.locator(".flow-node.kind-root .col-row", { hasText: "full_name" }).first().click();
  await expect(page.locator(".inspector .rename-chips")).toBeVisible();
  await page.locator(".inspector .rchip", { hasText: "full_name" }).click();
  await expect(page.locator(".inspector .blk.doc input").first()).toHaveValue("full_name");
});

test("the inspector reads as source → document blocks", async ({ page }) => {
  await page.locator(".flow-node.kind-root .col-row.on").first().click();
  await expect(page.locator(".inspector .blk.src")).toBeVisible();
  await expect(page.locator(".inspector .blk.doc")).toBeVisible();
});

test("advanced drawer adds a knob without breaking validation", async ({ page }) => {
  await page.locator(".flow-node.kind-root .col-row.on").first().click();
  await page.locator(".inspector .drawer-h", { hasText: /advanced/i }).click();
  await page.locator(".inspector .knob", { hasText: /^analyzer$/ }).click();
  await expect(page.locator(".inspector .opt-row")).toHaveCount(1);
  await expect(page.locator(".banner.error")).toHaveCount(0);
});

test("adding an option keeps the schema valid (regression)", async ({ page }) => {
  // a fresh option used to post an empty string — an invalid GenericValue —
  // which 400'd the live preview/validate instantly. It must now round-trip.
  await page.locator(".flow-node.kind-root .col-row.on").first().click();
  await page.locator(".inspector .drawer-h", { hasText: /advanced/i }).click();
  await page.locator(".inspector .addline", { hasText: /option/ }).click();
  await expect(page.locator(".inspector .opt-row")).toHaveCount(1);
  await expect(page.locator(".banner.error")).toHaveCount(0);
  await page.getByRole("button", { name: "Validate" }).click();
  await expect(page.locator(".toast")).toBeVisible();
  await expect(page.locator(".toast")).not.toContainText(/failed|deserialize/i);
});

test("validate surfaces a result toast", async ({ page }) => {
  await page.getByRole("button", { name: "Validate" }).click();
  await expect(page.locator(".toast")).toBeVisible();
});

test("switches the UI language", async ({ page }) => {
  await expect(page.getByRole("button", { name: "Save" })).toBeVisible();
  await page.locator(".lang-select").selectOption("it");
  // labels switch to Italian…
  await expect(page.getByRole("button", { name: "Salva" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Convalida" })).toBeVisible();
  // …and back to English.
  await page.locator(".lang-select").selectOption("en");
  await expect(page.getByRole("button", { name: "Save" })).toBeVisible();
});

test("toggles the light/dark theme", async ({ page }) => {
  await page.getByRole("button", { name: "Toggle light/dark theme" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
});

test("canvas controls carry hover descriptions", async ({ page }) => {
  await expect(page.locator('.panel-btn[data-tip="Reset layout"]')).toBeVisible();
  await expect(page.locator('.panel-btn[data-tip="Show minimap"]')).toBeVisible();
});

test("toggles the minimap", async ({ page }) => {
  await expect(page.locator(".react-flow__minimap")).toHaveCount(0);
  await page.locator(".map-toggle").click();
  await expect(page.locator(".react-flow__minimap")).toHaveCount(1);
});
