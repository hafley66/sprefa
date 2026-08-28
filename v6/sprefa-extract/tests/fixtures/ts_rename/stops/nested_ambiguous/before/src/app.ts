function first(): number {
  const Foo = 1;
  return Foo + 1;
}

function second(): number {
  const Foo = 2;
  return Foo * 2;
}

export const total = first() + second();
