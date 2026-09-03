// Every shape the ts syntax tier claims, in one file: a class with an
// application, a tuple, a literal and a function-typed field; an interface
// with an optional and a readonly member; a union alias and an applied alias;
// a typed const and a typed let; a function with keyword types.

export class Error {}

export class Trail<T> {
  steps: Map<string, Option<T>>;
  label: [string, number];
  failed: { reason: string; code?: number };
  project: (element: T) => U;
  outcome: Result<number, Error>;
}

export interface Render {
  readonly id: string;
  name?: string;
  render(into: Formatter): boolean;
}

export type Step = "idle" | "retry" | "failed";

export type Query = Partial<Trail<number>>;

export const RETRY_LIMIT: number = 3;

export let banner: string;

export function isEmpty(steps: string[], flag: boolean): void {}
