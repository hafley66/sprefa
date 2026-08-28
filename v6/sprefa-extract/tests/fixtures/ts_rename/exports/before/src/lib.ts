export class Foo {
  readonly tag = "foo";
}

export function makeFoo(): Foo {
  return new Foo();
}
