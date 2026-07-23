export function add(left: number, right: number): number {
  return left + right;
}

// Named-type signature: exercises the arrow-type payload (param/ret sigs).
// Keyword-only `add` above carries no resolvable name, so it emits no sig.
export function shift(p: Point, d: Dir): Vec2 {
  return new Vec2(p.x, p.y);
}

export interface Point {
  x: number;
  y: number;
}

export type Vec = Point[];

export enum Dir {
  North,
  South,
  East,
  West,
}

export class Vec2 {
  constructor(public x: number, public y: number) {}
  magnitude(): number {
    return Math.sqrt(this.x * this.x + this.y * this.y);
  }
  // method with a named-type param + return.
  scaled(by: Vec): Vec2 {
    return new Vec2(this.x, this.y);
  }
}

export const origin: Point = { x: 0, y: 0 };

export const sub = (a: number, b: number) => a - b;

// arrow with named types.
export const mirror = (p: Point): Point => ({ x: p.x, y: p.y });
