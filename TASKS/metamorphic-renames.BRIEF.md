# metamorphic-renames (issue: metamorphic-renames, size:med)

FIRST ACTION: `git merge --ff-only e23893b2ef8d3e4c5f60f0a98f015b95dea23128`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root. Issue body:
/Users/chrishafley/projects/sprefa/issues/metamorphic-renames/item.md

GOAL: a metamorphic rename pass over the corpus. Law: renaming every
rel/var/module in a program (camelCase, __dunder__, snake, mixed shapes
included) must produce output identical modulo the rename map. Evidence this
finds real bugs: the camelCase module mangling and the __dunder__
silently-dropped interface (both fixed in PR #262) were pure name-sensitivity
defects.

APPROACH (decided, follow it): operate at the fixture TERM level, like
arm_census.pl does (v6/prolog/compile/scripts/arm_census.pl is your structural
template — read it first). For each corpus fixture that the manifest says
compiles: build a rename map over rel names, variable names, and module names;
apply it to the prog term; compile BOTH originals and renamed variants through
the same single-program compile entry the sweep uses; compare emitted
artifacts after applying the inverse map to the renamed output. Any residue
that is not explained by the rename map is a FINDING. Include at least these
name shapes in the map generator: camelCase, __dunder__, trailing_underscore_,
ALLCAPS, and a max-length name. Deterministic seed, printed.

HARD RAIL: your run writes NOTHING under v6/prolog/compile/out/. Compile to a
scratch directory (find how sweep points the compiler at an output dir; if the
entry hardcodes out/, copy compilation inputs to a temp tree and run there —
never point the real out/ at your run). VALIDATION includes `git status` in
the worktree showing ONLY your owned files.

FILES YOU OWN: v6/prolog/compile/scripts/metamorphic_rename.pl + a runner
metamorphic_rename.sh, and plans/2026-08-15-metamorphic-renames.REPORT.md.
FORBIDDEN: every existing .pl (read-only), fixtures, emitters, out/,
conformance/**, v6/tsv2/**, v6/sprefa-engine-rs/**, v6/sprefa-extract/**.

DELIVERABLE: the REPORT lists (1) fixtures swept and skipped with reasons;
(2) per-finding: fixture, the rename that broke it, the artifact diff excerpt,
and the suspected compiler site (grep for the mangling point, cite file:line);
(3) counts: swept, identical-modulo-map, findings. Two full runs with the same
seed produce identical counts — paste both. File each finding with
`issuectl --json new` (bug, label bugmine + area:compiler), one issue per root
cause, smallest fixture cited. Do NOT fix compiler code.

Findings language: a mismatch is a defect hypothesis with a cited site, never
"the language does not support". dl variable names in any snippet you write
are descriptive, never single-letter.

COMMIT plain. Close:
`issuectl --json close metamorphic-renames --commit <sha>:<summary>`.
Report: the three counts, findings list with issue slugs, both run receipts.
