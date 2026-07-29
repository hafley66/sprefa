// C1 decl spread: is there a compile-time COLUMN splice in TypeScript?
// Positional splice via variadic tuple types is the closest analog to the
// ordered column list of a rel declaration. EXPECTED: accepts, width is
// spliced, source order preserved.

type ARow = [id: number, name: string];
type BRow = [...ARow, extra: number];

const bOk: BRow = [1, "n", 7];

// order is positional and preserved: element 0 is A's first column
const firstIsNumber: number = bOk[0];
const secondIsString: string = bOk[1];
const thirdIsNumber: number = bOk[2];

// named splice has no type-level spread; the mechanisms are intersection
// (type level) and object spread (value level)
type AObj = { id: number; name: string };
type BObjIntersect = AObj & { extra: number };
declare const a: AObj;
const bValue = { ...a, extra: 7 };
const bValueCheck: { id: number; name: string; extra: number } = bValue;
declare const bIntersect: BObjIntersect;
const bIntersectCheck: { id: number; name: string; extra: number } = bIntersect;

export { bOk, firstIsNumber, secondIsString, thirdIsNumber, bValueCheck, bIntersectCheck };
