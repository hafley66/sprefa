// Expected: every closure-caller edge (a call whose covering def is a Lambda)
// gets ONE mirror edge onto the innermost NAMED enclosing def, the walk
// `oracle_ts.mjs` `enclosingName` does; nested anonymous arrows mirror to
// `outer`, never to an outer arrow, and a module-level arrow mirrors to
// `<module>`.
// Observed at HEAD: the closure rows are the only rows; no mirror exists.

export function helper(x: number): number {
  return x;
}

export function wrap(): number {
  return 1;
}

export function run(fn: () => void): void {
  fn();
}

export function outer(): void {
  run(() => {
    helper(1);
    run(() => {
      wrap();
    });
  });
}

run(() => {
  helper(2);
});
