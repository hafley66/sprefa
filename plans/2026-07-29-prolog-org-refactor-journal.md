# v6 prolog organization refactor journal

Contract: `plans/2026-07-29-prolog-org-review.md` (analysis at base
`15a25c08`). Execution base sha: `5a9bfdd8c36acff55a9e683d080bc3ede38d9418`.

Review staleness corrections applied before editing:

| Review claim | Measured at `5a9bfdd8` |
|---|---|
| 46 `.pl` files | 47 (`compile/scripts/dl6_oracle.pl` added by the runtime-bridge merge) |
| 15,028 lines | 15,159 |
| conformance 135/0 | 136/0 |
| plunit 74/74 | 75/75 |
| sweep 73 compiled / 70 identical | unchanged, 73/70/0 |

Every cited line number was re-located by predicate name rather than trusted
as a line reference.

---

## R10: `just prolog-lint` read-only gate

Landed. Files: `v6/prolog/tools/prolog_lint.pl`,
`v6/prolog/tools/prolog-lint.sh`,
`v6/prolog/tools/prolog-lint-baseline.txt`, `v6/justfile` recipe.

The review's section-6 8-step recipe is implemented as three SWI processes,
because two source clusters declare the same module name and a third file set
only exists once loaded:

| Phase | Goal | Steps covered |
|---|---|---|
| sources | `lint_sources` | 1 xref every file, 2 duplicate module names, 8 unused-export advisory |
| compile cluster | `lint_loaded(compile)` | 3 load `compile/test/plunit_tests.pl`, 4 `list_undefined`, 5 `list_cross_module_calls`, 6 `list_redefined` + `list_void_declarations` + `list_trivial_fails` + `list_format_errors` |
| example cluster | `lint_loaded(example)` | 7 fresh process over `examples/ghcacher.pl`, same checks |

`library(check)` reports through `print_message/2`, so findings are captured
with a `user:message_hook/3` that records the structured term and then fails,
leaving the human-readable line intact for interactive runs.

Two normalizations keep the baseline stable across unrelated edits:

- clause references and term positions are dropped; only
  `Module:Name/Arity` survives.
- a PLUnit caller reports as its unit name with the `@line N` suffix
  stripped, so inserting a test above another one does not churn the file.

Two defects found in the first draft of the tool and fixed before commit:

1. `ensure_loaded/1` called from inside the `prolog_lint` module pulled the
   module-free entry files' clauses into `prolog_lint`, which misattributed
   the `examples/ghcacher.pl` caller as `prolog_lint-report/0`. The load is
   now qualified `user:ensure_loaded/1` and the caller reports correctly as
   `user-report/0`.
2. Rendering parse errors through `message_to_text:message_to_codes/2` was
   itself an undefined private cross-module call, which the gate flagged
   against its own author. Parse errors now render via `term_to_atom/2`.

### Measured findings at base

Reproduces the review's numbers exactly:

| Class | Count |
|---|---:|
| duplicate module | 1 (`emit_ts`) |
| private cross-module call | 9 (8 compile cluster, 1 example cluster) |
| undefined predicate | 0 |
| redefinition | 0 |
| void declaration | 0 |
| trivial fail | 0 |
| format error | 0 |
| unused-export candidate (advisory) | 37 excluding the gate's own two entries |

### Ratchet posture

`prolog-lint-baseline.txt` was committed holding the 9 private call sites
only. The `emit_ts` duplicate module was deliberately left OUT of the
baseline, so the gate is RED at the R10 commit. That is the fail-first
receipt the R6 rename needs, since no existing gate in the battery can
detect the collision (the main battery never loads both emitters).

Receipt, at the R10 commit:

```
── NEW findings (gate failure) ──
FINDING	duplicate_module	emit_ts-compile/emit_ts.pl src/emit_ts.pl

PROLOG_LINT findings=10 baseline=9 FAIL
exit 1
```

The ratchet fails in both directions: a new finding fails, and a baseline
entry that disappears also fails with a prune instruction. That keeps the
baseline from rotting while R7 and R8 shrink it.

---

## R6: `emit_ts` module collision

Landed. `v6/prolog/src/emit_ts.pl` declares module `emit_ts_engine_v1`; the
file path is unchanged, and `compile/emit_ts.pl` keeps the plain `emit_ts`
name because it is the live tsv2 backend.

The name says which engine seam the file targets. `src/emit_ts.pl` is the
engine-v1 experiment (prolog terms rendered as literal TypeScript through
`ast.ts` constructor helpers, run by `evalProgramSql`), which `ARCH.pl`
already records as superseded by the tsv2 rows.

### Red-then-green receipt

Loading both emitter files in one SWI process, before the rename:

```
ERROR: -g both: module/2: No permission to redefine module `emit_ts'
       (Already loaded from .../v6/prolog/src/emit_ts.pl)
```

After the rename:

```
BOTH EMITTERS LOADED
```

And the gate installed by R10, which was RED at the previous commit:

```
PROLOG_LINT findings=9 baseline=9 OK
```

### Callers, checked before renaming

The review predicted the importers were the `src/` cluster and the
`examples/ghcacher.pl` chain. Measured, they are neither:

| Caller | Names the module? | Action |
|---|---|---|
| `v6/sprefa-store/bench/engines/swi_emit.sh` | No, `-l <path> -g "emit(...)"` | none needed |
| `v6/sprefa-store/bench/v1-scale-gen.pl` | Yes, 3 qualified calls | 3 qualifiers updated |
| `v6/prolog/examples/ghcacher.pl` | No, imports `kernel`/`checks`/`grader` | none needed |
| `v6/prolog/ARCH.pl` | No, cites file paths as text | none needed |
| `v6/prolog/src/{checks,grader,kernel}.pl` | No | none needed |

Output equality receipts across the rename:

- `bench/v1-scale-gen.pl` `write_program(s2, ...)`: byte-identical.
- `-l v6/prolog/src/emit_ts.pl -g "emit(reach, ...)"`: byte-identical to the
  checked-in `v6/sprefa-store/js/src/gen/reach.gen.ts`.
- `examples/ghcacher.pl` loads clean, `just arch` unchanged.

### OWNERSHIP DEVIATION, disclosed

`v6/sprefa-store/bench/v1-scale-gen.pl` is outside this arc's declared file
ownership (`v6/prolog/**` plus the justfile recipe). Three module qualifiers
in it were changed anyway, because the alternative was landing a rename that
breaks a file in the same commit. The edit is mechanical: `emit_ts:decl_ts`,
`emit_ts:rule_ts`, `emit_ts:used_helpers` gain the new module prefix. Nothing
else in that file or that directory was touched, and its output is proven
byte-identical above.

### R6 side finding, unowned

Those same three call sites are private cross-module calls (`decl_ts/2`,
`rule_ts/3`, `used_helpers/2` are not in the module's export list, which is
`emit/2, emit/3, go/0`). They are a 10th private-call site that the lint gate
cannot see, because `v6/sprefa-store/bench/` is not one of the two clusters
the gate loads. Widening the gate to a bench cluster is a follow-up, not part
of this arc.

---

## R1: shared registry-driven body walker

Landed in two commits, test first. New module `v6/prolog/0_body_walk.pl`,
181 lines of which 84 are the header stating the contract and the deliberate
exclusions.

```prolog
walk_body(+Body, +WalkPolicy, -Events)
% Events = left-to-right list of event(Path, Polarity, Surface, Term)
% WalkPolicy = walk_policy(descend_not(Bool), splice_bare(Bool))
```

Design points that made the consolidation possible:

- Polarity absorbs to `neg` under any descended `not/1` rather than flipping.
  Both consumers that read polarity already behaved that way, so a doubly
  negated atom reads negative on both sides.
- `Surface` is the registry projection, or the atom `plain_atom` when the
  registry has no row. Refusal by absence stays the registry's job, and
  "is a relation atom" becomes "has no registry row".
- A wrapper node is always emitted whether or not the walk descends through
  it. That is what lets one traversal serve both the projections that read
  the wrapper (`latest/1`'s argument is the sampled reference) and those that
  read past it.
- Conjunction is flattened by shape, not by registry row, because `','/2` is
  the body spine and not a construct. Left-nested and right-nested bodies
  produce the same event list.

### Sites consolidated, 10

| Site | Policy | Note |
|---|---|---|
| `engine:body_finalize_ref/2` | not(false), splice(false) | now `body_wrapper_refs/4` |
| `engine:body_latest_ref/2` | not(true), splice(false) | same predicate as the compiler's |
| `engine:body_pre_ref/2` | not(true), splice(false) | same predicate as the compiler's |
| `analyze:level_body_latest_ref/2` | not(true), splice(false) | four predicates, two implementations, now one |
| `analyze:level_body_pre_ref/2` | not(true), splice(false) | |
| `analyze:body_ref_uses/2` | not(true), splice(true) | the reference semantics, now a projection |
| `analyze:conjunction_goals/2` | not(false), splice(true) | splicing is the registry's `splice_bare` role now |
| `analyze:reserved_construct_in_body/2` | not(false), splice(false) | |
| `analyze:body_forbidden_goal/2` | not(false), splice(false) | |
| `engine:trigger_items/2` | not(false), splice(false) | shared spine, local classification |
| `body:body_atoms/2` | not(false), splice(false) | shared spine, local classification |
| `host_expand:body_goals/2` | not(false), splice(false) | the plainest flatten in the tree |

### The one rank-1 site deliberately not taken

`level_eval:goal_rel_refs/3` keeps its own `not/1` recursion. Its clause
appends inner-positive before inner-negative, so for `not((not(a), b))` it
answers `[b/1, a/1]` and not source order. The `not_mixed` golden pins that
empirically. A source-ordered projection would reorder stratification
constraints, so the review's "can project from registry roles" reading does
not survive contact with the actual clause.

### The three drifts stay drifts

`trigger_items/2` and `body_atoms/2` each keep the silent-form list they
always had, and each list is a strict subset of the registry's body rows:
`next/1`, variadic `combine`, `zip/2`, the four reserved lifecycle wrappers
and the six comparison operators are absent from both. So `next(d(4))`,
`combine(e,f)`, `8<9` and `unsubscribe(a(1))` are still classified as arrivals
and as atoms respectively.

They are inert downstream, because `occurrence_trigger/4` unifies an arrival
against a real stored row and none of those shapes can match one. Projecting
them from the registry would reclassify them, which is a semantics change owed
a fixture, not something to slip in under a refactor. Both sites now say so in
place.

### The trap, hit and fixed mid-refactor

The first draft collected uses with `findall/3`. It copies its template, which
severed every `use/4`'s `Args` from the body's own variables, and 27 plunit
tests went red at once. Both collectors are recursive now. `engine.pl` already
carried this exact warning about trigger items, which is what named the cause
in one read.

### Receipts

| Gate | Result |
|---|---|
| conformance | 136 pass / 0 fail |
| plunit | 90 / 90 (75 before the characterization test) |
| roundtrip | ALL GRADES PASS |
| TEXT_DOOR | compiled=73 byte_identical=73 failures=0 |
| sweep | 136/73 compiled, RUN 73 identical=70 wrong=0, FINAL 70 identical |
| sweep, naive referee | identical counts under `SPREFA_TSV2_EMITTER_MODE=naive` |
| prolog-lint | findings=10 baseline=10 OK |

The strongest is the sweep leaving a CLEAN git diff: regenerating every
emitted TypeScript module and every tick log after the refactor produced
byte-identical artifacts.

Sabotage receipt for the characterization test itself: deleting the `not/1`
descent clause from `engine:body_latest_ref/2` turns two tests red, while
conformance stays 136/0 under the same sabotage. The test covers ground the
existing battery did not.

### Environment note

The worktree had no `node_modules` under `v6/tsv2` or `v6/sprefa-store/js`, so
the sweep's diff stage could not start. `pnpm install` in both (the packages
declare `packageManager: pnpm` and a `link:` dependency npm refuses) fixed it.
No lockfile changed.

---

## R2: shared cross-plane program-check module

Landed in two commits, test first. New module
`v6/prolog/0_program_check.pl`, 156 lines.

```prolog
program_violation(+CheckName, +Program, -Payload)
first_violation(+Program, +OrderedChecks, -violation(Name, Payload))
```

The shared part is the TRIGGER CONDITION only. Three things stayed with each
door, all of them fixture-visible data:

| Kept per door | Why |
|---|---|
| exception terms | the oracle throws bare terms, the compiler wraps in `unsupported_construct/1` |
| check ORDER | a program violating two classes reports a different one at each door |
| compiler capability refusals | the oracle is deliberately the wider language |

Order in particular could not be a single shared list: the compiler
INTERLEAVES the shared classes with its own per-rule checks (edge body shape,
head arithmetic, conflict risk). It calls `shared_refusal/2` twice, in the two
positions where the four separate `forall/2` goals it replaced used to sit.

### Six mirrored classes, one implementation each

`keyed_level_head`, `keyed_log_rel`, `log_on_level_headed_rel`,
`keep_on_non_log_rel`, `latest_in_level_rule`, `pre_in_level_rule`, plus
`finalize_in_level_rule` which the oracle checked directly and the compiler
still reaches through its generic refused-goal path.

The `keyed_log_rel` payload is the one that differs by design: the shared
trigger yields `Ref-Positions`, the oracle's adapter drops the positions, the
compiler's keeps them. `keyed_log_rel_payloads_differ_by_design` pins both.

### Two engine-only holes CLOSED, both fail-first

| Hole | Compiler before | Compiler after |
|---|---|---|
| `missing_retention` | accepted | `unsupported_construct(missing_retention(Ref))` |
| `aggregate_in_edge_head` | accepted | `unsupported_construct(aggregate_in_edge_head(Ref))` |

Red-before receipts are recorded in the test unit header. The retention hole
was not hypothetical: `engine_core.pl:log_without_retention_rejected` sat in
the sweep manifest's `compiled` bucket with an empty reason, against an oracle
that throws `missing_retention(event/1)`. The compiler was emitting a
TypeScript module for a program the reference door rejects.

The compiler names the aggregate hole WITH the offending head reference where
the oracle's term is bare `aggregate_in_edge_head`. A compiler refusal has to
say which rule to edit; the oracle's bare term is what fixtures already pin,
so it is unchanged.

### Deliberate count movement, authorized by the brief's diagnostic clause

| Metric | Before | After | Cause |
|---|---:|---:|---|
| conformance | 136 | 137 | new fixture `aggregate_in_edge_head_rejected` |
| sweep total | 136 | 137 | same |
| sweep compiled | 73 | 72 | `log_without_retention_rejected` stops being fake-compiled |
| sweep unsupported | 63 | 65 | that fixture plus the new one |
| RUN identical | 70 | 70 | unchanged |
| RUN wrong | 0 | 0 | unchanged |
| RUN no_oracle_log | 1 | 0 | the fake compile was the only one |
| FINAL no_oracle_final | 1 | 0 | same |
| TEXT_DOOR | 73/73/0 | 72/72/0 | one fewer compiled fixture to check |

`identical` and `wrong` are the quality metrics and both held. The `compiled`
drop is a fixture leaving a bucket it never belonged in.

### Stale artifact removed

`v6/tsv2/gen_emitted/log_without_retention_rejected.ts` survived the sweep,
because the sweep only removes the fixture module it rewrites and this fixture
no longer compiles. Nothing imported it. Deleted; the import gate stays green
at 3 gen / 8 runtime / 7 serve. This is the checked-in-stale-gen-module class
the ledger already records against `door-handwritten.ts`.

### Receipts

| Gate | Result |
|---|---|
| conformance | 137 pass / 0 fail |
| plunit | 103 / 103 (90 before this rank's 13 tests) |
| roundtrip | ALL GRADES PASS |
| TEXT_DOOR | compiled=72 byte_identical=72 failures=0 |
| sweep | 137/72 compiled, RUN identical=70 wrong=0, FINAL identical=70 |
| sweep, naive referee | identical counts |
| import gate | OK |
| prolog-lint | findings=10 baseline=10 OK |

### Finding banked, not acted on

`finalize_in_level_rule` is refused by both doors but named differently: the
oracle says `finalize_in_level_rule(gone/1)`, the compiler says
`level_body_goal(out(Item), finalize(gone(Item)))` with the rule's own
variable shared between head and goal. The shared trigger exists and the
oracle uses it; routing the compiler onto it would change a fixture-visible
diagnostic for no correctness gain, so the drift is pinned by
`finalize_in_level_rule_diagnostics_drift` instead of repaired.

Related asymmetry, also pinned rather than fixed: neither door's finalize scan
descends `not/1`, so `not(finalize(...))` in a level rule is ACCEPTED by both.
`nested_not_finalize_is_opaque_to_both_doors` records it, so closing one side
alone becomes a visible change rather than a silent divergence.
