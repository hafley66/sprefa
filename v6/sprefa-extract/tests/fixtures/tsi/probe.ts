// The issue's reproduction shape: a generic contract, a declaration that
// implements it, a generic method, and an alias whose body is a written call.

export interface Mapper<T> {
  seed: T;
}

export class User<T> implements Mapper<T> {
  readonly id: T;
  name?: string;
  label: string;
  seed: T;

  constructor(id: T, label: string, seed: T) {
    this.id = id;
    this.label = label;
    this.seed = seed;
  }

  map<U>(project: (element: T) => U): U {
    return project(this.id);
  }
}

export type Query = Partial<User<number>>;
