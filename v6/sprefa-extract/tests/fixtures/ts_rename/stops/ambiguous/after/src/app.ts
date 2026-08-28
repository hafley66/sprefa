class Bar {
  greet(): string {
    return "hi";
  }
}

function wrap(): number {
  const Foo = 7;
  return Foo + 1;
}

const greeter = new Bar().greet();
const total = greeter.length + wrap();
