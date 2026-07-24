// docs.ts: doc-facet parity fixture. Exercises v5's ts_docs_from: each jsdoc
// block (a slash-star-star comment) is associated with the entity whose anchor
// byte is nearest at/after the block end, with only whitespace between.
// Anchors: top-level fn/class/interface/alias/enum decls (export-wrapped ok),
// class methods (ctor skipped), and arrow-consts. A plain string const is NOT
// an anchor, so the jsdoc above `greeting` is DROPPED (the const statement
// sits between the block and the next anchor). The documented entities
// re-exercise the ported facets (type/call/df/const) on doc-heavy input.
// ASCII-only so oxc byte offsets round-trip cleanly (parity is clean).

/** Adds two numbers. */
export function add(left: number, right: number): number {
  return left + right;
}

/** A 2D point. */
export interface Point {
  x: number;
  y: number;
}

/** A list of points. */
export type Vec = Point[];

/** Cardinal directions. */
export enum Dir {
  North,
  South,
}

/** A mutable 2D vector. */
export class Vec2 {
  constructor(public x: number, public y: number) {}

  /** Length of the vector. */
  magnitude(): number {
    return Math.sqrt(this.x * this.x + this.y * this.y);
  }

  /** Scale by a scalar factor. */
  scaled(by: number): Vec2 {
    return new Vec2(this.x * by, this.y * by);
  }
}

/** DROPPED: a string const has no doc anchor, so this block documents nothing. */
export const greeting = "hi";

/** Mirrors a point across both axes. */
export const mirror = (p: Point): Point => ({ x: p.x, y: p.y });
