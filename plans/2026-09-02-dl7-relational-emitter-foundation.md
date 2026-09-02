# DL7 Relational Emitter Foundation

## Context

The V7 compiler closes two related data sets:

- `CompilerFacts` contains the type graph, interned applications, generated
  declarations, and compiler-derived rows.
- `RuntimeProgram` contains checked relation declarations, seeds, rules,
  dependencies, and strata.

`v7/src/3_emit/0_artifact_emitter.pl` currently exposes both through a Prolog
record, but DL7 emitters can query only `CompilerFacts`. A DBSP, SQL, Rust, or
schema emitter therefore lacks one ordinary relational input describing the
checked runtime program.

DL6 packed semantic relation data, SQLite layout, generated SQL, retention,
host adapters, and transport fields into `plan/9`, `rel/5`, and one large JSON
document. V7 needs separate relational layers so one target cannot add its
storage vocabulary to the compiler kernel.

The same separation applies to structured payloads and source extraction.
`json(Type)` should be an ordinary capability row over a type node. A
Tree-sitter query should be parsed source data. Storage representation bridges
and extractor execution belong to later target plans.

## Decisions

1. Reify every checked runtime program as target-neutral rows:

   ```text
   program_relation(Relation, Arity)
   program_key(Relation, KeyOrdinal)
   program_key_position(Relation, KeyOrdinal, Position)
   program_seed(Seed, Call)
   program_rule(Rule, HeadCall)
   program_goal(Rule, GoalOrdinal, Polarity, Call)
   program_apply(Call, Relation)
   program_argument(Call, Position, Value)
   program_dependency(HeadRelation, BodyRelation, Polarity)
   program_stratum(Relation, Level)
   ```

   Seed, rule, head-call, and goal-call identities are deterministic structural
   identities derived from their checked-program ordinal. Values retain the
   checked IR vocabulary: `ref/1`, `const/1`, `var/1`, and aggregates.
2. `compiler_view/2` exposes four fields in order:
   `TypeGraphFacts`, `CompilerFacts`, `LogicalProgramRows`, and
   `RuntimeProgram`. Host emitters can use the rows or the checked IR during
   migration. DL7 emitters receive compiler facts plus logical rows as their
   queryable input.
3. Logical rows describe relational behavior only. They contain no table name,
   SQL type, SQLite expression, Rust type, retention policy, wire shape, or
   decode call.
4. Layout rows form a later target-selected graph derived from logical rows and
   type capabilities. Rendering consumes layout rows. No renderer reaches back
   into source syntax.
5. Structured representation is an ordinary relation:

   ```text
   (: json (* (: type type)))
   (json Payload)
   ```

   `json(Type)` states a capability of `Type`. It neither declares a primitive
   nor selects a database encoding.
6. Representation conversion is inferred in the layout planner. A bridge row
   is needed when a target stores a capable node in an encoded representation
   and a logical consumer follows its structured edges. DL7 source has no
   `decode` form.
7. Tree-sitter query syntax is one raw query literal inside ordinary DL7 call
   syntax. The reader preserves exact query text as data. The extraction
   adapter compiles that text, reports capture names, joins query demand with
   files, and emits rows. Query reading does not perform filesystem or parser
   effects.
8. Compiler performance has two guard surfaces:
   the always-on phase trace and a repeatable performance probe. The probe
   records cold inference counts, closure rounds, row counts, and warm-cache
   behavior. Wall time is reported but does not act as the sole regression
   gate.

### Data flow

```text
DL7 source
    |
    v
checked runtime program ---------> logical program rows
    |                                  |
    |                                  +----> DBSP layout rows
    |                                  +----> SQL layout rows
    |                                  +----> Rust layout rows
    |
type/compiler graph
    |
    +---- json(Type)
    +---- query literal data
    +---- userland type operations
```

### Representation bridge

```text
logical use follows fields
            +
target layout stores encoded bytes/text
            |
            v
representation_bridge(StorageValue, LogicalType, UseSite)
            |
            v
target renderer chooses its decode/extract operation
```

The bridge is target-plan data. `decode(...)` remains absent from DL7 syntax
and the target-neutral logical program.

## Migration sequence

<!-- todo(planning): Define target layout rows and representation_bridge derivation without target names in comptime predicates. -->
<!-- todo(emitter): Implement the first DBSP relational-plan emitter over logical and layout rows. -->

## Verification

- Reifying the same checked program twice yields term-identical sorted rows.
- Every declared relation has one `program_relation/2` row and one
  `program_stratum/2` row.
- Every seed, rule head, and body goal has one application row and one argument
  row per relation position.
- Logical dependencies equal the checker-owned `depends/3` rows.
- Existing monomorphic Datalog emission remains term-identical.
- Existing Prolog and DL7 emitter tests consume the expanded compiler view.
- `json(Type)` compiles through ordinary declaration, fact, and relation
  machinery; no JSON primitive or compiler case is added.
- Query literals round-trip exact Tree-sitter query text and cause no
  extraction effect during compilation. Capture validation belongs to the
  extraction adapter checkpoint.
- The performance probe fails on structural budget drift and prints wall-time
  measurements for comparison.
- The complete V7 SWI suite and Tree-sitter corpus pass before merge.

Implementation receipts on 2026-09-02:

- `0_logical_program_reifier.pl` emits deterministic relation, key, seed,
  rule, goal, call, argument, dependency, and stratum rows.
- `compiler_view/2` and the `relational_program` emitter expose those rows
  without adding them to comptime evaluation.
- `json(Type)` is an ordinary unary prelude relation; the test fixture contains
  no `primitive(json)` term.
- Raw `{ tree-sitter query }` literals are accepted by both readers, preserve
  exact inner text, and compile as ordinary ground call data.
- `just compiler-perf` measured 62,761,010 cold inferences, seven closure
  rounds, 6,774 compiler rows, and 1,822 warm inferences. The measured wall
  times were 14,197 ms cold and 264 ms warm.

## Staffing

- Implementation and review: Codex directly, high reasoning.
- Worktree: `/private/tmp/sprefa-v7-value-nodes`.
- Branch: `feature/v7-generated-application-nodes`.
- Base SHA: `28c889b0cc88678aa3383ef7f7cf7be71718d373`.
- Suite budget: focused emitter, parser, and performance checks per commit;
  complete V7 SWI suite and Tree-sitter corpus before push.
