# Relation-pattern adversarial review

Date: 2026-07-30

Branch: `codex/relpattern-review`

Reviewed HEAD: `609066ee0f5f9b5e64837371680092134c11c20f`

Feature commits: `4b0bc279`, `472320f4`, merge `2e2b983b`

Comparison commit: `68d1ca3f`

## Verdict

**Partly both.**

The reference engine needed one semantic repair: a relation value constructed by a
rule had to become the same canonical `obj(...)` value that an arrival produces.
Before the repair, even depth-1 construction stored a Prolog compound and disagreed
with emitted JSON. The failure is recorded at
`v6/prolog/conformance/fixtures/6_relation_depth.pl:41-73`.

The nested term compiler pass is sugar. A program at `68d1ca3f` can build the same
reference chain by staging one relation level per rule and using the already
existing whole-atom identity projection. The old compiler accepts that program and
emits reference ids and canonical boundary rendering. The old oracle still prints
compound terms, so the semantic repair on the oracle door remains necessary.

Nested reads are also sugar. `decode/2` already lowered recursively before this
feature. Its pre-feature implementation is visible in
`v6/prolog/compile/lower.pl` at commit `68d1ca3f`, then-current lines 886-1047.
The new positional pattern:

```prolog
found(Path) <- located(span(file(_, fpath(Path)), _, _), _).
```

can be written with old constructs:

```prolog
found(Path) <-
    located(Span, _),
    decode(Span, {file: File}),
    decode(File, {at: At}),
    decode(At, {name: Path}).
```

The feature therefore contains:

| Part | Classification | Evidence |
|---|---|---|
| Canonicalize rule-built relation values in the oracle | needed | `v6/prolog/0_relation_pattern.pl:17-34`; pre-feature oracle output shown below |
| Construct an arbitrary-depth value in one nested term | sugar | staged pre-feature program compiles successfully |
| Read an arbitrary-depth value in one positional term | sugar | recursive `decode/2` already existed |
| Reject malformed concrete terms | useful guard, incomplete | variable-bound malformed values bypass it |
| Support nested terms under negation and in edge rules | unfinished compiler work | oracle cases execute; compiler refuses |

## 1. Necessity and the pre-feature alternative

### Concrete program

The concrete requirement is a flat `rawk/5` arrival becoming a canonical nested
`located/2` value:

```prolog
located(
    span(
        file(repo(RepoName), fpath(PathName)),
        Start,
        End),
    Kind) <-
rawk(RepoName, PathName, Start, End, Kind).
```

Its output contract includes:

```prolog
located(
  obj([
    end-20,
    file-obj([
      at-obj([name-'src/a.rs']),
      repo-obj([name-acme])
    ]),
    start-10
  ]),
  def)
```

The committed fixture is
`v6/prolog/conformance/fixtures/6_relation_depth.pl:285-319`.

### Same construction without arbitrary-depth terms

This version uses only mechanisms present at `68d1ca3f`. Each head projects the
identity of one complete body atom. No head contains more than one new reference
level.

```prolog
repo(RepoName) <-
    rawk(RepoName, _, _, _, _).

fpath(PathName) <-
    rawk(_, PathName, _, _, _).

file(repo(RepoName), fpath(PathName)) <-
    rawk(RepoName, PathName, _, _, _),
    repo(RepoName),
    fpath(PathName).

span(file(Repo, At), Start, End) <-
    rawk(_, _, Start, End, _),
    file(Repo, At).

located(span(File, Start, End), Kind) <-
    rawk(_, _, Start, End, Kind),
    span(File, Start, End).
```

The old mechanism is documented at the pre-feature
`v6/prolog/compile/lower.pl:336-350`: a complete body relation atom is bound to
its hidden `__id`, and the same compound in a head projects that id.

Executed against an archive of `68d1ca3f`:

```text
compile_fixture(staged_relation_value_without_nested_term, ...)
exit 0
wrote .../staged.ts
```

The emitted module contains:

```text
file head:    SELECT b0."__id", b1."__id"
span head:    SELECT b0."__id", ...
located head: SELECT b0."__id", ...
```

and boundary reads through `__ref_repo`, `__ref_fpath`, `__ref_file`, and
`__ref_span`.

The same staged program on the old oracle returned:

```text
[located(span(file(repo(acme),fpath('src/a.rs')),10,20),def)]
```

That result isolates the needed part. The old compiler already supports staged
construction. The old oracle does not canonicalize any rule-built relation value.
A smaller repair could canonicalize the oracle door and leave arbitrary-depth term
construction as optional syntax.

### Runnable check

The old tree can be created without a Git worktree:

```bash
scratch="$(mktemp -d /tmp/relpattern-pre.XXXXXX)"
git archive 68d1ca3f | tar -x -C "$scratch"
SPREFA_CONFIG=/nonexistent/x.toml \
DL_NO_DAEMON=1 \
TSV2_PORT=0 \
TSV2_DB="$scratch/check.db" \
swipl -q -l "$scratch/v6/prolog/compile/compile.pl" \
  -g "compile_fixture(staged_relation_value_without_nested_term, 'staged_relation_value.pl', '$scratch/staged.ts')" \
  -g halt
```

The fixture body for this check is the staged program above with the declarations
from `v6/prolog/conformance/fixtures/6_relation_depth.pl:285-299`.

## 2. Orthogonality to named arguments

Named arguments resolve columns of one relation call. The parser:

1. looks up that relation's immediate column order,
2. places named values into those positions,
3. fills positional values into remaining positions,
4. gives omitted body positions fresh variables,
5. refuses omitted head positions.

Receipts:

- `v6/prolog/compile/parse_dl.pl:862-895`
- `v6/prolog/compile/parse_dl.pl:917-941`
- `v6/prolog/compile/test/plunit_tests.pl:1258-1275`

Named arguments can replace this:

```prolog
seen(Start) <- span(file: _, start: Start).
```

for a query that treats `file` as opaque. They cannot replace this:

```prolog
seen(Path) <- span(file: file(at: fpath(name: Path))).
```

The second call crosses `span.file`, `file.at`, and `fpath.name`. Named argument
resolution never changes relation depth and never performs a dictionary join.

The old equivalent requires another construct:

```prolog
seen(Path) <-
    span(file: File),
    decode(File, {at: At}),
    decode(At, {name: Path}).
```

No kwargs-only case covers nested construction or nested dereference. Named
arguments and this feature compose, but they operate on different axes:
same-call slot selection versus cross-reference traversal.

## 3. Burrs

Fourteen burrs were found.

### B1. A variable bypasses the new malformed-value refusal

`relation_argument_violation/6` immediately requires `nonvar(Value)` at
`v6/prolog/0_program_check.pl:275-286`. This program therefore passes the shared
check:

```prolog
span(File, Start, End) <- raw3(File, Start, End).
```

with `span.file: file` and `raw3.file: text`.

Executed results:

```text
oracle:   span('src/a.rs',10,20)
compiler: exit 0, emitted module written
```

The emitted SQL is:

```sql
INSERT OR IGNORE INTO "span" ("file", "start", "end")
SELECT DISTINCT d0."file", d0."start", d0."end"
FROM "__frontier_raw3" d0
```

The destination is declared `INTEGER NOT NULL` by
`v6/prolog/compile/lower.pl:757-763`, while boundary rendering treats it as a
dictionary id through `dictionary_render_expr/3` at
`v6/prolog/compile/lower.pl:810-816`. SQLite can store text in an
integer-affinity column, so this remains a silent value disagreement.

The three new refusal fixtures use concrete values and do not exercise variable
flow: `v6/prolog/conformance/fixtures/6_relation_depth.pl:415-469`.

### B2. The claimed canonical value is refused in rule position

`v6/prolog/0_type_plane.pl:108-118` says the stored value is canonical
`obj(...)`. `v6/prolog/0_relation_pattern.pl:17-33` says there is one value
spelling and calls that spelling `obj(SortedPairs)`.

This body is nevertheless refused:

```prolog
seen(Start) <-
    span(obj([name-'src/a.rs']), Start, _).
```

Executed result:

```text
relation_pattern_not_a_relation_value(span/3,file,file,obj([name-'src/a.rs']))
```

The implementation supports one surface constructor spelling and one stored
spelling, while the comments state one spelling.

### B3. Negated relation patterns are valid oracle programs with missing compiler lowering

The compiler skips every registered body construct at
`v6/prolog/compile/lower.pl:1043-1050`, then detects the surviving pattern at
`v6/prolog/compile/lower.pl:1104-1136`.

The oracle explicitly descends `not/1` at
`v6/prolog/0_relation_pattern.pl:92-95`.

Executed case:

```prolog
seen(Start) <-
    raw(_, _, Start, _),
    not(span(file(_, fpath('missing.rs')), Start, _)).
```

Results:

```text
oracle:   seen(10)
compiler: relation_pattern_not_lowerable(span/3,file,file,...)
```

This is a compiler capability gap.

### B4. Relation values in edge rules are valid oracle programs with missing compiler lowering

The compiler refuses them at `v6/prolog/compile/lower.pl:1006-1024`.
The oracle has an edge-rule expansion clause at
`v6/prolog/0_relation_pattern.pl:71-73`.

A live oracle case used:

- `span/3` declared as `log keep(all)`,
- a canonical `file/2` row in initial state,
- a `raw/4` arrival,
- the nested value in the edge head.

It returned a canonical `span(obj(...),10,20)` row. The compiler returned
`relation_value_in_edge_rule`.

This is a compiler capability gap.

### B5. The committed edge-refusal unit does not demonstrate its oracle claim

The unit builds a `plan(...)` directly at
`v6/prolog/compile/test/plunit_tests.pl:2550-2572` and calls `lower_program/2`.
It bypasses normal program preparation and oracle execution.

Normal expansion adds `latest(file(...))` to the edge body:
`v6/prolog/0_relation_edge_expand.pl:38-42`. The exact unit program has no file
membership row, so the oracle derives no span. If membership is supplied, the
unit's undeclared-key Set head reaches `edge_into_unkeyed_set/1` at
`v6/prolog/conformance/engine.pl:336-354`.

The statement at `v6/prolog/compile/test/plunit_tests.pl:2542-2546` that the
reference engine executes the exact cases is therefore untested and false for
the exact edge program.

### B6. The memo comment overstates correctness impact

`v6/prolog/compile/lower.pl:989-992` says duplicate occurrences would join the
same table twice with nothing relating them.

Forcing the memo lookup to fail produced five joins per arm instead of three,
but the duplicate repo and fpath aliases remained constrained by the same raw
names and parent endpoints. The generated SQL retained the same result set. This
matches the committed sabotage receipt at
`v6/tsv2/tests/relationDepth.test.ts:45-50`.

The memo is a plan-size and query-work optimization for the tested program, not
a correctness requirement.

### B7. The depth-3 construction test does not assert its stated join-count rule

The header says every statement names exactly one dictionary view per level at
`v6/tsv2/tests/relationDepth.test.ts:18-22`.

The depth-3 construction test at
`v6/tsv2/tests/relationDepth.test.ts:215-228` checks:

- distinct view names,
- absence of the old `json_extract`,
- no scans,
- at least four searches.

It does not call `dictionaryJoinsPerArm`.

Independent sabotage counts:

```text
baseline located insert:  4 joins per arm
memo disabled:             7 joins per arm
distinct view set:         unchanged
```

Those assertions can remain green with repeated indexed joins.

### B8. Three statement families have no plan assertion

The test itself records this at
`v6/tsv2/tests/relationDepth.test.ts:52-56`: `supportSql`, `recomputeSql`, and
the naive-mode arm are checked only for row equality. A change that keeps rows
equal while adding scans or duplicate joins is invisible there.

### B9. The two compiler-only refusals are absent from the graded fixture corpus

The only checks are the three Prolog units at
`v6/prolog/compile/test/plunit_tests.pl:2574-2612`. The file explicitly declines
to add fixtures at `v6/prolog/compile/test/plunit_tests.pl:2540-2546`.

Consequences:

- the conformance suite never runs either compiler-only refusal,
- the compile and replay sweep never buckets either program,
- oracle behavior for those exact programs is not compared with compiler
  behavior.

### B10. Oracle expansion branches added by the feature are not covered by fixtures

`v6/prolog/0_relation_pattern.pl:68-108` adds separate handling for:

- level heads,
- edge heads,
- negation,
- splice forms,
- `latest/1`,
- `pre/1`,
- `finalize/1`.

The 11 new fixtures contain plain level heads and bodies only. Repository search:

```bash
rg -n '<\+.*(file|span|repo)|not\([^\n]*(file|span|repo)' \
  v6/prolog/conformance/fixtures v6/prolog/compile/test
```

finds the negation and edge cases only in the compiler unit file. No fixture
executes the new oracle branches.

### B11. Surface traversal is implemented three times with different policies

The oracle rewriter has its own recursive traversal and hardcoded wrapper list at
`v6/prolog/0_relation_pattern.pl:76-108`.

The shared malformed-value checker uses `walk_body/3` at
`v6/prolog/0_program_check.pl:251-270`.

The compiler residue check uses `walk_body/3` with another policy at
`v6/prolog/compile/lower.pl:1116-1136`.

The new oracle phase is also deliberately outside the shared expansion table:
`v6/prolog/0_relation_pattern.pl:36-42`. The different policies are observable
as the negation, edge, splice, and wrapper asymmetries above.

### B12. The argument-length fallback is unreachable for checked plans

`rewrite_relation_arguments/6` has a fallback at
`v6/prolog/compile/lower.pl:1076-1079` for a column-type list whose length does
not match the argument list.

For ordinary atoms, `RelPlans` is constructed from the same relation arity and
column declarations at `v6/prolog/compile/compile.pl:141-148`. For nested values,
`relation_value_shape/3` checks exact declared arity at
`v6/prolog/0_type_plane.pl:119-124`.

No checked program can reach the documented mismatch case. The branch hides an
internal invariant violation if a malformed plan is supplied directly.

### B13. The depth-2 sharing fixture does not observe physical sharing

`relation_depth2_many_rows_share_one_leaf` claims that per-parent copies would
produce two repo rows at
`v6/prolog/conformance/fixtures/6_relation_depth.pl:174-177`.

The emitted target table has a content `UNIQUE` constraint at
`v6/prolog/compile/lower.pl:725-736`, and the oracle's Set relation also dedups
equal public rows. The fixture inspects public `repo/1` rows and final derived
rows only at `v6/prolog/conformance/fixtures/6_relation_depth.pl:200-209`.

It cannot distinguish one interned child id from duplicate internal work that
converges on the same content row.

### B14. The depth-3 many-row fixture has an oracle-vacuous feature leg

The file records that `relation_depth3_many_rows` already passed the old oracle
because its expectations name no relation-valued column:
`v6/prolog/conformance/fixtures/6_relation_depth.pl:15-18`.

Its expectations at `v6/prolog/conformance/fixtures/6_relation_depth.pl:397-401`
check repo, fpath, and scalar `found/2`, while
`relation_depth3_construct_and_read` already checks the nested construction and
read path. Its oracle leg cannot detect the rule-built value representation
defect.

## 4. Memoization and corpus visibility

### Independent confirmation

The memo lookup in a scratch archive of HEAD was forced to fail. No tracked file
was edited.

Compiler output counts:

| Statement | Baseline | Memo disabled |
|---|---:|---:|
| depth-2 `span` insert, each delta arm | 3 | 5 |
| depth-3 `located` insert, each delta arm | 4 | 7 |

The generated depth-2 SQL changed from one each of `__ref_repo`,
`__ref_fpath`, and `__ref_file` to a second repo and fpath alias. All aliases
remained joined by equality predicates.

Hermetic full checks on the sabotaged scratch archive:

```text
Prolog conformance: 186 fixtures passed
compile sweep:       total=186 compiled=131 unsupported=55 crash=0
```

Environment for every run:

```bash
SPREFA_CONFIG=/nonexistent/x.toml
DL_NO_DAEMON=1
TSV2_PORT=0
TSV2_DB=/private/tmp/relpattern-review.../scratch.db
```

The TypeScript replay leg was not independently reproduced. This review
worktree has no `node_modules`; `pnpm install --offline --frozen-lockfile`
reported missing locked tarballs, including `@neon-rs/load-0.0.4` and
`undici-types-7.18.2`. External installation was declined to keep the run
hermetic. The committed statement that the whole runtime sweep stays green is
consistent with the constrained duplicate SQL, but remains a prior receipt
rather than an independently completed replay here.

### Other corpus-invisible behavior

The graded corpus does not observe:

1. the variable-bound malformed reference in B1,
2. either compiler-only refusal in B9,
3. oracle edge, negation, splice, or wrapper expansion in B10,
4. duplicate joins in depth-3 construction due to B7,
5. plan quality for `supportSql`, `recomputeSql`, and naive mode due to B8,
6. physical child sharing due to B13.

## 5. Fixture redundancy

Four of the 11 fixtures repeat behavior already established by another new or
pre-existing fixture.

| Redundant fixture | Existing coverage |
|---|---|
| `relation_depth2_many_rows_share_one_leaf` | `relation_depth2_construct_and_read` covers the new depth-2 construction and read rewrite. `struct_shared_child_survives_one_release` at `v6/prolog/conformance/fixtures/4_struct_values.pl:433-450` covers shared child values. The new fixture cannot inspect internal child ids. |
| `relation_depth2_nested_decode_pattern` | `relation_depth2_chained_decode` already proves that a rule-built nested value is canonicalized and readable. The recursive nested-object decoder predates this feature at old `lower.pl:1006-1041`. |
| `relation_depth3_chained_decode` | `relation_depth3_construct_and_read` proves three-level canonical construction. The two decode spellings repeat the pre-existing recursive decoder after that value exists. |
| `relation_depth3_many_rows` | `relation_depth3_construct_and_read` covers the depth-3 rewrite, and `relation_depth2_many_rows_share_one_leaf` supplies the multi-row schedule shape. Its old-oracle leg was already green and its expectations omit the relation-valued columns. |

The three refusal fixtures are not pairwise identical:

- text literal exercises an atomic wrong value,
- wrong target exercises a compound with the wrong functor,
- wrong arity exercises the right functor with the wrong arity.

They still leave the variable-flow path in B1 uncovered.

## Executed checks

```text
git rev-parse HEAD
609066ee0f5f9b5e64837371680092134c11c20f

run_tests(relation_depth_lowering)
3/3 passed

pre-feature staged construction compile
exit 0

pre-feature staged construction oracle
[located(span(file(repo(acme),fpath('src/a.rs')),10,20),def)]

current oracle, negated nested pattern
[seen(10),raw(acme,'src/a.rs',10,20)]

current oracle, live edge nested value
span(obj([...]),10,20) present

current compiler, live edge nested value
unsupported_construct(relation_value_in_edge_rule(...))

current variable-flow case
oracle accepted span('src/a.rs',10,20)
compiler emitted direct text-to-reference insertion

memo-disabled Prolog conformance
186 passed

memo-disabled compile sweep
SWEEP total=186 compiled=131 unsupported=55 crash=0
```
