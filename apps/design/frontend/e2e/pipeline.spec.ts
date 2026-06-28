import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { expect, test } from "@playwright/test";
import { flussoBin, projectDir } from "./paths";

// The end-to-end contract: edit in the UI → Save → the regenerated files are a
// valid flusso config. We prove it by running the real `flusso check` against
// exactly what the designer wrote.
test("UI edit → save → flusso check accepts the output", async ({ page }) => {
  await page.goto("/");
  await page.locator(".flow-node").first().waitFor();

  // Make a real change: include the first not-yet-included column on the root.
  // Use click (not check): including a column re-renders the row, which would
  // detach the element before check()'s post-assertion.
  await page.locator(".flow-node.kind-root .col-row:not(.on) input[type=checkbox]").first().click();

  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".toast.ok")).toContainText("Saved");

  // The files on disk are now canonical-regenerated; `flusso check` must accept
  // them (parse + convert + typed mapping). `--offline` keeps the assertion
  // independent of DB seeding. Throws (failing the test) on a non-zero exit.
  execFileSync(flussoBin, ["check", "--config", resolve(projectDir, "flusso.toml"), "--offline"], {
    stdio: "pipe",
  });
});
