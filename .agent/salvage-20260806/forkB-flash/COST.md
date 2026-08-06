# COST.md: what create-on-write makes worse

One page, one claim: create-on-write trades a forward declaration of shape for convenience, and the price is a silent-catch class plus a collision class. Both are named with their proof-of-harm.

## The specific thing this branch makes worse

Branch A demands a module be declared before any other file adds to it. That declaration is a CHECKPOINT: a misspelled dotted head is an error on contact. Create-on-write removes the checkpoint, so a misspelled dotted HEAD synthesizes a brand-new empty module instead of failing. A rule `car.wheel(x) <- ...` that the author intended as `car.wheel_base(x)` creates `car.wheel` and an empty `car` subtree it was never meant to hold. No error, no warning, the shape just exists. That is the whole meaning of the branch, and it is also the defect class: typos become features.

The second worse class is cross-program catalog id collision. The module table assigns ids positionally per compile and seeds them with `INSERT OR IGNORE`; a served second program sharing one database reuses the same ids and the ignore keeps the first program's rows (measured: `rel_id 6` present as two different rels, `parent_id 6` child walk returning both programs' columns). Create-on-write adds rows per program, so it raises the chance of two programs colliding on the same module table. Branch A has the same backend defect, but branch B increases its surface area by the count of created modules and by the fact that modules no longer come from a declaration any one program owns.

## The case where a user is surprised

Semantic warping: a user renames a module `a.b` to `a.b2` in file one of four, and because file two still spells `a.b(x) <- ...`, the machine keeps a working `a.b` they thought they deleted. Every contributor to `a.b` holds its path alive. The live path is the union of every file's spelling, not what any one file believes. Under branch A the empty stub would be refused for want of a declaration; under B it silently persists and keeps clocking. A user who removes the last declaring file of a module branch A always went through will not get the equivalent "gone" signal here.

## What I would need to see before shipping

1. A lint that flags a dotted head whose leaf is read by NO body reference in the same compile unit, at a warning-before-error severity, so the typo class has a hook (the plumbing exists: analyze already walks every head ref and every body use). Evidence: a fixture corpus where a deliberate misspelling produces the warning, not silence.
2. A served two-program test that boots program A then program B into one database and asserts the two module tables do not interleave. If the server cannot namespace catalog tables per program, the pattern of seeding by `INSERT OR IGNORE` at shared ids must be replaced (content-addressed ids or per-program table names) as a prerequisite, because create-on-write raises the collision odds.
3. A rename-then-recompile fixture asserting the semantic-warp case either produces the lint warning (item 1) or a named refusal, never silent persistence.
