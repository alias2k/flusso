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
  await page.locator(".topbar button.icon").first().click();
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

test("validate surfaces a result toast", async ({ page }) => {
  await page.getByRole("button", { name: "Validate" }).click();
  await expect(page.locator(".toast")).toBeVisible();
});

test("toggles the minimap", async ({ page }) => {
  await expect(page.locator(".react-flow__minimap")).toHaveCount(0);
  await page.locator(".map-toggle").click();
  await expect(page.locator(".react-flow__minimap")).toHaveCount(1);
});
