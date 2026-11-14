// When a name box applies its name: never inside the pointer press that left
// the box.
//
// A press (pointerdown … pointerup, then the click) that moves the focus out
// of a name box blurs it at pointerdown. Applying the name right then
// re-renders the page while the press is still going: a control that moves,
// or is re-created because its key changed, is no longer the one under the
// pointer at pointerup, and the browser sends no click. So a blur during a
// press only marks the name; it is applied once the press is over: after the
// click has run its handlers, or, when no click comes (a drag, a cancelled
// touch, a release outside the window), on `pointercancel`, a window blur, or a
// short fallback after the last pointerup. A press counts until its click (or
// the fallback), not only until pointerup: on a touch screen the focus moves at
// the mouse events a tap sends after pointerup. A blur with no press going on
// (Tab, a programmatic focus) applies at once.
//
// This module is the pure state machine; `components/nameDraft.ts` feeds it the
// document's events and runs the effects it returns.
//
// ```ts
// let s = idle;
// [s] = press(s, { type: "down", pointer: 1 });
// const [s2, effect] = press(s, { type: "blur" }); // effect === "none": deferred
// press(press(s2, { type: "up", pointer: 1 })[0], { type: "click" })[1]; // "settleAfterDispatch"
// ```

/// The pointers held down; whether a blur during the press is waiting for it to
/// end; and `released`: every pointer is up and the click may still come.
export interface PressState {
  pointers: readonly number[];
  deferred: boolean;
  released: boolean;
}

export type PressEvent =
  | { type: "down"; pointer: number }
  | { type: "up"; pointer: number }
  | { type: "cancel"; pointer: number }
  | { type: "click" }
  | { type: "blur" }
  | { type: "fallback" }
  | { type: "windowBlur" };

/// What the driver does: nothing; apply the waiting names now; apply them once
/// the current event has finished dispatching (after the click's handlers);
/// or start the fallback timer that ends a press no click follows.
export type PressEffect = "none" | "settleNow" | "settleAfterDispatch" | "armFallback";

export const idle: PressState = { pointers: [], deferred: false, released: false };

/// The next state and the effect for one event.
export function press(state: PressState, event: PressEvent): [PressState, PressEffect] {
  switch (event.type) {
    case "down":
      return [
        { ...state, pointers: [...state.pointers.filter((p) => p !== event.pointer), event.pointer], released: false },
        "none",
      ];
    case "blur":
      if (state.pointers.length > 0 || state.released) return [{ ...state, deferred: true }, "none"];
      return [state, "settleNow"];
    case "up": {
      const pointers = state.pointers.filter((p) => p !== event.pointer);
      if (pointers.length > 0) return [{ ...state, pointers }, "none"];
      return [{ pointers, deferred: state.deferred, released: true }, "armFallback"];
    }
    case "cancel": {
      const pointers = state.pointers.filter((p) => p !== event.pointer);
      if (pointers.length > 0) return [{ ...state, pointers }, "none"];
      return state.deferred ? [idle, "settleAfterDispatch"] : [idle, "none"];
    }
    case "click":
      if (state.pointers.length > 0) return [state, "none"];
      return state.deferred ? [idle, "settleAfterDispatch"] : [idle, "none"];
    case "fallback":
      if (state.pointers.length > 0 || !state.released) return [state, "none"];
      return [idle, state.deferred ? "settleNow" : "none"];
    case "windowBlur":
      return state.deferred ? [idle, "settleNow"] : [idle, "none"];
  }
}
