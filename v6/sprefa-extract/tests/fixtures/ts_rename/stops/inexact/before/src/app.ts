class Foo {}

let value: Foo | null = null;

value = new Foo();

const aliased: Foo | null = value;

const list: Foo[] = [value as Foo];
