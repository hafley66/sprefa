---
name: project-type-ir-value-space-plan
description: "type-IR DRAFT plan in sprefa-types worktree; 3 fork picks under value-space + callable-Value constraints; awaits user veto/lock"
metadata:
  node_type: memory
  type: project
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

`sprefa-types/plans/2026-05-19-type-ir-value-space.md` (DRAFT, branch
`feat/type-ir-value-space` @ base main 9bd1fad6). Operationalizes the
[[project_types_in_value_space]] ruling against the 3 open forks from
chat_log/20260518.0 (sprf-type-ir-macro-design).

3 fork picks (RATIONALE inline in plan; user veto path noted):

- **D-1 (the collapse)**: types ARE rules; reuse `Callable(Rule "Foo")`
  from [[project-callable-value]] as the value-space anchor. **ZERO new
  ValueKind variant.** Field projection = callable-Value H4 dot arm.
  Apply (membership/predicate) = callable-Value apply. Variants (sum
  types) = cons-step-6 merge fan-out. Generics = rule apply with t-arg.
  Bounds = first-class Callable refs (D-3 promote).
- **D-2 (fork #2)**: structured `tref` rows; `head` = Callable Value,
  `args` = ConsList<REF_ID>. Enables "every type mentioning Vec"
  queries. Marries value-space anchor with relational queryability.
- **D-3 (fork #3)**: meta = vis / lifetimes / decorators / conditional-TS
  / macro-gen. PROMOTE generic bounds out of meta to first-class
  Callable refs (preserves query power; "every type bounded by
  Iterator" stays queryable).

Macro shape: `use(:Rust, "path/to/src.rs")` lower-time CST splice. Emits
`rule(:Foo, F?)` synthetic decls + 5 tables (ty, fld, tref, bound, meta).
First adapter = rustdoc-JSON (self-dogfood pivot per chat_log/20260518.0).

8-step build order; steps 1-7 independent of cons-plan progress; step 8
marry-up with cons-step-5 D-TY (cells with `ty: Callable(Rule "Foo")`).

3 open Qs for user when fresh:
1. tref REF_ID stability across re-ingest (re-intern fresh per call vs
   persistent table)?
2. Synthesized rule name hygiene (`:a_rs::Foo` namespacing vs flat
   `:Foo`)?
3. First-language adapter rustdoc-JSON (self-dogfood) vs JSON Schema +
   quicktype (polyglot lossy)?

Precondition: feat/callable-value (c79c47f8, GREEN unmerged) MUST merge
before step 0 (D-1 reuses its Callable arm). Step 8 also blocked on
cons-plan steps 4 + 5 landing.

Resume path when user fresh: read type plan first (lower stakes, forks
to lock), THEN consider [[project-host-lsp-trait-architecture]] v2
patch (higher energy, 6 FATAL to resolve). Type plan deliberately does
not depend on host-LSP v2.
