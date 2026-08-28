export class Baz {
  readonly tag = "foo";
}

export function makeFoo(): Baz {
  return new Baz();
}
