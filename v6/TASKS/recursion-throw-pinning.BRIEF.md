# recursion-throw-pinning (issue: recursion-throw-pinning, size:small)

FIRST ACTION: `git merge --ff-only e23893b2ef8d3e4c5f60f0a98f015b95dea23128`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root. This brief depends on PR #277's census script,
present at that sha: v6/prolog/compile/scripts/arm_census.pl (+ runner sh).

GOAL: the recursion refusal surface in v6/prolog/lower.pl gains corpus
coverage. Census (plans/2026-08-15-lowerpl-arm-census.REPORT.md, "Real
finding" section): five arms unreached by all 448 fixtures —
- lower.pl:5205 throw `recursive_cte_multiple_self_reads`
- lower.pl:5260 throw `built_text_in_recursive_head`
- lower.pl:5264 throw `built_list_in_recursive_head`
- guard arms `recursive_arm_builds_no_string`, `recursive_arm_builds_no_list`

ARCH.pl:952 records why this matters: the direct spelling is refused loudly
while the two-rel spelling silently under-derives (the PR #266 silent-wrong).
Unpinned, the loud refusal can regress into silent wrongness with no test
noticing.

DELIVERABLE: ONE new fixture file
v6/prolog/conformance/fixtures/recursion_refusal_pins.pl containing:
1. Three `throws(unsupported_construct(...))` fixtures, one per throw site,
   each spelling the direct construct (a recursive head read twice in its own
   body; text built in a recursive head; a list built in a recursive head).
   Copy the assertion pattern from fixtures/1_match_block.pl:110
   (match_enum_nonexhaustive_is_refused). READ each throw site in lower.pl
   first to match the exact error term shape and arguments.
2. At least one COMPILING recursive fixture whose lowering passes through both
   guard arms (recursive_arm_builds_no_string / recursive_arm_builds_no_list),
   with expected deltas, modeled on an existing recursive fixture
   (e.g. fixtures/20_parent_chain.pl).
Check how the conformance runner discovers fixture files
(v6/prolog/conformance/go.pl) and register the new file if discovery is not
glob-based.

FILES YOU OWN: the new fixture file only (+ the runner's fixture list line if
registration needs it).
FORBIDDEN: lower.pl and every compiler .pl (read-only), every existing
fixture file, emitters, v6/tsv2/**, v6/sprefa-engine-rs/**,
v6/prolog/compile/scripts/** (run, never edit).

VALIDATION (paste outputs):
1. `cd v6/prolog/conformance && swipl -g go -t halt go.pl` — all PASS
   including your new fixtures, zero regressions (448 -> 448+N, 0 FAIL).
2. Census before/after: run the arm census runner at base sha state and after
   your change; the five rows above flip unreached -> reached. Paste both rows
   sets. Any row that will NOT flip = report why with the throw-site citation,
   do not force it.
3. Run each leg twice; identical numbers both runs.

LAW REMINDERS: a refusal is a hypothesis — if a fixture you write COMPILES
where the report predicts a throw, that is a REAL FINDING: capture the program
+ output, file with `issuectl --json new`, and report it; do not "fix" the
fixture to force a throw. Do not use the word "refusal" in prose you write;
say TODO / not built yet. dl variable names descriptive, never single-letter.

COMMIT plain. Close: `issuectl --json close recursion-throw-pinning --commit <sha>:<summary>`.
Report: fixture names, PASS counts, the five census rows before/after, any
compiles-instead-of-throws findings.
