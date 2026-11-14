// The rename chips under a field's name box, in a fixed order.
//
// A chip's place depends only on the suggestions and the sibling names, never
// on the field's current name: the chip holding the current name is kept in
// its place, marked `current` (the row hides it). So applying a typed name,
// which happens on the press that leaves the box, moves no chip along the row,
// and that press lands on the chip it was aimed at.
//
// ```ts
// renameChips(["full_name", "fullName"], "fullName", []);
// // [{ name: "full_name", current: false }, { name: "fullName", current: true }]
// ```

/// One chip: a suggested name, and whether the field holds it now.
export interface RenameChip {
  name: string;
  current: boolean;
}

/// The first three distinct, non-empty `suggestions` that `refused` lets
/// through, in order, each marked when it is `current`.
export function renameChips(suggestions: string[], current: string, refused: (name: string) => boolean): RenameChip[] {
  return suggestions
    .filter((n, i, a) => n && a.indexOf(n) === i && !refused(n))
    .slice(0, 3)
    .map((name) => ({ name, current: name === current }));
}
