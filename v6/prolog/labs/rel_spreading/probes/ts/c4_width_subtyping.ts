// C4 width subtyping: is a WIDER row accepted where a NARROWER one is wanted?
// EXPECTED: TypeScript is structural, so the wider value IS accepted through
// a variable, and only a fresh object literal is caught. This is the hazard
// the nominal draft call avoids.

type Narrow = { id: number };
type Wide = { id: number; extra: number };

declare function takesNarrow(row: Narrow): void;
declare const wide: Wide;

// (1) wider value through a binding: ACCEPTED, no diagnostic
takesNarrow(wide);

// (2) fresh literal with the same excess column: refused by the freshness
// check only, not by the assignability rule
takesNarrow({ id: 1, extra: 2 });

// (3) positional rows are width-checked exactly: a 3-tuple is not a 2-tuple
type NarrowRow = [id: number];
type WideRow = [id: number, extra: number];
declare function takesNarrowRow(row: NarrowRow): void;
declare const wideRow: WideRow;
takesNarrowRow(wideRow);

// (4) and a rel-shaped nominal brand blocks even the equal-width case
type BrandedA = { id: number; readonly __rel: "a" };
type BrandedB = { id: number; readonly __rel: "b" };
declare function takesA(row: BrandedA): void;
declare const branded: BrandedB;
takesA(branded);

export { };
