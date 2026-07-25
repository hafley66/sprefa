// scip-ratchet fixture (4c-ii): one of TWO corpus defs named `helper`, so the
// v5-shaped name-match is AMBIGUOUS corpus-wide and binds nothing. scip (the
// real compiler) resolves gamma.ts's call HERE through the import.
export function helper(): number {
  return 1;
}
