# Conditionals, Recursion, Negation

These three are relational, not control flow. sprf adds **no new
operator** for them — they fall out of the pipe (`>`), rule overloads,
and the existing `FactRead` antijoin.

## `>` is `if`

A pipe step that can drop its cursor is already a conditional. The
cursor flows to the next step only when the step emits; a step that
emits nothing for a given cursor is a failed branch.

```
src > re`(?P<N>\d+)` > tag(:hit, N)
```

`re` emits per match and drops on no match, so `tag` runs only for
cursors that matched. There is no `if` keyword because `>` already
gates. A predicate step (`where`, `fact?`) is the pure form: it passes
the cursor through unchanged on success and drops it on failure.

## Recursion = rule self-call + base overload

A rule may call itself. Termination comes from a second overload of
the same name whose column set is the base case, plus the existing
rule fixpoint (rows reach a fixed point and the engine stops). See
`v4-retraction-fixpoint-plan.md` for the fixpoint contract.

```
rule(:reach, FROM?, TO?)             { edge?(FROM?, TO?) }          # base
rule(:reach, FROM?, TO?)             { reach?(FROM?, MID?)
                                       > edge?(MID?, TO?) }          # step
```

The discriminant is the column set present at the call site (variant =
name-overload, the C-spine rule). No explicit recursion marker; the
fixpoint bounds it.

## Negation / `else` = stratified antijoin

Honest negation is `WHERE NOT EXISTS`, surfaced by
`FactRead::anti` (`JoinKind::Anti`, `v4/src/fact.rs`). Semantics:
pass the cursor through iff the keyed table has **no** matching row;
drop it on any match. It is the mirror of the default semi-join
(`Inner`: flow N times for N matches, drop on empty).

```
expected_docs?(DOC?)
  > sql`
      SELECT input.__cursor_idx, input.DOC
      FROM input
      WHERE NOT EXISTS (
        SELECT 1 FROM actual_docs WHERE actual_docs.DOC = ${DOC}
      )
    `
  > missing_docs(DOC)
```

`else` is the same shape: the negative stratum is the antijoin of the
positive one. Stratification (compute the positive relation fully
before its antijoin consumer) is what keeps it well-founded; the
fixpoint provides the stratum boundary.

## Tie-in: types as rules

Because a type is a rule whose columns are its fields (see
`resolve_dot` / `lang/dot-miss`), drift detection is just an antijoin
between a declared type and its observed instances: a field present in
the type rule but absent on disk falls out of `WHERE NOT EXISTS` and
becomes a loud row, not silence. This is the autodoc north-star —
`Ty.field` addressing plus the antijoin above.
