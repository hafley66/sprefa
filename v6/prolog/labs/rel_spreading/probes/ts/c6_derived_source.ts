// C6 spreading a DERIVED source: what does a compiler need in order to splice
// columns out of something whose shape is itself computed?

declare function derive(): [id: number, tag: string];

// (1) TypeScript resolves the derived shape first, then splices. Accepted.
type DerivedRow = ReturnType<typeof derive>;
type SplicedFromDerived = [...DerivedRow, extra: number];
const ok: SplicedFromDerived = [1, "t", 7];

// (2) forward reference in file order: also accepted, the type pass is not
// source-ordered
type SplicedEarly = [...LaterRow, extra: number];
type LaterRow = [id: number, tag: string];
const okEarly: SplicedEarly = [1, "t", 7];

// (3) self reference through the splice: refused as circular
type SelfRow = [...SelfRow, extra: number];

// (4) mutual reference through the splice: refused as circular
type MutualA = [...MutualB, a: number];
type MutualB = [...MutualA, b: number];

export { ok, okEarly };
export type { SelfRow, MutualA, MutualB };
