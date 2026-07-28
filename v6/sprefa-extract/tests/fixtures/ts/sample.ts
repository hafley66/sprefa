export function add(left: number, right: number): number {
  return left + right;
}

// Named-type signature + a nested named fn (exercises the arrow-type sigs AND
// the nested-call-def walker) + call sites (identifier / new / member).
export function shift(p: Point, d: Dir): Vec2 {
  function clamp(n: number): number {
    return n;
  }
  return new Vec2(clamp(p.x), clamp(p.y));
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
