---
name: sprf-invariants-via-antijoin
description: Invariant violation checking in sprefa via DD antijoin. Note assertions like "warn if X is gone", cross-repo reference staleness, missing symbol detection. Antijoin retraction propagation gives auto-clear on fix for free. Load when designing diagnostic rules, note-orphan checks, or any "violation when Y not present" pattern.
---

# Invariants via DD antijoin

## The pattern

```sprf
symbol(:s, ${P}, ${NAME})           ; from each parsed file
note_ref(:n, ${PATH}, ${TARGET})    ; from your notes parser
note_orphan(${PATH}, ${TARGET}) <-
    note_ref(:n, ${PATH}, ${TARGET}) > !symbol?(:s, _, ${TARGET})
;
note_orphan(${PATH}, ${TARGET}) > publish_diag(${PATH}, "missing: ${TARGET}")
```

## What DD does for free

**Startup.** symbol bag fills. note_ref bag fills. Antijoin fires +1 for every note_ref whose target is not in symbol. Diag published.

**Fix scenario.** User adds a function named `${TARGET}`.
```
parse → symbol(:s, F, TARGET) +1 at T+1
antijoin re-evaluates → note_orphan -1 at T+1
publish_diag clears at T+1
```
ZERO custom code. The diag clears automatically because retraction propagates through antijoin.

**Break scenario.** User deletes the function.
```
parse → symbol(:s, F, TARGET) -1 at T+2
antijoin re-evaluates → note_orphan +1 at T+2
diag re-publishes at T+2
```

This is the invariant violation that v0 wanted (ref-v0-goals.md item 10: "Violation checking, hoping dd unlocks negation joins etc. for checking/verifying things"). It's antijoin in DD.

## The general shape

```
invariant(args) <- precondition_facts > !consequence_facts?
```

Read: "the invariant violation `invariant(args)` holds when the precondition facts are present AND the consequence facts are absent."

Rules are antijoins. The consequence facts can be:
- symbol declarations (note refs to nonexistent symbols)
- import targets (imports of nonexistent modules)
- file existence (refs to nonexistent paths)
- API endpoints (callers to nonexistent routes)
- database columns (queries on dropped columns)

## Cost for the note-orphan rule alone

```
symbol bag        ~10M rows for 50 GB source
                  (~200 symbols/file × 50k files)
note_ref bag      small, you write these by hand. ~10k.
antijoin trace    ~400 MB
warm round delta  μs–ms (focused hot class)
cold first-fire   ~1–5 s for the first sweep
```

## Cross-repo invariants

Same shape. The fact bags can hold rows from multiple repos. Antijoin works across repos because DD's joins don't care about file boundaries.

```
api_endpoint(:e, ${REPO}, ${ROUTE})    ; declared in service repos
api_caller(:c, ${REPO}, ${ROUTE})      ; emitted by caller repos
broken_caller(${REPO}, ${ROUTE}) <-
    api_caller(:c, ${REPO}, ${ROUTE}) > !api_endpoint?(:e, _, ${ROUTE})
```

When a service deletes an endpoint, every caller in every consumer repo gets a diag. When the endpoint comes back, diags clear. Cross-repo retraction propagation is free.

## Disambiguation: NOT all invariants are antijoins

Some invariant kinds need different operators:

| Invariant shape                  | DD operator    |
|----------------------------------|----------------|
| "X exists when Y absent"         | antijoin       |
| "X count exceeds threshold"      | reduce + filter |
| "X equals Y across renames"      | join + filter  |
| "X fixed-point closure includes" | iterate        |
| "no circular dependency"         | iterate + cycle detect |

Antijoin is the most common shape and the easiest to reason about. Default to it.

## Filter-to-zero diag (different concept)

This skill is about "invariant fires when X is gone." That's different from the runtime emission counter that warns when an op produces 0 rows for K gens — that's a debugging hint. Invariant rules legitimately produce 0 rows when the system is healthy.

To distinguish: tag invariant rules with a flag (e.g. `rule! note_orphan(...)`). The runtime emission counter ignores rules tagged this way.

## Performance anchor

For the note-assertion case described above on a 500-repo polyglot corpus:
- antijoin arrangements: ~400 MB for 10M-row symbol bag, ~10k-row note_ref bag
- per-edit propagation: μs–ms (only retouches affected joins)
- cold sweep (first publish): 1–5 s

These are anchors from comparable workloads. Real numbers come from running the perf test (see sprf-dd-memory-tiering-500).

## Sources

- chat_log/20260501.1.dd-effects-control-flow-types.md (note-orphan example)
- ref-v0-goals.md item 10 (Violation checking, negation joins for checking/verifying)
- v3 antijoin discussion (within sprefa-4m7 design vector)
