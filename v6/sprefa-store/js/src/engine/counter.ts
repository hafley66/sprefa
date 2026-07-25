/**
 * counter.ts — the SQL statement tripwire, on its own so the runner can count without
 * importing engine.ts (which would cycle once engine.ts runs its statements through
 * the runner). engine.ts re-exports it, so every existing call site is unchanged.
 */

/**
 * Global, resettable count of SQL statements issued. The N+1 tripwire (a golden test
 * resets it, runs a batch, asserts the count is O(N/CHUNK), never O(N)). Process-global.
 */
export namespace stmt_counter {
  let count = 0;
  export function incr(): void {
    count++;
  }
  export function get(): number {
    return count;
  }
  export function reset(): void {
    count = 0;
  }
}
