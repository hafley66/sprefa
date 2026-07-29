// C1 boundary: TypeScript has NO type-level object spread syntax.
// EXPECTED: parse refusal.

type AObj = { id: number; name: string };
type BObj = { ...AObj; extra: number };

export type { BObj };
