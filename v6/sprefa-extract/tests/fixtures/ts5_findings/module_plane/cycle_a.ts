// A re-export cycle: each file stars the other, and neither declares `missing`.
// ResolveExport's visited set is what stops the walk.
export * from "./cycle_b.js";

export function fromA(): number {
    return 1;
}
