// C2 column collision: two sources that share a column name.
// EXPECTED (the hazard the draft call cites): silent last-wins on object
// spread, and duplicate positional labels accepted with no diagnostic.

type AObj = { shared: number; onlyA: string };
type BObj = { shared: string; onlyB: boolean };

declare const a: AObj;
declare const b: BObj;

// value-level spread of both: no error, `shared` silently takes B's type
const merged = { ...a, ...b };
const sharedIsString: string = merged.shared;

// order flipped: `shared` silently takes A's type instead. Same program text
// shape, different result type, no diagnostic either way.
const mergedFlipped = { ...b, ...a };
const sharedIsNumber: number = mergedFlipped.shared;

// type level with intersection instead: the collision does NOT error at the
// declaration, it produces `number & string` = never, discovered later
type Intersected = AObj & BObj;
declare const i: Intersected;
const sharedIsNever: never = i.shared;

// duplicate positional LABELS in a variadic tuple splice: accepted
type ARow = [shared: number, onlyA: string];
type BRow = [shared: string, onlyB: boolean];
type Spliced = [...ARow, ...BRow];
const spliced: Spliced = [1, "a", "s", true];

export { merged, sharedIsString, mergedFlipped, sharedIsNumber, sharedIsNever, spliced };
