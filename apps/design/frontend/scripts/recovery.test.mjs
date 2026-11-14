// The wording the recovery notice and the blocked page pick, per case
// (issue #292). Run with `npm test`: Node strips the types of the imported
// TypeScript, so the model runs as is, with no browser and no bundler.

import assert from "node:assert/strict";
import { test } from "node:test";

import { en } from "../src/locales/en.ts";
import { noticeLines, routeLeads } from "../src/model/recovery.ts";

/// A translator that answers the key, so a test sees which message a case picks.
const t = (key, vars = {}) => {
  assert.ok(key in en, `${key} is in the English catalog`);
  return `${key} ${JSON.stringify(vars)}`;
};
const time = (ms) => `at ${ms}`;
const keys = (lines) => lines.map((line) => line.text.split(" ")[0]);
const empty = { recovered: [], leftovers: [] };
const save = (outcome, files, kept = [], displaced = []) => ({
  id: "1-2",
  outcome,
  started_at_ms: 7,
  files,
  kept,
  displaced,
});

test("a file deleted after a rolled-back save is told as deleted, not as written by another program", () => {
  const recovery = {
    ...empty,
    recovered: [
      save("rolled_back", ["a.yml"], [{ path: "b.yml", backup: ".b.yml.flusso-save.1-2.0.bak", deleted: true }]),
    ],
  };
  const told = keys(noticeLines(t, recovery, time));
  assert.deepEqual(told, ["recovery.rolledBack", "recovery.keptDeletedBackup"]);
  assert.doesNotMatch(en["recovery.keptDeletedBackup"], /written by another program/);
});

test("a file deleted after a completed save is told as deleted", () => {
  const recovery = { ...empty, recovered: [save("completed", ["a.yml"], [{ path: "b.yml", deleted: true }])] };
  assert.deepEqual(keys(noticeLines(t, recovery, time)), ["recovery.completed", "recovery.keptDeletedAfter"]);
});

test("a kept backup is named once, by its file, not again among the leftovers", () => {
  const backup = ".b.yml.flusso-save.1-2.0.bak";
  const recovery = {
    recovered: [save("rolled_back", ["a.yml"], [{ path: "b.yml", backup }])],
    leftovers: [backup, ".old.yml.flusso-save.3-4.0.bak"],
  };
  const lines = noticeLines(t, recovery, time);
  assert.equal(lines.filter((line) => line.text.includes(backup)).length, 1, JSON.stringify(lines));
});

test("a copy whose notice was dismissed still says what it holds", () => {
  const recovery = {
    ...empty,
    copies: [
      { copy: ".b.yml.flusso-save.1-2.0.bak", path: "b.yml", holds: "before", started_at_ms: 7 },
      { copy: ".c.yml.flusso-save.1-2.1.bak", path: "c.yml", holds: "displaced", started_at_ms: 7 },
    ],
  };
  assert.deepEqual(keys(noticeLines(t, recovery, time)), ["recovery.copyBefore", "recovery.copyDisplaced"]);
});

test("a save whose every file was kept still says which save it was", () => {
  const recovery = { ...empty, recovered: [save("completed", [], [{ path: "b.yml" }])] };
  assert.equal(keys(noticeLines(t, recovery, time))[0], "recovery.interrupted");
});

test('the way forward starts with "Or" only after a way back', () => {
  const steps = [{ do: "delete", path: ".flusso-save.journal" }];
  const report = { cause: "no_record", journals: [], files: [], leftovers: [] };
  assert.equal(routeLeads(t, { ...report, forward_route: steps }).forward?.split(" ")[0], "recovery.wayForwardOnly");
  const both = routeLeads(t, { ...report, back_route: steps, forward_route: steps });
  assert.equal(both.forward?.split(" ")[0], "recovery.wayForward");
  assert.match(en["recovery.wayForward"], /^Or, /);
  assert.doesNotMatch(en["recovery.wayForwardOnly"] ?? "", /^Or\b/);
});

test("a sentence that tells to copy over a file says the file may have changed since", () => {
  for (const key of ["recovery.keptBackup", "recovery.keptDeletedBackup", "recovery.copyBefore"]) {
    assert.match(en[key], /changed since/, key);
  }
});

test("each leftover copy is told with the file it is a copy of", () => {
  const recovery = {
    ...empty,
    leftovers: [".users.schema.yml.flusso-save.1-2.0.bak", "schemas/.a.yml.flusso-save.3-4.1.tmp"],
  };
  const lines = noticeLines(t, recovery, time);
  assert.deepEqual(keys(lines), ["recovery.leftover", "recovery.leftover"]);
  assert.match(lines[0].text, /"path":"users\.schema\.yml"/);
  assert.match(lines[1].text, /"path":"schemas\/a\.yml"/);
});

test("a kept file's line is true whether or not an earlier attempt rolled it back", () => {
  for (const key of ["recovery.kept", "recovery.keptBackup", "recovery.keptDeleted", "recovery.keptDeletedBackup"]) {
    assert.doesNotMatch(en[key], /did not roll it back/, key);
  }
});

test("a leftover's line also covers a copy of what another program wrote", () => {
  assert.match(en["recovery.leftover"], /another program/);
});

test("the deleted file's backup line names the file that may have changed", () => {
  assert.match(en["recovery.keptDeletedBackup"], /\{path\} may have changed since/);
});

test("a notice that could not be saved says it is not shown again after the designer stops", () => {
  const unsaved = { ...save("rolled_back", ["a.yml"]), unsaved: true };
  assert.deepEqual(keys(noticeLines(t, { ...empty, recovered: [unsaved] }, time)), [
    "recovery.rolledBack",
    "recovery.unsaved",
  ]);
  assert.deepEqual(keys(noticeLines(t, { ...empty, recovered: [save("rolled_back", ["a.yml"])] }, time)), [
    "recovery.rolledBack",
  ]);
});
