export function add(left: number, right: number): number {
  return left + right;
}

export interface Point {
  x: number;
  y: number;
}

export const origin: Point = { x: 0, y: 0 };
