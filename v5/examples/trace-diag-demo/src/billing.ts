// Tiny demo corpus for examples/trace-diag-demo/trace-diag.dl.
// The trailing marker comment on the call line below is the thing a user
// "aims" at, per plans/2026-07-28-lsp-trace-diag-feasibility.md.

export function computeTotal(items: number[]): number {
  return items.reduce((runningSum, item) => runningSum + item, 0);
}

export function main(): void {
  const items = [1, 2, 3];
  const total = computeTotal(items); // @trace
  console.log(total);
}
