# DL7 programmable compiler fragments

Date: 2026-08-30

## Goal

Let ordinary DL7 compiler rules generate relation definitions and executable
rule heads and bodies. Generated fragments enter the next bounded compiler
round, pass the ordinary Datalog checks, and survive in the checked runtime
program. `HistoryV1` is the first proof.

## Public relational carrier

```text
def(Relation, Arity)
head(Rule, Relation)
head_arg(Rule, Position, Kind, Value)
body(Rule, GoalIndex, Polarity, Relation)
body_arg(Rule, GoalIndex, Position, Kind, Value)
```

Keys:

```text
def       (Relation)
head      (Rule)
head_arg  (Rule, Position)
body      (Rule, GoalIndex)
body_arg  (Rule, GoalIndex, Position)
```

Argument kinds in the first slice:

```text
variable   Value is the variable's text identity
constant   Value is a scalar constant
reference  Value is a canonical type or relation reference
```

## Type signatures

```prolog
assemble_generated_program(
    +CompilerRows,
    +BaseRelations,
    -GeneratedRelations,
    -GeneratedRules,
    -Diagnostics
).

check_resolved_rules(
    +Relations,
    +Rules,
    -Depends,
    -Strata,
    -Diagnostics
).
```

Generated relations have the existing checked shape:

```prolog
relation(ref(RelationId), Arity, KeySets)
```

Generated rules have the existing checked shape:

```prolog
rule(
    call(ref(HeadRelation), HeadArguments),
    [checked_goal(Polarity, call(ref(BodyRelation), BodyArguments)), ...]
)
```

## Instance timeline

```text
round N frozen graph + generated program N
    -> evaluate ordinary compiler rules
    -> derive def/head/body rows
    -> assemble generated program N+1
    -> check arity, dense positions, variables, modes, safety, strata
    -> refreeze
round N+1 executes the generated rules
```

The compiler reaches stability when all three sets are unchanged:

```text
type edges
application requests
generated relations and rules
```

The existing 16-round bound covers all three.

## Storage, reads, writes, and uniqueness

- Compiler carrier rows are immutable set rows within one evaluation.
- A generated relation identity determines one arity.
- A rule identity determines one head.
- Head and body argument positions are dense and zero-based.
- Body goal positions are dense and zero-based.
- Generated rules write ordinary relation rows through the shared evaluator.
- Compiler carrier rows remain compiler evidence. They create no target table.
- Target-neutral layout work consumes the generated checked relations later.

## HistoryV1 proof

`HistoryV1(Source, Options, Result)` interns one canonical specialization. Its
compiler rules:

1. create `Result` as a product node;
2. copy the source's ordered edges;
3. derive `def(Result, Arity)`;
4. derive one generated head over `Result`;
5. derive one positive body goal over `Source`;
6. derive matching variable arguments for both sides.

The fixture supplies a typed options node containing a `mode` edge. The
HistoryV1 rules require that edge, proving options participate in
specialization. The generated rule copies one source row into the generated
history relation during a later compiler round.

This proof establishes programmable schema and behavior. Version allocation,
timestamps, append-only flow, retention, and operation rows remain userland
HistoryV1 library rules built after the generated-program carrier is proven.

## Verification

- Extend the existing consolidated entrypoint oracle.
- Assert one canonical HistoryV1 type identity.
- Assert copied generated edges and relation definition.
- Assert exact assembled head and body.
- Assert the generated rule derives its output row.
- Assert the checked runtime program contains the generated definition, rule,
  dependency, and stratum.
- Keep Tree-sitter at its single corpus file.
