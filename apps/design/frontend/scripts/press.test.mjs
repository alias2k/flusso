// When a name box applies the name it holds, relative to the pointer press
// that left it (issue #284): never inside the press, so nothing re-renders
// under the pointer before the click. Run with `npm test`: Node strips the
// types of the imported TypeScript.

import assert from "node:assert/strict";
import { test } from "node:test";

import { PRESS_FALLBACK_MS, createPressTracker } from "../src/model/press.ts";

/// A tracker on fake timers, and a name box's `settle` that counts its calls.
function setup() {
  let clock = 0;
  let timers = [];
  const tracker = createPressTracker({
    set: (run, ms) => {
      const timer = { run, at: clock + ms };
      timers.push(timer);
      return timer;
    },
    clear: (timer) => {
      timers = timers.filter((t) => t !== timer);
    },
  });
  const box = { applied: 0 };
  const settle = () => {
    box.applied += 1;
  };
  /// Moves the clock by `ms`, running every timer due by then.
  const advance = (ms) => {
    clock += ms;
    for (;;) {
      const due = timers.filter((t) => t.at <= clock).sort((a, b) => a.at - b.at)[0];
      if (!due) return;
      timers = timers.filter((t) => t !== due);
      due.run();
    }
  };
  const ev = (type, pointer = 1) => tracker.dispatch({ type, pointer });
  return { tracker, box, settle, advance, ev };
}

test("a press that leaves the box applies the name after its click, not before", () => {
  const { tracker, box, settle, advance, ev } = setup();
  ev("down");
  tracker.afterPress(settle);
  ev("up");
  assert.equal(box.applied, 0, "not at pointerup");
  ev("click");
  assert.equal(box.applied, 0, "not inside the click's dispatch");
  advance(0);
  assert.equal(box.applied, 1, "once the click has been dispatched");
});

test("a press no click follows applies the name after the fallback", () => {
  const { tracker, box, settle, advance, ev } = setup();
  ev("down");
  tracker.afterPress(settle);
  ev("up");
  advance(PRESS_FALLBACK_MS - 1);
  assert.equal(box.applied, 0);
  advance(1);
  assert.equal(box.applied, 1);
});

test("a cancelled press (a touch that scrolls) applies the name", () => {
  const { tracker, box, settle, advance, ev } = setup();
  ev("down");
  tracker.afterPress(settle);
  ev("cancel");
  advance(0);
  assert.equal(box.applied, 1);
});

test("the window losing focus mid-press applies the name", () => {
  const { tracker, box, settle, ev } = setup();
  ev("down");
  tracker.afterPress(settle);
  tracker.dispatch({ type: "windowBlur" });
  assert.equal(box.applied, 1);
});

test("a blur with no press going on (Tab, Enter) applies the name at once", () => {
  const { tracker, box, settle } = setup();
  tracker.afterPress(settle);
  assert.equal(box.applied, 1);
});

test("a tap, whose focus moves after pointerup, applies the name after its click", () => {
  const { tracker, box, settle, advance, ev } = setup();
  ev("down");
  ev("up");
  tracker.afterPress(settle);
  assert.equal(box.applied, 0, "the click is still to come");
  ev("click");
  advance(0);
  assert.equal(box.applied, 1);
});

test("with two pointers down, the name waits for the last one", () => {
  const { tracker, box, settle, advance, ev } = setup();
  ev("down", 1);
  ev("down", 2);
  tracker.afterPress(settle);
  ev("up", 1);
  ev("click");
  advance(PRESS_FALLBACK_MS);
  assert.equal(box.applied, 0, "pointer 2 is still down");
  ev("up", 2);
  ev("click");
  advance(0);
  assert.equal(box.applied, 1);
});

test("a second press starting before the first one's name applied carries it to its own end", () => {
  const { tracker, box, settle, advance, ev } = setup();
  ev("down");
  tracker.afterPress(settle);
  ev("up");
  ev("down");
  advance(PRESS_FALLBACK_MS);
  assert.equal(box.applied, 0, "the first press's fallback does not cut into the second");
  ev("up");
  ev("click");
  advance(0);
  assert.equal(box.applied, 1, "applied once");
});

test("Save (or Enter) while a name waits for its press applies it at once, and only once", () => {
  const { tracker, box, settle, advance, ev } = setup();
  ev("down");
  tracker.afterPress(settle);
  tracker.now(settle);
  assert.equal(box.applied, 1);
  ev("up");
  ev("click");
  advance(PRESS_FALLBACK_MS);
  assert.equal(box.applied, 1);
});

test("a box that goes away while its name waits is not settled again by the tracker", () => {
  const { tracker, box, settle, advance, ev } = setup();
  ev("down");
  tracker.afterPress(settle);
  tracker.forget(settle);
  ev("up");
  ev("click");
  advance(PRESS_FALLBACK_MS);
  assert.equal(box.applied, 0);
});

test("a press with no name waiting applies nothing", () => {
  const { box, advance, ev } = setup();
  ev("down");
  ev("up");
  ev("click");
  advance(PRESS_FALLBACK_MS);
  assert.equal(box.applied, 0);
});
