export function add(left: number, right: number): number {
  return left + right;
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
}

export const origin: Point = { x: 0, y: 0 };

export const sub = (a: number, b: number) => a - b;
