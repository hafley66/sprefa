# ADDENDUM (2026-08-03, after recon verdict): laziness rulings

The recon confirmed the system as built is 100% eager and queries are
dead metadata. The user has since ruled, binding for this plan:

1. The language is lazy end to end: NOTHING evaluates until demanded.
   Queries (the single-`?` statement) are the only subscribe roots;
   imports ride the same demand plane.
2. External events are typed clock-world rows; a generic event-source
   decl (beyond interval/watch/sh) is wanted — worked example: a git
   pre-commit hook entering as a typed EDB row.
3. CORRECTED 2026-08-03: the user described share-with-no-reset only
   inside the worked pre-commit example, as that composition's shape.
   It is NOT a ruled default for demanded sources; refcount/reset
   behavior in general is an OPEN fork. Before first demand is also an
   open fork (drop vs buffer vs store materialization) — present both,
   decide neither.
4. A parallel impact-analysis lane is covering laziness-vs-existing-code
   in depth at /Users/chrishafley/projects/sprefa-impact-lazy. Your plan
   should assume demand-driven evaluation as the target semantics and
   focus on scope/module/import structure; defer deep engine-migration
   detail to that lane, but your import-as-demand section must be
   consistent with these rulings.
