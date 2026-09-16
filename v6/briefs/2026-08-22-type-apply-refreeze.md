# Terra lead: closed constructor application and bounded type refreeze

## Authority

The user explicitly authorized implementation on 2026-08-22 and requested that Terra lead Luna, keep both agents focused, and clear issue cards as work completes.

Read the repository `AGENTS.md`, `v6/AGENTS.md` if present, and run `boop --help` before spawning or messaging Luna. Preserve unrelated work and use issuectl for every issue mutation.

## Goal

Implement the narrow closed-constructor slice that lets compiler-time relations construct or reuse semantic type applications through relation-shaped lowering, then materializes absent applications through a bounded freeze/evaluate/discover/specialize/refreeze loop.

The semantic contract is fixed for this slice:

```text
type_apply(ConstructorTypeId, OrderedArgumentTypeIds, ApplicationTypeId)

ApplicationTypeId =
  application(ConstructorTypeId, OrderedArgumentTypeIds)
```

- Constructors are closed, named, and fixed-arity.
- The application identity reuses the existing `SemanticTypeId` representation.
- The result identity is available during the current compiler closure.
- `$type` reflection rows are immutable during one closure.
- An absent application enters a deduplicated next-construction frontier.
- Existing generic, wrapper, enum, and anonymous minting paths remain authoritative.
- Refreeze exposes generated declarations and members to the next compiler closure.
- Stable canonical type rows terminate the outer loop.
- Constructor-producing recursive SCCs receive a named refusal in this slice.
- Constructor variables, higher kinds, mixed runtime/comptime staging, unrestricted chase policies, and general schema/rule generation remain outside this slice.

`type_apply` is a provisional compiler-IR name. Preserve ordinary functional DL6 surface syntax and lower it into the relation-shaped intrinsic where appropriate. Do not add annotation sigils or a second macro language.

## Existing authority

Read these before editing:

- `chat_log/20260821.0.dl6-comptime-type-relational-macros.md`
- `plans/2026-08-18-relational-type-schema-wrappers-and-literals.md`
- `v6/plans/2026-08-20-canonical-type-row-pipeline.md`
- `plans/2026-08-19-applicative-type-annotations.md`
- issues `@semantic-type-identity`, `@compiler-type-relations`, `@type-relation-ir`, `@type-annotation-eval`, `@review-type-fixpoint`, and `@review-higher-kinds`

Current relevant code:

- `v6/prolog/0_compiler_relations.pl`
- `v6/prolog/0_generic_expand.pl`
- `v6/prolog/0_type_ids.pl`
- `v6/prolog/compile/parse_dl_dcg.pl`
- `v6/prolog/compile/test/compiler_relations.test.pl`
- `v6/prolog/compile/test/type_relation_ir.test.pl`

Reuse the landed contracts. Do not create a second type registry, second evaluator, second application identity, or parallel annotation system.

## Issue sequence

1. Use `issuectl note --decision` on `@review-type-fixpoint` to record the user ruling above.
2. Check that card's acceptance criteria only after the body accurately records frontier identity, stable-row termination, duplicate handling, recursive-construction refusal, and permission for generated types to trigger the next query round.
3. Close `@review-type-fixpoint` as done.
4. Create `@type-apply-refreeze` as the implementation card under `@comptime-type-model`, assigned to `terra`, related to the completed foundations and `@review-higher-kinds`. Its acceptance criteria must cover implementation and CI receipts.
5. Update and close the implementation card only after committed code, independent review, and current test receipts exist.
6. Leave `@review-higher-kinds` and unrelated review cards open.

Known unrelated `issuectl doctor` findings exist for an invalid `medium` priority and a timestamp ordering issue. Report them and leave them unchanged.

## Terra and Luna split

Terra owns architecture, compiler implementation, integration, issue state, and final verification.

Native agents are authorized. Terra must drive a native Luna agent for a bounded test-first task:

- Inspect the existing semantic application identity and generic-discovery path.
- Add failing tests for existing application reuse, absent application discovery/refreeze, nested construction, duplicate requests, recursive constructor refusal, and compiler-plane erasure.
- Prefer existing test files over a new file. Do not edit compiler implementation files.
- Commit only test and fixture changes with `Refs-Issue: @type-apply-refreeze`.
- Report the changed files, exact test commands, and any semantic mismatch to Terra.

Terra reviews Luna's uploaded diff before integration and keeps Luna's write scope disjoint from Terra's active implementation files.

After Terra's implementation is committed, Terra must drive a Luna review agent over the complete diff. Luna checks the fixed semantic contract, runs focused tests, and either returns bounded corrections or a clean review with receipts. Terra independently verifies any correction.

## Implementation requirements

1. Keep compiler rules function-free in their relational IR. Constructor application is an interpreted body relation or an equivalently explicit request relation.
2. Preserve the current positive safe set-fixpoint evaluator and functional conflict behavior.
3. Reuse `application(ConstructorId, OrderedArgumentIds)` identity from `0_type_ids.pl`.
4. Keep one compiler round observationally immutable over canonical type-source rows.
5. Deduplicate requests by semantic application identity.
6. Reuse current specialization/minting rather than cloning it.
7. Run an outer bounded fixpoint only when the canonical graph grows.
8. Produce named diagnostics for arity mismatch, unknown constructor, non-ground application after joins, recursive construction, and round-limit exhaustion where those conditions are reachable.
9. Keep compiler relations, source views, requests, and evidence absent from runtime relations and emitted storage.
10. Preserve compiler/oracle parity where the current suite requires it.

## Verification

Run focused tests during implementation, including:

```bash
cd v6/prolog
swipl -q -l compile/test/compiler_relations.test.pl -g run_tests -t halt
swipl -q -l compile/test/type_relation_ir.test.pl -g run_tests -t halt
swipl -q -l compile/test/annotation_surface.test.pl -g run_tests -t halt
```

Discover and run the repository's current full Prolog compiler CI command before closeout. Report only current CI results. Formatting and lint status are not CI reporting.

## Completion

- Commit implementation and issue receipts together where practical.
- Commit message must include `Refs-Issue: @type-apply-refreeze`.
- Worktree must be clean except for explicitly reported artifacts.
- Send the parent a completion hail containing commit hashes, files changed, focused and full test counts, issue statuses, and any remaining scoped refusal.
