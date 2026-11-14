// The rename chips under a field's name box keep their places whatever name
// the field has (issue #284): applying a name moves no chip along the row.
// Run with `npm test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { renameChips } from "../src/model/renameChips.ts";

const none = () => false;

test("the chips keep one order through a chain of renames", () => {
  const suggestions = ["full_name", "fullName", "name"];
  const rows = ["fullName", "full_name", "x", "name", "fullName"].map((current) =>
    renameChips(suggestions, current, none),
  );
  for (const row of rows)
    assert.deepEqual(
      row.map((chip) => chip.name),
      suggestions,
    );
});

test("the chip holding the current name keeps its slot, marked current", () => {
  const chips = renameChips(["full_name", "fullName"], "fullName", none);
  assert.deepEqual(chips, [
    { name: "full_name", current: false },
    { name: "fullName", current: true },
  ]);
  assert.deepEqual(
    renameChips(["full_name", "fullName"], "x", none).map((chip) => chip.current),
    [false, false],
  );
});

test("duplicates, empty names and refused names are left out; at most three chips", () => {
  const chips = renameChips(["a", "", "a", "taken", "b", "c", "d"], "b", (n) => n === "taken");
  assert.deepEqual(
    chips.map((chip) => chip.name),
    ["a", "b", "c"],
  );
});
