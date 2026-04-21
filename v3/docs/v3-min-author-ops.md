# Appendix: v3 optimizes for min-author-ops

The v3 design target, stated as a single optimization:

> **Minimize the number of files an op author must edit to add or
> extend a capability.**

Every trait surface, every associated type, every registry choice in
v3 is justified by how it affects this number. If a v3 change does not
reduce author-edit fanout, it does not pay.

---

## The metric

For each capability an op can add, count:

- files created
- files modified (excluding `mod.rs` module declarations)
- central enums / trait-method sets touched

Lower is strictly better. The ideal is `(1, 0, 0)`: one new file, zero
existing edits, zero central growth.

---

## What v3 targets

| capability | v2 today | v3 |
|---|---|---|
| new effect kind | `ReadBatch` variant + reader match arm + op | 1 file |
| new capture kind | `CaptureKind` enum variant + hover arm + op | 1 file |
| new op | op file + grammar registration + capture variant | 1 folder |
| new diagnostic | `impl Diagnostic` + op | 1 file (already fine in v2) |
| new sub-lang (ast_grep/regex/sql/sem) | parse enum + lower match + op | 1 folder |
| new mutation | `MutationEffect` impl + handler enum arm | 1 file |
| new runtime knob | `RuntimeConfig` field + `test_default` + N call sites | 1 field on op struct (if op-local) |

`(1, 0, 0)` is not achievable for every capability — the registry
needs one global dyn cell, the grammar needs to know op names — but
it is the direction every v3 decision pulls toward.

---

## Validation prototype

`v3/experiments/effect_proof/` is the smallest executable proof of
the thesis for the effect-dispatch slice. Numbers at time of writing:

- framework core (`src/lib.rs`): 126 LoC
- effect #1 (`src/effects/read_bytes.rs`): 36 LoC
- effect #2 (`src/effects/count_lines.rs`): 32 LoC
- tests (`tests/surface.rs`): 59 LoC, 4 passing

Adding effect #2 touched:
- 1 new file (`count_lines.rs`)
- 1 line in `mod.rs`
- 0 edits to `lib.rs`
- 0 central enums

`(1, 1, 0)` — the one modification is the `mod.rs` one-liner, which
is a rustc requirement, not a framework concession.

The test file `tests/surface.rs` asserts the typed roundtrip, the
two-effect coexistence, the absence of type-erasure in authorable
surface, and the clear panic on unregistered effects.

---

## Drill for re-validating on new capabilities

Each time v3 grows, re-run the drill:

```bash
source v3/experiments/effect_proof/plugins.sh
_.sprfv2.expr.plugins.add <new_effect_name>
# edit + test
_.sprfv2.expr.plugins.test
_.sprfv2.expr.plugins.audit    # src/lib.rs must stay untouched
_.sprfv2.expr.plugins.count    # LoC per file; framework:ops ratio
```

If `audit` finds `Any`/`downcast`/`TypeId` leaks into
`src/effects/` or `tests/`, the thesis is broken and the framework
abstraction failed. If `lib.rs` grew to accommodate the new effect,
the thesis is broken and a new central enum was introduced.

---

## How this shapes the Phase A–D surface

From `v3-plugin-author-surface.md`, each row exists because its
absence would force a cross-cutting edit. The min-author-ops rule is
why each row is a single trait slot instead of a framework match arm:

| phase | rule | min-author-ops payoff |
|---|---|---|
| A (parse + lower) | op owns its args grammar, decls, uses | no central parser dispatch per op |
| B (LSP) | op owns hover / completion / signature | no central hover match arm per op |
| C (runtime) | op calls `ctx.put<E>` with typed response | no central Reader/Writer method per effect |
| D (side effects) | op provides `impl MutationEffect` | no central mutation-handler variant |

Any row that would require cross-cutting edits has been identified as
a Tier-2 collapse target (see `v3-plugin-author-surface.md` Inv-1
audit). The remaining Tier-1 dyn boundaries (`Captures`, `Slots`, the
one registry cell) are inherent to "one registry, N kinds" and do not
grow when ops are added.

---

## The cost side

Minimizing author edits has an inherent framework cost:

- The framework core wears the where-clause ladder
  (`Send + 'static` bounds on every generic).
- The framework core wears the `Any`/downcast dance inside `put<E>`.
- Stack traces pass through one extra frame (the batcher).
- Monomorphization grows binary size ~5–8% (Haxl-port measured;
  see `chat_log/20260418.2.v3-design-and-numbers.md`).

These costs are paid once, in one file, by the framework maintainer.
Op authors never touch them. This is the trade the design takes.

---

## Anti-goals

Min-author-ops is not min-LoC. Specifically:

- Do not collapse distinct op-author slots into one fat trait just
  to reduce files. Separate slots per concern (grammar, pipe, hover,
  diag) cost zero extra files but preserve clarity and make future
  additions one-row edits.
- Do not hide effect responses behind `Box<dyn Any>` at the call
  site to "simplify." The typed `E::Response` is the whole point —
  it is what makes `ctx.put` pleasant to read and write.
- Do not add a runtime-config knob when an op-local struct field
  suffices. `RuntimeConfig` is reserved for cross-cutting invariants
  (buffer sizes, file caps, cancellation thresholds).

---

## Cross-references

- `v3-plugin-author-surface.md` — the full A–D touchpoint inventory.
- `convergent-evolution-effect-dispatcher.md` — why four ecosystems
  converged on the surface this prototype validates.
- `v3-vs-v2-reading-preview.md` — what a v3 op looks like to read.
- `../../experiments/effect_proof/README.md` — the drill harness.
- `chat_log/20260418.0.v3-effect-algebra-and-harmonization.md` — the
  locked algebra.
- `chat_log/20260418.2.v3-design-and-numbers.md` — LoC delta,
  per-effect batching policy, migration path.

---

## Evolution notes 2026-04-20 — affordances added, metric held

Cross-reference: `../crates/sprefa_parse/parse.md` (session lockfile).

The sigil-unification and parametric-rule session added framework
affordances. Every affordance carries a default that costs zero to
existing ops. The min-viable op stays at four slots; the metric
survives.

### Affordances added

| affordance | default | cost to existing op |
|---|---|---|
| `ArgValue::TermRef` | not used unless op reads TermRefs | 0 |
| `TermMode` dispatch via `OpCtx::resolve_term_modes` | op never calls it | 0 |
| `Operator::signature()` declaring `ArgSpec { accepts: AcceptsMode }` | default `Either` for all args | 0 |
| `ProgramCtx::register_invocation` | op never calls it | 0 |
| Parametric `Rule { params }` | built-in rule op handles; existing ops untouched | 0 |
| `BindingGraph` + `StageDeps` analysis | framework-computed | 0 |
| `SubscribePolicy` | defaults to Cold | 0 |
| `MutationEffect::{preview, reversible, reverse, source_range}` | all default None/false/unreachable | 0 for read-only ops; write ops opt in |
| Tag-op / link-op families as first-class | nothing — these are new ops, not framework changes | 0 |

### New op folders this session enables

Each is a single folder, `(1, 0, 0)` preserved:

| op | lane | cost |
|---|---|---|
| `is_repo`, `is_rev`, `is_fs`, `is_repo_norm`, `is_rev_norm` | tag-op family | one folder each |
| `link`, `depends_on`, `generated_from` | link-op family | one folder each |
| `retry`, `until`, `while_changes`, `when`, `if_else`, `debounce`, `distinct_by`, `merge_by_key` | higher-order control ops | one folder each |
| `filter` | binary conditionality | one folder |
| `load(term)` | capture → content dual of `> $X` capture-write | one folder |

### Anti-goals reaffirmed

- Do not fold term-annotation semantics into the base term grammar
  until the annotation lane (`../crates/sprefa_parse/parse.md` Section 14)
  resolves. Pre-commit risks baking an atom-only shape that can't
  carry kind/link/persistence directives later.
- Do not add framework match arms for tag-op or link-op kinds. Each is
  its own op with its own kind string; the relations table stores `kind`
  as a free text column.
- Do not expand `RuntimeConfig` for parametric-rule options.
  `@recursive(max_depth=N)` is per-rule attribute; subscribe policy is
  per-pipeline; neither is a cross-cutting knob.

### Framework core LoC impact

Session adds ~200 LoC to framework core (resolver's BindingGraph,
StageDeps, TermMode resolution helper, rule-param mode derivation).
`effect_proof` drill target was 126 LoC — language lockfile surface
raises the realistic framework-core target to ~400 LoC. Op authoring
surface unchanged.

### Drill amendment

Re-run `_.sprfv2.expr.plugins.audit` now also fails if:

- any op touches `ArgValue::TermRef` resolution directly (should use `ctx.resolve_term_modes`)
- any tag-op or link-op reaches into the `relations` table via raw SQL (should use the framework writer helper)
- any higher-order control op hardcodes a batching policy (should accept `SubscribePolicy` from caller if composable)

Policy: these failures are warnings, not errors, until the first
production consumer of each surface lands.
