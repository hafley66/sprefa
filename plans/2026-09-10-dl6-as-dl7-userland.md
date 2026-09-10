# DL6 as the first DL7 userland application

## Context

Chris requests resuming the DL6 relational/database experience as an application
of the DL7 Lisp-shaped relational compiler, then using the same emitter-facing
representation for TypeSpec-like generation and eventually Rust. Main starts at
8aba008df. SQLite IVM is already an independent plugin under sqlite_ivm/, pushed
to main and smoke-tested for CRUD, rollback, reopening and fresh writers.

The authority is existing DL7 nodes, ordered : edges, type/TSI facts and rules.
Userland defines concepts and derives layouts/algebra; emitters independently
target SQLite IVM, DD and later generated Rust. Existing TypeSpec/extract work
remains preserved. Legacy Rust engine structs impose no userland requirements.

## Decisions

- Save this session's plans and reports on main, then implement on an isolated
  branch/worktree and publish a PR for Chris. Do not merge the implementation.
- Implement as far as existing contracts permit. No changes to kernel/type
  semantics, functional-key enforcement, binding, evaluation, macrotime/comptime
  boundaries, or new Prolog-hosted facilities without explicit user review.
  Document blocked seams and continue independent authorized work.
- Every next implementation move gets an Opus sidecar challenge: demonstrate
  existing mechanisms cannot express the needed information before adding any
  schema, IR, hosted operation or bespoke adapter.
- Begin with an Interned(text TYPE) userland constructor/annotation. A compiler
  application identity and a runtime persistent dictionary ID have different
  lifetimes. Do not implement InternedText(raw string) as a substitute.
- Reuse Key/composite_key, existing storage_* rows and public program graph.
  Do not introduce a duplicate graph or PersistentStoragePlanEmitter/StoragePlan.
- Preserve plain text when explicitly selecting an Interned field. Preserve
  existing policy-wide dictionary behavior in existing fixtures. Assert exactly
  one storage representation per field; scalar fallback must not double-claim.
- Key annotation output is metadata today. Merely deriving program_key cannot
  change the checker relation KeySets. No claim of enforced identity from that.
- Option redesign is withdrawn. Do not give Option a new sum shape in this arc.
- Rust generation and plugin integration are eventual consumers. Immediate work
  stays on authoring types, reading them at comptime, and reusable userland data.
- Preserve other work, including primary v6/sprefa-extract/src/types.rs edits,
  raw .recovery evidence, old worktrees and stashes. No cleanup authorization.

## Signatures, lifetime and storage

Existing kernel: intern(+Constructor,+Arguments,-Identity), cons/3, :/4 and
edge_snapshot/4. Intern builds application(Constructor,Arguments) symbolically.
Proposed userland: Interned(+LogicalType,-SpecializedType), using the existing
constructor convention. Two owners selecting the same specialization share it.
Fields retain authored owner/label/position; target type exposes storage intent.

Compiler rounds freeze edge/intern snapshots, evaluate userland, then discard
snapshot rows before publishing CompilerFacts. Existing emit_compiled seeds
from CompilerFacts as well as logical/runtime rows. Test the actual handoff;
discarded snapshots alone do not prove already-derived outputs disappear.

Physical interning, when reached, requires exact value equality, explicit
identity domain, child-before-parent dependencies, caller transaction ownership,
reopen stability and batched operations. No hash-only equality or per-value SQL.
Dictionary equality still needs an explicit lookup strategy; integer references
do not eliminate that requirement. No new physical index policy by assumption.

## Sequence and gates

1. Reproduce ordinary emitter handoff on existing storage fixture. Capture exact
   rows and diagnostics for full versus compiler-closed views. No phase changes.
2. Add a userland Interned type and focused source fixture. Test plain field,
   application-typed field, named alias with Key label, and repeated specialization
   across owners. If syntax needs a lowerer/binding change, yield on that case.
3. Derive storage selection and key metadata with existing relations and graph
   operations. Pin one row per field and preservation of existing policies.
4. Expand the DL6 userland application with independently verified concepts:
   products/references, constructor identity, explicit key metadata, type-directed
   storage/projection and target-neutral rule/operator facts. Each addition must
   have an authored source example and exact comptime outputs.
5. Publish capability/parity receipt: working DL6 concept, actual DL7 expression,
   proof command, and missing consumer/contract. Never label the whole DL6
   experience complete based on a fixture or schema alone.

<!-- todo(feature): Implement DL6-compatible authoring and type-directed storage behavior using existing DL7 userland primitives, with executable examples. -->
<!-- todo(decision): Yield before any kernel, binding, phase, enforced-key or new hosted-facility change; record the exact failing example. -->

## Verification

Exact assertions for identities, field order and target, plain versus interned
selection, domains, single-row layouts, key metadata and artifact parity. Existing
baseline: three selected entrypoint tests passed; the existing storage test passed
in the Opus read-through. Run focused current gates, then affected entrypoint,
storage, emitter and module tests. Add CI coverage for new userland behavior.
No repository-wide repeated builds or unrelated code generation.

## Staffing

Coordinator codex-2344 owns checkpoint, boundaries, integration, review and PR.
Implementation and independent challenge use Claude Opus 5 medium via Boop's
Claude harness only. No native Codex/Sol or OpenCode agents; no worker delegation.
Each lane owns its worktree and bounded files, sends ACK/evidence/final hails,
and stops at its assigned gate. Base SHA is the main documentation checkpoint.
Budgets are one focused proof then bounded implementation, followed by review;
suite runs are selected by touched behavior. No silent expansion.

## End state for this PR

A reviewable userland implementation and reproducible demonstrations progressing
toward DL6 on DL7, with source authority reused, no unauthorized kernel changes,
and honest blocked gates. PR includes a reading-order map, before/after examples,
current test commands/results, and the remaining path to SQLite IVM/DD/Rust.
