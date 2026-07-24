// Lambda/closure parity fixture, mined from v5 tests/fixtures/callables/ts.ts
// (the shapes v5 tags "lambda"). Exercises: unbound arrow arguments with
// expression + block bodies, an unbound function-expression argument, nested
// closures (an inline arrow inside an inline arrow), and closures capturing
// an enclosing local. Controls: a const-bound arrow (a callable def, NO
// closure df-node) and a named-callback pass (no lambda minted at all).

// control: const-bound arrow -> function def, NO closure df-node.
export const double = (n: number): number => n * 2;

export function summarize(values: number[]): number {
  // captured by the inline arrows below.
  const factor = 3;
  // unbound arrow, expression body -> closure df-node + lambda call_def (v5).
  const scaled = values.map((value) => value * factor);
  // unbound arrow, block body with an explicit return.
  const positive = scaled.filter((value) => {
    return value > 0;
  });
  // unbound function expression argument -> closure df-node + lambda call_def.
  return positive.reduce(function (acc, value) {
    return acc + value;
  }, 0);
}

// nested closures: an inline arrow whose body maps with another inline arrow.
export function pairs(values: number[]): number[] {
  return values.map((outer) => [outer, outer + 1].map((inner) => inner * 2)).flat();
}

// control: passing a named callback mints no lambda (no closure df-node).
export function doubleAll(values: number[]): number[] {
  return values.map(double);
}
