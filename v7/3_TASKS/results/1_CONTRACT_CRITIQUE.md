# DL7 kernel contract critique

Date: 2026-08-28

Reviewed: `v7/2_DESIGN/1_MINIMAL_VERTICAL_SLICE.PLAN.md`,
`v7/3_TASKS/results/0_KERNEL_CONTRACT.md`. Donor reports opened to verify
disputed claims: `5_TYPES.md`, `6_GENERICS.md`, `7_COMPILER_FIXPOINT.md`,
`4_SCOPE.md`, `11_ORACLES.md`. Donor source read for receipts:
`v6/prolog/0_type_ids.pl`, `0_compiler_relations.pl`,
`0_generic_expand/2_compiler_plane.pl`, `0_generic_expand/5_type_freeze.pl`,
`use_resolve.pl`, `compile.pl`, `executor_modules.pl`.

No implementation file modified. No suite run.

## TOC

1. [Verdict board](#1-verdict-board)
2. [C1: the Partial proof goal cannot close](#2-c1-the-partial-proof-goal-cannot-close)
3. [C2: the inserted intern is unsafe and in the wrong position](#3-c2-the-inserted-intern-is-unsafe-and-in-the-wrong-position)
4. [C3: stage ownership is narrower than the donor's](#4-c3-stage-ownership-is-narrower-than-the-donors)
5. [C4: module-unqualified Name/Arity](#5-c4-module-unqualified-namearity)
6. [C5: the evaluator's compile-time branch](#6-c5-the-evaluators-compile-time-branch)
7. [Duplicate representations](#7-duplicate-representations)
8. [Outside the overnight ceiling](#8-outside-the-overnight-ceiling)
9. [Deletions, in order, before any addition](#9-deletions-in-order-before-any-addition)
10. [Module identity: both forms, no selection](#10-module-identity-both-forms-no-selection)
11. [Minimum additions the contract still needs](#11-minimum-additions-the-contract-still-needs)
12. [Receipt index](#12-receipt-index)

---

## 1. Verdict board

| # | Risk as briefed | Verdict | Primary receipt |
|---|---|---|---|
| C1 | (found here) `Partial` has no demand path for member types | **BLOCKING.** The one overnight proof goal derives zero rows | `2_compiler_plane.pl:341` vs PLAN:557 |
| C2 | inserted `intern/3` only for type-returning callables | **CONFIRMED, two defects.** Unsafe placement, wrong trigger, and redundant with lowered `Requests` | `2_compiler_plane.pl:192-196,232-256` |
| C3 | ownership inferred from a `primitive(type)` return | **CONFIRMED.** Donor keys on any `type` column, not the return | `0_compiler_relations.pl:31,79-80` |
| C4 | module-unqualified `Name/Arity` | **CONFIRMED.** Donor carries a name-to-module sidecar and a named collision throw; the contract has neither | `compile.pl:552-557,590-592` |
| C5 | (found here) `construction_request/3` emitted inside `evaluate/4` | **CONFIRMED.** Donor extracts requests outside the evaluator | `2_compiler_plane.pl:17-19` |
| D | duplicate identity across four relations | **CONFIRMED, five carriers not four.** See §7 | `5_type_freeze.pl:428-439` |
| X | machinery deletable before first proof | **9 items.** See §8, §9 | — |

---

## 2. C1: the Partial proof goal cannot close

This is the finding that should stop the plan, ahead of the module-identity
ruling.

The contract has exactly one way to create a new semantic identity request:
a ground `intern/3` firing (PLAN:471-473, 503-509), plus ground applications
already visible to lowering (PLAN:512-515). There is no goal that says *"I
need `Option(X)` for an `X` that evaluation just bound to me."*

The donor has that goal. `elaborate_compiler_body_argument/6` emits
`type_requested(Argument, ConstructorId, Arguments)` for a compound type term
in **body** position (`0_generic_expand/2_compiler_plane.pl:341`), separately
from the head-position `type_apply/3` (`:256`). The contract dropped
`type_requested/3` and kept only the head form.

`Partial`'s second rule (PLAN:554-558) needs exactly the dropped goal:

```lisp
(<-
  (: ?Output ?Name ?OptionalType ?Index)
  (Partial ?Input ?Output)
  (: ?Input ?Name ?MemberType ?Index)
  (Option ?MemberType ?OptionalType))
```

`?MemberType` is bound by the third goal, during evaluation. `Option/2` then
has to already hold a row for it. Trace one round over the `User` fixture at
PLAN:216-229:

```
step 0  lower_dl7/7   Requests = [construction_request(
                                    application(Partial,[User]), Partial, [User])]
                      (Partial User) is ground in source, so this row exists)
step 1  driver seeds  specialization(application(Partial,[User]), Partial, 1)
                      argument(application(Partial,[User]), 0, User)
step 2  evaluate/4    'Partial'(User, application(Partial,[User]))     rule 1, 1 row
step 3  evaluate/4    ':'(User, id,   primitive(int),  0)              seed
                      ':'(User, name, primitive(text), 1)              seed
step 4  evaluate/4    'Option'(primitive(int),  ?OptionalType)         0 rows  <-- stops
                      'Option'(primitive(text), ?OptionalType)         0 rows  <-- stops
step 5  closure       ':'(application(Partial,[User]), _, _, _)        0 rows
step 6  driver        new construction_request rows: none
step 7  driver        row set unchanged -> "semantic rows and request keys
                      stabilize" -> loop exits, reporting success
```

Termination at step 7 is the base case the contract asks for, and the answer
is empty. `application(Option,[primitive(int)])` was never requested, because
nothing in the contract can request it. The plan's own success condition
(PLAN:527) accepts this silently: an empty derivation is stable.

The three exits, for the user's choice, not mine to settle:

| exit | shape | cost |
|---|---|---|
| restore the donor goal | `requested(Result, Constructor, Arguments)` in body position, ground-on-exit, producing a request row | one evaluator goal, one lowering case |
| pre-request from source | lowering walks every member type of every ground application argument and emits requests transitively | unbounded for recursive types; the 16-round cap becomes the only guard |
| drop `Partial` from the overnight goal | prove the evaluator on a fixture with no type-valued callable | loses the reason the slice exists |

---

## 3. C2: the inserted intern is unsafe and in the wrong position

PLAN:392-395 and PLAN:622: *"For every type-returning callable rule, insert
`intern/3` before user goals."*

Three defects, each against a stated law in the same document.

### 3.1 Placement violates the contract's own groundness law

Apply the rule to `Partial`'s defining rule (PLAN:547-552):

```prolog
% after lowering, as PLAN:392-395 specifies
'Partial'(Input, Output) :-
    intern('Partial', [Input], Output),        % inserted, first
    specialization(Output, 'Partial', 1),
    argument(Output, 0, Input).
```

At the inserted goal, `Input` and `Output` are both unbound. PLAN:463-464
requires *"`Constructor` and every member of `Arguments` are ground when
`intern/3` executes"*, and PLAN:461-462 binds head variables only from a
**preceding** positive goal. The rule is refused by the evaluator the same
contract specifies. The flagship prelude rule does not survive its own
lowering pass.

The donor puts head-derived construction goals **after** the body, exactly to
avoid this: `elaborate_compiler_rule/4` at
`0_generic_expand/2_compiler_plane.pl:192-196` calls
`append_compiler_body_goals(Body1, HeadGoals, Body)`, and `:274` produces
`(Body0, Body)` with the head goals last.

### 3.2 The trigger is the callable's return, the donor's is the argument's domain

Donor `elaborate_compiler_head_argument/6`
(`2_compiler_plane.pl:232-256`) fires per **argument position whose declared
domain is `type`** and whose surface term is a compound constructor
application. It recurses into nested arguments through
`elaborate_compiler_type_term_arguments/5` (`:260-268`).

Under the contract's per-rule trigger, `(Partial (Pick User 'id))` produces
one inserted goal for the outer constructor and none for the inner one. The
inner application is never interned, never requested, never specialized.

### 3.3 It duplicates the lowered request

PLAN:512-515 already emits `construction_request/3` into `Requests` for
"ground type-returning applications visible before evaluation" — which is
every application the inserted goal could ground, because the inserted goal
can only run when its arguments are already ground. The two paths carry the
same row. One of them is dead.

**Ruling.** The inserted-interning rule as written is wrong on trigger,
position, and necessity. It should be deleted outright, and the demand
problem in C1 solved with a body-position goal instead.

---

## 4. C3: stage ownership is narrower than the donor's

PLAN:641-645: *"colon, class, callable, specialization, argument,
construction_request, and a callable returning `primitive(type)` are
compiler-owned; other callables are runtime-owned."*

Donor law, verbatim at `v6/prolog/0_compiler_relations.pl:31-32`:

> A relation is compiler-plane when a declared column has the `type` value
> domain.

Implementation, `0_compiler_relations.pl:79-80`:

```prolog
compiler_relation_columns(_, Columns) :-
    member(_-type, Columns).
```

Any column. Not the last one.

| callable | donor plane | contract plane | consequence |
|---|---|---|---|
| `(-> (* (: source type)) type)` | compiler | compiler | agree |
| `(-> (* (: source type)) text)` | compiler | **runtime** | a runtime relation whose column holds a semantic identity that is erased before `RuntimeProgram` (PLAN:650) |
| `(-> (* (: n int)) int)` | runtime | runtime | agree |

Row 2 is a real program: a `NameOf` or `ArityOf` callable, type in, scalar
out. The contract routes it to the runtime plane, where its input domain no
longer exists.

Second gap: the donor refuses mixed-plane rules by name
(`0_compiler_relations.pl:130` block, `partition_rules/4` at `:259`, cited in
`7_COMPILER_FIXPOINT.md` §2). The contract states a partition and no refusal.
A runtime rule reading `':'/4` is silently wrong instead of a named diagnostic.

**Ruling.** Replace the return-position heuristic with the donor's
any-`type`-column rule, or declare the stage explicitly on the callable. Add
the mixed-plane refusal either way. The plan's own line PLAN:644 already
enumerates six compiler-owned relation names by hand; that hand-list is the
tell that the inferred rule does not cover them.

---

## 5. C4: module-unqualified Name/Arity

PLAN:356-360 pins `callable(Callable, 'Identity'/2, 1)` with functional keys
on both `Callable` and `RelationRef`, and PLAN:373 lowers a call to the bare
compound `'Identity'(1, Result)`. The rule IR after lowering carries no module
term anywhere (PLAN:380: *"After lowering there is no call or application
wrapper in the rule IR"*).

The first slice compiles **two** modules: the source file and
`v7/4_PRELUDE/0_types.dl7` (PLAN:638-640). So the collision is reachable in
the smallest program the plan describes: a source module declaring `Option`
shadows the prelude's, and the two `named/3` identities stay distinct while
their relation atoms unify.

Donor behavior, three receipts:

| donor mechanism | file:line | effect |
|---|---|---|
| name-to-module sidecar row | `use_resolve.pl:194-197` `rel_module_decl(Name, Hash)` | every source relation name carries its declaring module hash |
| collision throw | `compile.pl:590-592` `rel_module_identity_collision(Name, Hashes)` | two modules declaring one name is a named compile error |
| import rename | `executor_modules.pl:84-86,120-145` `rename_term/3` | a rel name reaches a term in exactly four shapes and is rewritten in all four before merge; an unaliased leaf collision throws `ambiguous_executor_leaf` (`:108`) |

The stated intent behind the throw, `compile.pl:552-556`:

> A same Ref declared by separate modules is already one runtime relation
> before lowering, so refuse it rather than inventing two SQLite tables that
> the runtime cannot distinguish.

**Ruling.** `Name/Arity` is not a sound relation reference in a two-module
program. Pick one of: carry the module identity in the reference
(`callable(Callable, module_ref(ModuleIdentity, Name, Arity), N)`), mangle at
lowering the way the donor renames, or add the donor's collision throw and
accept a flat namespace. The third is the cheapest and is enough for the
overnight slice; it is not enough once imports arrive. Note this choice is
downstream of the module-identity ruling in §10 and should be settled with it.

---

## 6. C5: the evaluator's compile-time branch

The acceptance criterion asks for an evaluator branch that makes compile time
and runtime mechanically different. There is one. It is not an `if`.

PLAN:436-437, inside the `evaluate/4` pseudocode:

> Let `intern(Constructor, Arguments, Result)` return the canonical structural
> ID **and add one ground `construction_request/3` row to the active row set**.

PLAN:444 then claims *"The evaluator contains no compile-time or runtime
branch"*, and PLAN:696-697 confirms *"the same `intern/3` clause applies if a
runtime rule contains one"*.

Consequences at a runtime call:

1. `RuntimeClosure` gains `construction_request/3` rows nobody drains.
   PLAN:742 calls that row family *"one compile request loop; removed before
   `RuntimeProgram`"* — a lifetime the runtime call cannot honor, because
   the runtime call has no request loop.
2. PLAN:716 declares `construction_request/3` key `[[1,2]]` in the kernel key
   table. A runtime program that never declares the relation now has a kernel
   key validated against rows it did not ask for.
3. The two calls therefore differ in output row families, from one predicate
   body, with the difference paid by the caller instead of a flag. The
   mechanical difference is real; only its spelling was removed.

Donor placement: `elaborate_and_erase_compiler_relations/5` calls
`evaluate_compiler_relations/3` (`2_compiler_plane.pl:17`) and **then**
`compiler_type_apply_requests/3` (`:19`) as a separate pass over the closed
rows. `compiler_type_apply_requests/3` lives at
`0_compiler_relations.pl:393-401` and re-satisfies bodies non-tabled, outside
the closure. The evaluator emits no request rows.

**Ruling.** Move request extraction out of `evaluate/4` into the driver, over
`Closure`. `intern/3` then returns only its `Result`, the evaluator's output
is phase-free in fact and not just in wording, and `construction_request/3`
leaves the kernel key table.

---

## 7. Duplicate representations

Acceptance criterion 1. The brief named four carriers; there are five for
construction, plus five more duplicates elsewhere.

### 7.1 One construction fact, five carriers

| carrier | plan line | information |
|---|---|---|
| `application(Constructor, Arguments)` | 486 | Constructor, Arguments, structurally |
| `intern/3` row, key `(Constructor, Arguments)` | 741 | Constructor, Arguments |
| `construction_request/3`, key `(Constructor, Arguments)` | 742 | Constructor, Arguments |
| `specialization(Result, Constructor, Arity)` | 743 | Constructor, and `Arity` |
| `argument(Result, Index, Value)` | 744 | Arguments, one per row |

`intern/3` and `construction_request/3` have the **same declared key and the
same determined third column**, both are transport, both are erased at
PLAN:650. That is one relation written twice.

`Arity` in `specialization/3` is derivable from `Result` itself. Donor
receipt: `validate_type_application_arguments/3` at
`0_generic_expand/5_type_freeze.pl:135-137` computes it with
`AppId = application(_, ExpectedArgs), length(ExpectedArgs, ExpectedArity)`.
The donor's own row is `application(Id, Constructor)` — two columns, no arity
(`5_type_freeze.pl:431`).

The driver's `intern/3` verification step (PLAN:524, report line 33) cannot
fail: it checks that the third column of
`construction_request(application(C,A), C, A)` unifies with
`application(C,A)`. Delete the step.

### 7.2 Elsewhere

| duplicated value | carriers | plan lines |
|---|---|---|
| callable input count | `callable/3` arg 3; the `Arity` in `Name/Arity` (= N+1); the `return` edge's `Index` | 356-361 |
| callable name | `named(M, relation, 'Identity')` and the unqualified `'Identity'` in `Name/Arity` | 351, 356 |
| source path | inside `reader_node(Path, Index)` and again as `source/8` arg 2 | 147, 152 |
| variable name | inside `variable(VariableId, Name)` where `VariableId = variable(TopNodeId, Name)` | 143, 155 |
| span | byte offsets **and** line/column, both in `source/8` | 147-148, 159-160 |

`class/2`'s declared key `[[0,1]]` (PLAN:711) covers all columns of a 2-ary
relation, which PLAN:721-722 says complete-row set identity already provides.
The key is a no-op.

### 7.3 A donor mismatch worth pinning before the oracle is written

`argument/3` indices are zero-based (PLAN:744). The donor's `argument/4` rows
are built with `nth1/3` (`5_type_freeze.pl:433`), one-based. Either base is
fine; the snapshot test grades whichever is chosen, so the choice must be
written down before `0_kernel.test.pl` exists, not discovered by it.

---

## 8. Outside the overnight ceiling

Ceiling is PLAN:80-90. Measured against the one fixture and the one proof goal
(`Partial`, PLAN:594).

| # | Feature | Plan line | Why it is outside |
|---|---|---|---|
| X1 | stratified negation: dependency strata, lower-stratum `not/1`, strict-cycle diagnostics | 432-434, 458-460 | `Partial`'s two rules use no negation. Only Pick and Exclude do, and PLAN:594-596 defers both to separate cards |
| X2 | `(+ Binding...)` sum form | 169 | in the reader's kernel spellings with **zero** lowering contract anywhere in the document: no variant row, no discriminator, no `class` value |
| X3 | `class/2` | 208, 227, 353, 614 | written by lowering, read by no rule, goal, or law in the slice. Write-only |
| X4 | `callable/3`'s second functional key on `RelationRef` | 359-360 | unsound as written (§5) and unexercised by one fixture |
| X5 | float literals, `\r`/`\t`/`\\`/`\"` escapes, negative decimals | 123-131 | the fixture needs atoms, `?vars`, and forms. Each literal kind is a reader branch plus an oracle row |
| X6 | line/column half of `source/8` | 147-148 | derivable from offsets plus text; doubles every span row in the snapshot |
| X7 | `scope_parent/2` chain traversal | 323-327 | the slice has file module plus nested product: depth 1. A chain walk is not exercised |
| X8 | 8 declared key sets | 709-719 | `Partial` exercises `':'/4`, `specialization/3`, `argument/3`. Five of eight are unexercised, one is a no-op (§7.2) |
| X9 | compile-twice equality inside the one test | 784 | a second determinism property riding in the same test as the first correctness proof; makes a failure ambiguous |

X1 is the large one: strata, relaxation, and the negation-cycle diagnostics
are the biggest single block of evaluator code in the contract and prove
nothing the overnight goal needs.

### Donor attribution to correct

The report's receipt table (`0_KERNEL_CONTRACT.md:115`) credits `11_ORACLES.md`
with *"compile-twice cleanup determinism"*. `11_ORACLES.md` states no such
law; the cleanup-determinism receipts are `setup_call_cleanup/3` and the
per-`EvalId` table namespace in `7_COMPILER_FIXPOINT.md` §5
(`0_compiler_relations.pl:438-449`). Separately, `11_ORACLES.md:192-232`
proposes a **38-file, 8-load-bearing** minimal parity corpus; it does not
support a one-fixture ceiling at the strength the plan cites it. The ceiling
may still be right for one overnight slice. The citation is not.

---

## 9. Deletions, in order, before any addition

Acceptance criterion 4. Each line removes machinery the first proof does not
need, and none of them is blocked by the module-identity ruling.

| order | delete | plan lines | reclaims |
|---|---|---|---|
| 1 | the inserted `intern/3` lowering rule | 392-395, 622 | §3, all three defects |
| 2 | `construction_request/3` as a relation; keep `intern/3` rows only | 503-515, 716, 742 | §7.1 first duplicate |
| 3 | the driver's `intern/3` verification step | 524 | a step that cannot fail |
| 4 | `Arity` from `specialization/3` | 743 | derivable, §7.1 |
| 5 | request emission from inside `evaluate/4`; move to a driver pass over `Closure` | 436-437 | §6, the phase difference |
| 6 | `class/2` | 208-227, 353, 614, 711, 732 | X3 |
| 7 | `not/1`, strata, and cycle diagnostics from the first evaluator | 432-434, 458-460 | X1, the largest block |
| 8 | `(+ ...)` from the reader until its lowering is written | 169 | X2 |
| 9 | line/column columns of `source/8`; keep offsets | 147-148 | X6, halves the span snapshot |

After 1 through 9 the contract still states: one reader tree, one colon edge
with two keys, one callable shape, one saturating application lowering, one
`intern/3`, one tabled positive closure, one request loop in the driver, and
one evaluator body. That is the claim the overnight slice exists to prove.

---

## 10. Module identity: both forms, no selection

Held open per the brief. Both rows below are complete; neither is
recommended.

### 10.1 Side by side

| dimension | A: donor module hash | B: structural module path |
|---|---|---|
| identity term | `named(ModuleHash, Kind, Name)`, arg 1 a **16-hex atom** | `named(module(ModulePath), Kind, Name)`, arg 1 a **compound over a segment list** |
| owner term | `module(ModuleHash)` | `module(ModulePath)` |
| worked example, `lib/user.dl7` under entry dir | `named('3f2a9c1b7e4d0856', relation, 'User')` | `named(module([lib, user]), relation, 'User')` |
| required inputs | entry base dir, absolute path, extension strip, `/` join, SHA-256, first 8 bytes | entry base dir, absolute path, extension strip, split to atom segments |
| donor implementation | `use_resolve.pl:396-398` `module_hash/3` -> `module_stem/3` (`:386-394`) -> `short_hash/2` (`:405-414`) | none; new code |
| collision behavior | 64-bit truncation aliases two modules silently. Needs an added `(ModuleHash, ModuleStem)` guard | structural equality; distinct normalized segment lists cannot alias |
| donor collision receipt | `plunit_tests.pl:9495-9511` is a FAIL-FIRST receipt for the **basename** bug, not for digest truncation. Truncation is unguarded today | n/a |
| relocating the checkout | identity preserved | identity preserved |
| renaming the module | every declared ID in the module changes | every declared ID in the module changes |
| artifact encoding | `semantic_type_id_encoding(named(M,K,N), _)` calls `atom_encoding/2` (`0_type_ids.pl:54-56`), which calls `atom_string/2` (`:145-150`). **Works unchanged** | `atom_string/2` on a compound is not an atom. The `named/3` encoding clause needs a new case. `path_encoding/2` (`0_type_ids.pl:110-117`) already length-prefixes an atom list, so the case is small but it is a donor edit |
| cross-implementation portability | needs identical relative-path, separator, Unicode, extension normalization, **plus** identical digest and truncation | needs identical relative-path, separator, Unicode, dot-segment, extension normalization. No digest to agree on |
| readability in diagnostics and snapshots | opaque; the stem must be carried separately to name the module in an error | self-describing; the error text is the path |
| size in every row | 16 chars, fixed | grows with path depth; appears in every `named/3`, every `':'/4` owner, every `application/2`, every snapshot row |
| downstream signature effect | `lower_dl7/7` arg 1 must become a resolved hash, or gain a second module-identity argument | `lower_dl7/7` arg 1 stays a path-shaped term |

### 10.2 What the choice reaches, either way

Unchanged: `read_dl7/5`, `evaluate/4` (PLAN:307-309).

Changed: `compile_dl7/4` module derivation, `lower_dl7/7` arg 1, every
`named/3`, every module-owner `':'/4`, every callable constructor, every
`application/2`, every `construction_request/3`, `CompilerRows`, the
compile-twice snapshot, and the later ProgramJson projection (PLAN:296-306).

### 10.3 A third form the plan does not name

Option A puts a bare atom inside `named/3` but wraps it in `module/1` for the
owner (PLAN:241-243). Option B wraps in both places (PLAN:271-274). A bare
segment list inside `named/3` with `module/1` only at the owner is a third
coherent form, structurally equal like B and one term shallower. Named here
only so the user's choice is over the real set.

---

## 11. Minimum additions the contract still needs

Additions come after §9's deletions, and only these.

| # | Add | Because |
|---|---|---|
| A1 | a body-position demand goal, `requested(Result, Constructor, Arguments)`, that produces a request row when its arguments become ground | C1. Without it `Partial` derives nothing. Donor: `2_compiler_plane.pl:341` |
| A2 | an explicit stage word on a callable declaration, or the donor's any-`type`-column rule | C3. The contract's inferred rule needs a hand-list to work |
| A3 | the mixed-plane refusal | C3. Donor: `0_compiler_relations.pl:259` `partition_rules/4` |
| A4 | a module term in the relation reference, or the donor's `rel_module_identity_collision` throw | C4. Reachable in a two-module slice |
| A5 | the `Option` callable's `.dl7` text in the prelude | PLAN:109 names it, PLAN:557 depends on it, and the plan never writes it |
| A6 | the argument index base, written down | §7.3. The oracle grades it either way |

---

## 12. Receipt index

| claim | receipt |
|---|---|
| body-position type demand exists in the donor | `v6/prolog/0_generic_expand/2_compiler_plane.pl:341` |
| head-position construction goals run after the body | `2_compiler_plane.pl:192-196`, `:271-274` |
| construction goal fires per type-domain argument, recursively | `2_compiler_plane.pl:232-256`, `:260-268` |
| requests are extracted outside the evaluator | `2_compiler_plane.pl:17-19`; `0_compiler_relations.pl:393-401` |
| compiler plane = any column with `type` domain | `0_compiler_relations.pl:31-32`, `:79-80` |
| mixed-plane rules are a named refusal | `0_compiler_relations.pl:259`; `7_COMPILER_FIXPOINT.md` §2 |
| recursive construction refusal shape | `0_compiler_relations.pl:369-377` |
| per-`EvalId` table namespace and cleanup | `0_compiler_relations.pl:438-449` |
| refreeze cap 16 and its stability test | `0_generic_expand/0_expand.pl:18-38`; `6_GENERICS.md` §2 |
| donor application row is 2-ary, arity derived not stored | `0_generic_expand/5_type_freeze.pl:431`, `:135-137` |
| donor argument rows are 1-based | `5_type_freeze.pl:433` |
| `named/3` identity constructor | `v6/prolog/0_type_ids.pl:18` |
| `named/3` artifact encoding needs an atom module | `0_type_ids.pl:54-56`, `:145-150` |
| list encoding already exists for a path | `0_type_ids.pl:110-117` |
| `module_hash/3` and 8-byte truncation | `use_resolve.pl:396-398`, `:405-414` |
| module stem construction | `use_resolve.pl:386-394` |
| name-to-module sidecar rows | `use_resolve.pl:194-197` |
| cross-module name collision throw | `compile.pl:552-557`, `:590-592` |
| import rename and ambiguous-leaf refusal | `executor_modules.pl:84-86`, `:108`, `:120-149` |
| basename-collision fail-first receipt | `compile/test/plunit_tests.pl:9495-9511` |
| donor minimal parity corpus is 38 files | `v7/1_AUDIT/results/11_ORACLES.md:192-232` |
