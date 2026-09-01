# DL7 Callable Relations

## Context

DL7 currently gives product nodes a fixed-arity relation declaration during
lowering, while sum nodes receive only `node/1`, `sum/1`, and `:/4` graph rows.
The split is explicit in
`v7/src/2_comptime/0_lowerer.pl` at `constructor_relations/4`.

Expression calls already support one `return` edge, nested lowering, positional
partial applications, direct calls in heads and bodies, and canonical type
construction through `intern/3`. Generated `def/head/body` carrier rows become
checked relations and rules during compiler rounds. The initial checker still
resolves every authored call before those generated declarations are visible in
`v7/src/2_comptime/2_compiler.pl`.

The target model is relational:

```text
callable     = a node with a relation definition
call         = an ordinary relation tuple
partial call = relation identity plus supplied argument slots
expression   = an ordinary call with one selected result edge
```

Variables retain ordinary logic-variable semantics. Slot presence determines
partial-call completion. Groundness determines whether compile-time identity
construction and compiler effects have enough data to run.

## Decisions

1. `apply/3` will not become a public DL7 runtime primitive. Source calls lower
   to ordinary fixed-arity Datalog calls before runtime emission.
2. Generated relation declarations become visible before final authored-call
   validation. This is the sole new kernel allowance required for userland
   callable constructors.
3. Argument normalization uses relation edge labels and ordinals. Explicit
   labels win, variable-name puns bind matching labels, and remaining positional
   arguments fill remaining ordinals.
4. Defaults are compile-time facts over callable argument edges. They are
   inserted when a call is finalized. Explicit wildcard arguments count as
   supplied and suppress defaults.
5. Goal calls fill omitted non-default slots with fresh logic variables.
   Expression calls preserve omitted required input slots as a partial call.
6. A relation's `return` edge selects the value of an embedded expression.
   Variables themselves carry no input/output classification.
7. Call modes are userland graph data. The first implementation preserves the
   existing single-`return` forward mode and leaves multiple mode selection as
   derived prelude rules.
8. Sum alternatives lower to ordinary callable relations. The sum node remains
   the closed set of those alternatives; match coverage is set subtraction over
   the sum edges and authored arm rows.
9. Exhaustiveness conditions are userland relations. A generic compiler
   diagnostic sink converts their final rows into positioned diagnostics.

Rejected alternatives:

- Runtime `apply/3`: leaves dynamic predicate dispatch in emitted programs and
  prevents direct static SQL lowering.
- Groundness-triggered invocation: changes ordinary reverse and unground Prolog
  queries into partial values.
- Sum-specific host expansion: duplicates the userland `def/head/body`
  generation mechanism.
- Variable kinds for input and output: makes relation direction part of variable
  syntax instead of a mode of calling the relation.

## Relational protocol

Every authored call is normalized into a compiler-owned pending call record:

```text
pending call
  callee        relation identity
  use           goal | head | expression
  supplied      labelled and ordinal argument bindings
  source        reader node identity
```

Prelude rules derive effective slots:

```text
explicit or punned supplied slot
                  │
                  ├───────────────┐
                  ▼               ▼
            effective slot   suppress default

omitted slot + default ────────► effective slot
omitted required expression slot ► partial call
omitted goal slot ──────────────► fresh logic variable
```

After compiler declarations stabilize, every pending call must become one of:

```text
ordinary fixed-arity call
partial-call value consumed by a later expression application
positioned compiler diagnostic
```

The compiler emits no pending-call or partial-call transport into runtime IR.

## Minimal kernel and maximal userland

Kernel responsibilities:

- parse forms, retain source identities, and create pending call records;
- execute stratified compiler rules to fixpoint;
- freeze generated relation declarations before final call validation;
- assemble generated `def/head/body` rows;
- intern canonical identities after identity arguments are ground;
- convert final generic diagnostic rows into compiler diagnostics;
- require runtime IR to contain only statically resolved relation calls.

Prelude responsibilities:

- product call signatures;
- sum alternative declarations and constructor rules;
- named arguments, punned arguments, omission, and defaults;
- partial-call merge and completion;
- generic specialization and mapped type operators;
- call-mode facts and result-edge selection;
- match arms, wildcard coverage, exhaustiveness, and overlap relations.

### Implemented boundary

The compiler now performs a permissive bootstrap lowering, executes compiler
rules to closure, freezes generated declarations and their final `:` bindings,
then strictly lowers the original source. Generated relations can consequently
appear in authored facts, rule heads, and rule bodies. The final runtime graph
is checked again and contains no deferred-call transport.

Named, punned, positional, omitted, and partially supplied arguments currently
normalize inside `0_lowerer.pl`. This proves their static Datalog erasure and
keeps runtime IR clean, but it does not yet satisfy the maximal-userland side of
the design.

### Next generic carrier

One reified call-site protocol replaces the remaining call-policy predicates:

```text
call_site(Call, Callable, Use, Source)
supplied_slot(Call, Index, Value, SupplyKind)
declared_slot(Callable, Index, Label, Target)

                 userland compiler rules
                            │
                            ▼
effective_slot(Call, Index, Value)
partial_call(Call, Callable)
completed_call(Call, Callable)
compiler_error(Source, Reason)
```

The kernel then needs only two generic operations:

1. expose source calls and declaration edges as immutable rows;
2. assemble each `completed_call` plus its dense effective slots into one
   statically addressed Datalog tuple.

Defaults become ordinary rules joining omitted slots to annotation edges.
Punning becomes a rule joining variable spelling to declared labels. Partial
application becomes set difference between declared and effective slot
ordinals. Match exhaustiveness becomes set difference between sum-alternative
edges and arm rows. Their behavior can change in the prelude without changing
the reader, checker, evaluator, or runtime IR.

<!-- todo(feature): Normalize explicit, punned, omitted, and defaulted argument slots. -->
<!-- todo(refactor): Reify call sites so slot normalization and partial completion move from 0_lowerer.pl into prelude rules. -->
<!-- todo(feature): Generate callable sum alternatives and userland match coverage. -->
<!-- todo(feature): Convert final userland compiler-error rows into positioned diagnostics. -->

## Verification

Focused tests will cover:

- a generated relation called by authored facts and rules in the same unit;
- a generated relation used as an authored rule head;
- forward and reverse calls containing unground variables;
- explicit named arguments in shuffled order;
- variable-name puns, mixed punned and positional arguments, and alpha-sensitive
  diagnostics;
- defaults, explicit override, wildcard suppression, and defaults dependent on
  previously supplied slots;
- positional and named partial calls completed in later nested expressions;
- no partial-call terms in checked runtime IR;
- callable zero-field and nonempty sum alternatives;
- exhaustive, missing, wildcard, and overlapping match arm sets;
- generic sum specializations retaining their alternative relations;
- two consecutive compilations producing equal compiler rows and runtime IR;
- existing V7 reader, entrypoint, and module suites.

The full V7 SWI gate is:

```sh
swipl -q -g "load_files(['v7/test/0_reader.test.pl','v7/test/1_entrypoints.test.pl','v7/test/2_module_system.test.pl'],[silent(true)]),run_tests,halt"
```

Baseline on `db081a371`: 32 tests passed in 14.4 seconds.

## Staffing

- Implementation: Codex directly in isolated worktree, high reasoning.
- Review: separate reviewer after the focused suite is green.
- Worktree: `/private/tmp/sprefa-v7-callable`.
- Branch: `feature/v7-callable-protocol`.
- Base SHA: `db081a371`.
- Suite budget: focused tests after each checkpoint; complete 32-test V7 suite
  before every integration commit touching compiler ordering.
