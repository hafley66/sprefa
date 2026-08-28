function first(): number {
  const Foo = 1;
  return Foo + 1;
}

function second(): number {
  const Bar = 2;
  return Bar * 2;
}

export const total = first() + second();
