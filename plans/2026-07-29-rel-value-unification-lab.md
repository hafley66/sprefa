# Relation and value unification lab

## Context

This lab tests the existing V6 implementation. Specification-only Prolog
models are excluded from evidence.

The 2026-07-28 types-as-relations lab concluded that `rel` could carry value
and entity policies. The 2026-07-29 struct-as-rows implementation introduced a
separate surface declaration:

```text
type span(start: int, end: int).
```

Its implementation provides canonical structural values, dense storage mates,
join-based decoding, nested values, host outputs, and boundary rendering. Its
stated reason for using `type` was keeping emitted-only dictionary rows out of
observable relation deltas.

The user-directed hypothesis is:

```text
one rel declaration family
```

The lab must determine whether the current `type` invariants can be expressed
through `rel` without duplicating storage, leaking internal rows, changing
clock behavior, or adding compulsory `ref` spelling.

The same actual-world process must test:

- automatic relation-valued columns versus explicit reference operations;
- ordinary RHS relation membership versus dereferencing a relation value;
- `rel A -> B` versus ordinary relations with modes;
- link-time host and world bindings;
- SQLite storage, indexes, statement counts, and query plans;
- Rx demand, response, cache, retraction, and cancellation clocks.

## Evidence law

A claim counts only when it is produced by one of:

1. the current DL6 parser;
2. the current compiler analysis and lowering pipeline;
3. emitted TypeScript executed against SQLite;
4. the current Prolog oracle;
5. the current served host/bind runtime;
6. a measured database or process result.

An isolated semantics model does not count.

Prototype compiler work lives behind a lab-only entry point or in recoverable
temporary files. Shipping parser, compiler, runtime, and fixture behavior
remain unchanged until user review.

## Questions

### Q1: What does current `type` actually provide?

Inventory every parser term, IR fact, storage table, emitted join, renderer,
runtime coercion, refusal, and fixture relying on `type`.

### Q2: Can `rel` produce the same compiler plan?

Add one lab-only normalization path for a candidate `rel` spelling. Compile
paired programs through the real pipeline and compare:

```text
parsed declarations
expanded declarations
relation plans
DDL
lowered statements
emitted module
oracle tick log
emitted tick log
final rows
database tables and indexes
```

### Q3: What happens without explicit `ref`?

Test actual programs for:

```text
rel span(start: int, end: int).
rel finding(at: span, message: text).
```

Separate:

- a top-level RHS `span(Start, End)` membership read;
- a parent RHS read that carries an opaque span identity;
- nested span destructuring;
- constructing a span in a fact, head, host response, and world arrival;
- scanning spans while also capturing row identity;
- missing referenced rows;
- cross-relation identity mistakes.

Automatic inference survives only where the real checker has enough
information to produce the same typed join as an explicit reference form.

### Q4: Is `ref` needed?

Prototype at most three actual parser forms:

```text
automatic nested relation pattern
ref(Identity, RelationPattern)
ref(Relation, Identity, Fields...)
```

Compare parser ambiguity, inferred types, lowered SQL, emitted module bytes,
and required indexes. Equivalent surface forms must normalize through one
compiler path.

### Q5: Does `rel A -> B` have one semantics?

Test real compiler/runtime candidates for:

```text
mode direction
cardinality
functional dependency
keyed replacement
host production
```

The arrow survives only if one expansion predicts SQL constraints and Rx
deltas for deterministic, semideterministic, multi-row, cached, and mutable
cases. Otherwise test ordinary relation declarations plus mode metadata.

### Q6: How do bindings participate?

Use the existing `sh`, `probe`, `bind`, watch, interval, and served-host
fixtures. Measure:

- whether binding attaches to one existing relation or creates parallel
  demand/response relations;
- cold-miss and cache-hit tick positions;
- multi-row response replacement;
- cancellation and late responses;
- link-time signature failures;
- provider batch size and process-spawn count.

### Q7: Does storage remain rational?

Measure paired schemas with SQLite:

- database bytes per fact;
- statement count versus demand count;
- RSS;
- full-path and span filters;
- reverse reference lookup;
- `EXPLAIN QUERY PLAN`;
- duplicate content construction;
- shared child release;
- orphan detection;
- migration database shape.

## Invariants under test

### Declaration and type invariants

- one declaration category if the hypothesis holds;
- total columns;
- scalar domains remain distinct;
- relation identities remain distinct from integers;
- RHS variables preserve relation identity domains;
- unknown domains and cross-domain joins fail by name;
- integer literals never infer a relation identity.

### Identity and lifetime invariants

- logical keys remain separate from dense storage mates;
- full-row keys provide content identity;
- partial keys provide entity or state identity;
- content-key cycles fail;
- independent-key cycles remain representable;
- mutable entities retain temporal identity;
- logs do not lose referenced identity when current state retracts.

### Relational use invariants

- top-level RHS atoms retain membership semantics;
- dereference lowers to ordinary joins;
- construction interns once;
- missing targets do not fabricate values;
- explicit and automatic forms share one lowering;
- relation rows remain directly queryable when explicitly requested.

### Storage invariants

- paths, names, digests, and structural values store once;
- child and parent writes commit atomically;
- shared children survive one parent's retraction;
- dense mates do not cross the value boundary;
- internal support tables do not leak;
- statement count remains batch-shaped;
- hot joins use indexed SEARCH plans.

### Clock and binding invariants

- level and edge rules retain their current clocks;
- cold provider responses arrive after demand commit;
- cached responses participate as current rows;
- keyed replacement emits the established remove/add boundary;
- cancellation retracts demand support;
- late response policy is explicit;
- multi-row answers replace as complete batches;
- provider signatures include types, modes, cardinality, lifetime, and batch
  capability;
- one input batch does not become one subprocess or query per row.

### Optionality and variants

- optional links use relation membership;
- exclusive variants use variant relations and match;
- Git and stored content remain additive capabilities;
- nullable variant payloads and sentinel identities are excluded.

## Fixpoint procedure

The active session performs the iteration:

1. Select the smallest unresolved question.
2. Capture current behavior with a failing or characterization fixture.
3. Add one lab-only compiler/runtime candidate.
4. Run both through the oracle and emitted runtime.
5. Compare plans, bytes, rows, ticks, statements, and storage.
6. Record the crack rather than repairing it with new syntax.
7. Feed the crack into the next candidate.
8. Stop when one model explains every invariant and every accepted surface
   lowers through one path.

## First experiment

The first experiment uses the existing struct fixtures:

```text
struct_column_renders_canonical_json
struct_intern_order_a
struct_intern_order_b
struct_nested_value_renders_whole_tree
struct_shared_child_survives_one_release
struct_span_columns_are_int_after_decode
struct_host_output_schedule_answer_interned
```

It will:

1. census the current `type` surface and internal dependencies;
2. recover the existing inline-versus-struct migration twin;
3. replace the surface parser branch with referenced-`rel` normalization;
4. compare the real plans and outputs;
5. identify the first actual compiler seam preventing unification.

### Removal result

Actual surface census:

- 20 `type` declarations;
- 19 DL6 files containing them;
- 18 struct conformance fixtures;
- six Prolog modules reading `type_decl`;
- three compiler/runtime files calling `column_storage`.

Actual programs:

```text
v6/prolog/labs/rel_value_unification/0_type_current.dl6
v6/prolog/labs/rel_value_unification/1_rel_candidate.dl6
```

Current compiler receipt:

```text
v6/prolog/labs/rel_value_unification/2_receipt.sh
```

Pre-removal result:

```text
PASS  current_type_surface_compiles
PASS  rel_surface_reaches_real_column_type_unknown
```

Both declaration spellings parse. The current `rel` candidate reaches the real
program checker and fails with:

```text
column_type_unknown(span)
```

The first seam is therefore the construction of the value-definition table,
not the lexer or basic relation declaration parser.

The normalization now runs in the shipping parser. The focused receipts are:

```text
v6/prolog/labs/rel_value_unification/3_real_normalize.pl
```

For a relation referenced in column type position, the parser converts that
relation declaration to the existing `type_decl` IR, then runs the unchanged
analyzer, lowerer, and TypeScript emitter.

Result:

```text
PASS  referenced_rel_surface_compiles
PASS  removed_type_surface_is_rejected
PASS  referenced_rel_normalizes_to_existing_type_ir
PASS  real_analyzer_accepts_normalized_rel
PASS  real_lowerer_accepts_normalized_rel
PASS  real_emitter_accepts_normalized_rel
```

All 20 live declarations were migrated to `rel`. The DL6 highlighter and
canonical printer no longer emit or recognize `type`.

### Measured cost

- Internal `type_decl` IR remains, so analyzer, SQLite dictionary storage,
  interning, rendering, host decoding, and refusal behavior remain unchanged.
- A `rel` referenced as a column domain becomes a value relation for the whole
  program. Its declaration is removed from the public relation plan.
- The same declaration cannot currently be both a directly queryable public
  relation and a referenced value relation.
- Classification is contextual. Adding a column such as `at: span` changes
  the role of `rel span(...)`.
- No `ref`, arrow, mode, host, clock, or runtime syntax was added.

### Relational replacement prototype

The value-only normalization above was rejected. The current prototype keeps
the referenced declaration as an ordinary queryable relation and uses its
row identity as the endpoint stored by the parent:

```text
rel span(start: int, end: int).
rel finding(path: text, at: span).
```

Measured emitted storage:

```sql
CREATE TABLE span (
  __id INTEGER PRIMARY KEY,
  start INTEGER NOT NULL,
  end INTEGER NOT NULL,
  UNIQUE(start, end)
);

CREATE TABLE finding (
  path TEXT NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY(path, at)
) WITHOUT ROWID;
```

There is one ordinary `span` table. `__id` is a physical surrogate. The
language-visible row remains `span(start, end)`. `finding.at` is the edge
endpoint. No `__dict_*` table, `__semantic` column, `__rendered` column, or
stored JSON value exists. A temporary `__ref_span` SQL view exposes
`(__id,start,end)` to generated joins without creating another stored table.
`json_each(?)` remains only as the existing batched SQL parameter transport.

Actual compiler lab:

```text
v6/prolog/labs/rel_value_unification/4_reference_relation.dl6
v6/prolog/labs/rel_value_unification/5_reference_relation_holes.pl
```

Result:

```text
PASS  reference_target_remains_public_rel
PASS  parent_column_is_typed_reference_edge
PASS  no_dictionary_or_stored_json_columns
PASS  reference_target_has_one_ordinary_table
PASS  direct_rhs_relation_query_compiles
PASS  reference_dereference_is_an_indexed_relational_join
PASS  no_foreign_key_or_cascade_policy_invented
PASS  existing_key_is_not_yet_used_as_reference_identity
PASS  existing_keyed_cycle_is_still_refused
```

The last two passes confirm current holes:

1. `key(...)` already states entity identity, but reference insertion and
   lookup still use the full row.
2. A keyed entity cycle is still rejected by the old content-value DAG check.

Both holes can be closed with existing semantics. Reference identity uses the
target relation's existing key, with the full row as the unkeyed set default.
The cycle check applies only when identity recursively includes reference
columns. A key containing only scalar or already-ground identity columns does
not require child-first content identity.

The 102-fixture emitted sweep now executes all nine former struct acceptance
fixtures. Their results disagree with the old oracle because the emitter
returns edge endpoint ids plus public child relation rows while the oracle
still returns nested JSON-shaped values and hides child rows. Counts:

```text
102 compiled
91 identical outside the changed model
9 intentional old-oracle disagreements
2 pre-existing run errors
```

No new language concept is justified by this pass. The remaining work is
compiler and oracle migration onto existing `rel`, `key`, rule, and clock
semantics.

## Decisions

Decisions about the lab:

1. Current-world evidence only.
2. The `type` surface word is removed.
3. Existing internal value storage remains unchanged.
4. `ref` and arrow syntax remain undecided.
5. Modes, keys, cardinality, clocks, and providers are graded independently.
6. Every accepted convenience form must lower through one relational kernel.

Rejected as evidence:

- standalone semantic models;
- asserted SQL strings not emitted by the compiler;
- prose-only Rx timelines;
- single-row host tests used to claim batching;
- parser acceptance without execution parity.

## Verification

Baseline gates:

```text
swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt
swipl -q -l v6/prolog/conformance/go.pl -g go -g halt
```

Current baselines:

- compiler unit tests: 142/142;
- conformance: 163/163;
- focused live host/watch/decode: 6/6.

Focused current-world receipts:

```text
v6/prolog/labs/rel_value_unification/2_receipt.sh
swipl -q -l v6/prolog/labs/rel_value_unification/3_real_normalize.pl -g go -g halt
```

Each experimental pair must add:

- parser and printer receipt;
- plan comparison;
- both emitter modes;
- oracle tick-log comparison;
- final-row comparison;
- schema and query-plan capture.

## Staffing

- Implementer: current Codex session.
- Worktree: shipping parser, printer, highlighter, fixtures, and docs changed.
- Base: current local `main`.
- Concurrent `v6/INDEX.md` change remains untouched.
- Battery budget: focused receipts per iteration, full compiler and
  conformance gates at stable checkpoints.

<!-- todo(decision): Decide automatic versus explicit relation-reference
spelling only after both forms run through the real parser and compiler. -->

<!-- todo(decision): Decide rel A-to-B versus ordinary rel plus modes only
after deterministic, multi-row, keyed-update, cached, and host-produced real
fixtures establish its SQL and Rx meaning. -->
