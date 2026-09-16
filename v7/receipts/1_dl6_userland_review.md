# DL6-on-DL7 userland review

Independent challenge and parity lane `chore-dl6-userland-review-20260910`.
Read-only over the repository. Every probe ran out-of-repo under `/tmp/dl6probe`
with the DL7 project root pointed there, so no worktree source was authored.

## Contents

- [Baseline](#baseline)
- [Task A: the minimal userland Interned(text TYPE)](#task-a-the-minimal-userland-internedtext-type)
- [Task A findings](#task-a-findings)
- [Task A constraints on the implementation lane](#task-a-constraints-on-the-implementation-lane)
- [Task B: DL6 to DL7 capability and parity map](#task-b-dl6-to-dl7-capability-and-parity-map)
- [Decisions this map surfaces](#decisions-this-map-surfaces)
- [What this map does not say](#what-this-map-does-not-say)

## Baseline

| gate | command | result |
|---|---|---|
| interned storage | `swipl -q -s v7/test/15_interned_storage.test.pl -g run_tests -t halt` | passed, 26.131 sec |

## Task A: the minimal userland Interned(text TYPE)

The smallest thing that compiles is a declaration plus one rule on the existing
`nil` / `cons` / `intern` convention. No kernel change, no new schema, no new
prelude row.

```lisp
(: Interned
   (* (: source type)
      (: return type)))

(<- (Interned ?Source ?Result)
    (nil ?Empty)
    (cons ?Source ?Empty ?Arguments)
    (intern Interned ?Arguments ?Result))
```

Authoring then reads as `(: kind (Interned text))` and the `:` edge target is
`application(Interned, [primitive(text)])`.

### Necessity, measured rather than assumed

| variant | receipt | diagnostics |
|---|---|---|
| declaration alone | `/tmp/dl6probe/min.dl7` | `missing_derived_bind(Node, kind, 1)` |
| declaration + the one `intern` rule | `/tmp/dl6probe/min2.dl7` | `[]` |

The `intern_snapshot` mirror rule, `(node ?Result)` and `(product ?Result)` are
NOT required for authoring. Any of them needs its own failing example first.

### Specialization sharing is free

`Node.kind` and `Other.kind2`, declared in different owners, both carry the
identical target term `application(Interned, [primitive(text)])`. `Other.kind3`
declared `(Interned int)` carries the distinct `application(Interned,
[primitive(int)])`. Two owners selecting the same specialization share it with
no dedup mechanism. Receipt: `/tmp/dl6probe/min2.dl7`.

## Task A findings

### F1. An Interned field gets the wrong layout today, silently

Against `v7/emitters/2_interned_storage.dl7` unmodified, `(: kind (Interned
text))` produces exactly one `fields` row, through the scalar fallback at
`v7/emitters/2_interned_storage.dl7:170-176`:

```
[Policy, Node, kind, 1, app(Interned,[text]), "scalar", app(Interned,[text])]
```

while a plain `(: note text)` correctly resolves to `"dictionary-local-id"` ->
`SharedTextDictionary`. The annotation meaning "intern me" selects the one
representation that is not interned, the storage domain is an opaque application
node, and no `dictionaries` or `dependencies` row is produced for the field.

Cause: `storage_policy_dictionary` is keyed on `primitive(text)`, and no
`product`, `sum` or dictionary-target predicate holds for the application, so
all three negations in the fallback succeed. Verified by a `findall` over
CompilerFacts: predicates over applications = `[]`.

The parent brief's "may double-match scalar fallback" is not what happens as-is.
Today it is one wrong row.

### F2. The double-match appears the moment the wrapper is honoured

Adding a capability-discovered wrapper rule to an emitter copy, with no kernel
change, compiles with zero diagnostics and yields two rows for one field:

```
[Policy, Node, kind, 1, primitive(text),      "dictionary-local-id", SharedTextDictionary]
[Policy, Node, kind, 1, app(Interned,[text]), "scalar",              app(Interned,[text])]
```

One field, one position, two layouts. Receipts: `/tmp/dl6probe/emitter2.dl7`,
`probe3.dl7`, `probe3.pl`.

### F3. A pre-existing defect on main, independent of this arc

The shipped emitter already emits two layouts for one field with no `Interned`
anywhere. Plain-select an `int` field:

```
[Policy, Node, start, 1, primitive(int), "inline", primitive(int)]
[Policy, Node, start, 1, primitive(int), "scalar", primitive(int)]
```

Zero compile diagnostics, zero emit diagnostics. Receipt:
`/tmp/dl6probe/probe8.dl7`.

Cause: the scalar fallback at `v7/emitters/2_interned_storage.dl7:170-176`
carries `(not (product ...))`, `(not (sum ...))` and `(not
(storage_dictionary_target ...))` but not `(not (storage_plain_field ...))`.
Every other layout rule in that file carries the plain guard.

`v7/test/15_interned_storage.test.pl` stays green because its only
plain-selected field is `Node.kind : text`, and `storage_dictionary_target(Policy,
text)` holds from `TextDictionarySelection`, so the fallback is blocked by
coincidence rather than by design. Plain-select any field whose type is not a
policy dictionary target and the second row appears.

The fix is one line of userland DL7: add `(not (storage_plain_field ?Policy
?Owner ?Label))` to that rule. It should land whether or not the `Interned` arc
does, and it needs a count assertion.

### F4. Key plus an expression target is blocked, and the block is kernel work

| case | diagnostic | throw site |
|---|---|---|
| `(: (Key "slug" Opts) (Interned text))` | `unsupported_bind_target` | `v7/src/2_comptime/0_lowerer.pl:493`, reached from `:655` |
| `(: Holder (* (: patch UserPatch)))` where `(: UserPatch (Partial User))` | `unresolved_name(patch)` | `v7/src/2_comptime/1_checker.pl:267`, via `:635` / `:654` |

`lower_edge_bind/5` at `v7/src/2_comptime/0_lowerer.pl:650-665` routes an ATOM
label through `lower_bind/5`, which consults `expression_bind_target/1` at
`:425-429` and accepts a form target. A COMPOUND label at `:654-655` calls
`lower_target/4` directly and skips that branch. `lower_target/4` has clauses
only for `(* ..)`, `(+ ..)`, `(Host ..)`, atom, `()` and literal, so any other
form falls to `:492`.

The obvious workaround, naming the expression first and referencing the name, is
broken too, and not only for `Interned`: the second row above uses the prelude's
own `Partial`. Receipt: `/tmp/dl6probe/iso.dl7`. An expression-alias is
declarable but not referenceable as a field target at all.
`v7/test/fixtures/2_partial.dl7:21-22` declares `UserPatch` and `MaybePatch` and
never references them as targets, which is why this has stayed invisible.

Both fixes are lowerer or checker semantics, so `AGENTS.md:16-28` puts them with
the user. `Key` plus `Interned` on one field is out of scope for this arc.

## Task A constraints on the implementation lane

| id | constraint |
|---|---|
| C1 | Add nothing beyond the declaration and the one `intern` rule. Any extra prelude row, `node`/`product` fact or IR needs its own failing example first. |
| C2 | Add the `storage_plain_field` guard to the scalar fallback regardless of the wrapper work, and a wrapped-target guard if a wrapper rule is added. Two guards, two count tests. |
| C3 | The emitter cannot call the application module's `Interned` relation across modules; that is `undeclared_relation('Interned')`, measured. Reach the wrapper through the capability selection protocol the fixture already uses for roots and dictionaries, or through kernel `intern_snapshot`. No third discovery path. |
| C4 | `storage_reachable` at `2_interned_storage.dl7:131-141` requires `(product ?Child)` on the raw edge target, so an `(Interned SomeProduct)` field drops that child from the closure silently. Restrict the wrapper source to non-product/non-sum and test the rejection, or prove reachability. |
| C5 | Existing policy dictionaries are not broken by any of the above; `url` and `note` resolved to `dictionary-local-id` in every probe. Keep test 15's expectations unchanged. |
| C6 | Assert counts per `(owner, label, position)`, not only `exact_rows/2`. `exact_rows/2` sorts both sides and cannot catch a duplicate the expected list also contains, and the `length/2` checks at `v7/test/15_interned_storage.test.pl:118-123` are whole-artifact totals a compensating change can satisfy. |
| C7 | Do not author `(: (Key ...) (Interned ...))` or an alias reference in any fixture. Both fail today with the diagnostics in F4. |

## Task B: DL6 to DL7 capability and parity map

Status words: **implemented** = authored, derived and asserted by a test;
**metadata-only** = rows are derived but nothing enforces or consumes them;
**partial** = a proper subset of the DL6 concept; **missing** = no DL7 form.

| DL6 concept | DL6 evidence | DL7 equivalent and proof | status | smallest in-scope next proof |
|---|---|---|---|---|
| relation authoring | `rel name(col: type, ...).` — `v6/prolog/compile/SYNTAX.md:229-231` | `(: Name (* (: col type) ...))` mints node, `product` classifier and `relation(Owner, Arity, KeySets)` at `v7/src/2_comptime/0_lowerer.pl:596-600`; fixture `v7/test/fixtures/16_interned_storage.dl7:4-33`; test 15 green | implemented | none |
| products and sums | struct types only: `type_decl(Name, [col(..)])`, `SYNTAX.md:231`; no sum row in that table | `(* ..)` and `(+ ..)` both lower at `v7/src/2_comptime/0_lowerer.pl:460-471`; sum carried end-to-end through storage by `v7/test/fixtures/16_interned_storage.dl7:42-47` and asserted `"sum"` at `v7/test/15_interned_storage.test.pl:209-210` | implemented, DL7 ahead of DL6 | none |
| keys | `key(P, P, ...)` -> `keyed(Ref, Positions)` declaration modifier, `SYNTAX.md:235`; arrival `key(..)` -> `arrival_identity/2` | `Key("name", Options)` compound label, `composite_key/4` derived at `v7/prelude/3_derived_rules.dl7:72-74`. The only KeySet a user product ever gets is minted from a field literally named `return` (`v7/src/2_comptime/0_lowerer.pl:605-613`); `program_key` reifies from KeySets only (`v7/src/3_emit/0_logical_program_reifier.pl:191-202`); `validate_functional_rows/3` enforces KeySets (`v7/src/1_libtime/0_evaluator.pl:188-227`) | **metadata-only** | a fixture whose `Key`-annotated field takes two conflicting rows, asserting NO diagnostic is raised, so metadata-only is pinned before anyone claims enforcement |
| reference identities | ref column stores the dictionary id and renders the value at the boundary, `SYNTAX.md:232` | `"reference-local-id"` layout, `"reference"` dependency role, and `"structural"` / `"catalog-local-intern-id"` / `"constructor-and-ordered-fields"` identity rows at `v7/emitters/2_interned_storage.dl7:146-160,184-188,206-209`; asserted `v7/test/15_interned_storage.test.pl:216-244,293-320,328-344` | implemented as target-neutral rows; no runtime consumes them | point the SQLite IVM plugin at the six artifacts and assert the DDL it derives, or record the missing consumer explicitly |
| typed field storage | `col_type(Ref, Column, Type)`; struct values live in a storage-plane dictionary keyed on canonical content, `CONSTRUCT-REFERENCE.md` `type_decl/2` | `storage_field_layout` with representation in `reference-local-id` / `dictionary-local-id` / `scalar` / `inline`, `v7/emitters/2_interned_storage.dl7:41-48,146-182` | partial: policy-wide only. The dictionary claims every field of the selected scalar type (`:162-168`); per-field opt-in does not exist. F3 defect open | the `Interned` arc plus the F3 guard, with per-field count assertions |
| projection and destructuring | `decode/2` struct destructure, named args, `{k: v}` json patterns, `CONSTRUCT-REFERENCE.md` `decode/2` and `{}/1` | type level: `Pick` / `Exclude` at `v7/prelude/1_declarations.dl7:19,48` with rules in `3_derived_rules.dl7`, fixture `v7/test/fixtures/2_partial.dl7:32-52`. Boundary metadata: `storage_projection` at `v7/emitters/2_interned_storage.dl7:62-69` | type-level implemented; **value-level destructuring missing** — no `decode/2` analogue, no json plane | record as missing; do not design |
| generics and type queries | `rel_template/3` generics and generic interface bounds, `v6/prolog/compile/SYNTAX.md:7`, audit index reuse class "adapt" | userland constructors over `intern`: `Partial`, `Option`, `Pick`, `Exclude`, `Key`, `Curry`, `HistoryV1` (`v7/prelude/1_declarations.dl7`, `2_constructor_rules.dl7`); type algebra `Conforms`, `ConformsAll`, `Intersect` (`v7/prelude/4_type_algebra.dl7:149-270`); fixtures `2_partial.dl7`, `3_type_algebra.dl7`, `5_curry.dl7` | implemented for authoring and comptime reading, with the F4 hole | fix or record F4: an expression-alias is declarable and unusable as a field target |
| ordinary rules | `Head <- Body.` level rule, `SYNTAX.md:237` | `(<- (Head ..) (Body ..))`, used throughout `v7/prelude/3_derived_rules.dl7` and every emitter | implemented | none |
| negation | negative goals with safety and stratification | checked at `v7/src/2_comptime/1_checker.pl:36,157-196,494,545-556`; used by `v7/emitters/2_interned_storage.dl7:135,141,152,160,168,176` | implemented | none |
| aggregates | `group_concat/1` live, `json_array/1` refused, decomposable aggregate heads lifted into SQL, `CONSTRUCT-REFERENCE.md` `:=/2` | **`count` only**, hardcoded at `v7/src/2_comptime/1_checker.pl:193,656`, `v7/src/2_comptime/0_lowerer.pl:869,1576`, `v7/src/1_libtime/0_evaluator.pl:102,185,283,541`; used by `v7/prelude/3_derived_rules.dl7:66` | partial, one of DL6's set | author a rule using `(sum ?X)` and capture the exact diagnostic, so the gap carries a citation instead of an inference |
| clocks and change operators | `latest/1`, `finalize/1`, `now/1` contextual gates; `<+` edge rules (`SYNTAX.md:238`); `keep(all|count(N))` retention (`SYNTAX.md:236`); `v6/prolog/compile/TICK-MODEL.md` | `clock_dependency/8` derived by `v7/emitters/1_clock.dl7:27-34`, matching `program_rule_kind ?Rule "level"` only. `program_rule_kind(RuleId, level)` is the single kind ever reified, hardcoded at `v7/src/3_emit/0_logical_program_reifier.pl:228`. The emitter's own header states it: "both planes are the relation plane and every dependency has grade zero" (`v7/emitters/1_clock.dl7:1-4`) | **metadata-only, and the operators are missing** — one rule plane, no edge rule, no latest/finalize/now, no retention, no tick | none in scope; this is a kernel and phase question for Chris |
| emitter targets | TS+SQLite door `emit_ts.pl` (paused by user decision 2026-08-21), Rust door, jsonschema/openapi emitters under `v6/prolog/compile/` | `(emits E "name" rel)` userland declaration plus `emit_compiled/4`; backends `v7/src/3_emit/1a_dbsp_plan_emitter.pl`, `1b_dbsp_rust_emitter.pl`, `1c_sqlite_query_emitter.pl`, `2_rust_type_emitter.pl`, `2a_dl7_rust_emitter.pl`; userland emitters `v7/emitters/0_dbsp.dl7`, `1_clock.dl7`, `2_interned_storage.dl7`; tests 9, 10, 11, 13, 14, 15 | implemented, the strongest parity row | none |

### Kernel primitive inventory the map rests on

Seventeen kernel relations, at `v7/src/2_comptime/1_checker.pl:310-324` and
`v7/src/2_comptime/0_lowerer.pl:1601-1615`: `node`, `module`, `product`, `sum`,
`:`, `edge_snapshot`, `nil`, `cons`, `edge_ref`, `intern`, `intern_snapshot`,
`predecessor`, `def`, `head`, `body`. Everything in the map above that is marked
implemented composes from these.

## Decisions this map surfaces

Stated as questions with citations. No semantics proposed.

| # | owner | question | citation |
|---|---|---|---|
| 1 | kernel, needs Chris | should a `Key` label feed a relation's KeySets? Today only a field named `return` mints one | `v7/src/2_comptime/0_lowerer.pl:605-613` |
| 2 | kernel, needs Chris | should a compound label accept an expression target? | `v7/src/2_comptime/0_lowerer.pl:650-665`, `:492` |
| 3 | kernel, needs Chris | should an expression-alias name be referenceable as a field target? | `v7/src/2_comptime/1_checker.pl:267` |
| 4 | kernel, needs Chris | which aggregates beyond `count` belong in DL7? | `v7/src/2_comptime/1_checker.pl:656` |
| 5 | kernel and phase, needs Chris | the whole clock plane: edge rules, tick, retention, sampling gates | `v7/src/3_emit/0_logical_program_reifier.pl:228`, `v7/emitters/1_clock.dl7:1-4` |
| 6 | user scope call | is DL6 value-level destructuring, the `decode/2` and json plane, part of the DL6-on-DL7 target at all? | `v6/prolog/compile/CONSTRUCT-REFERENCE.md` `decode/2`, `{}/1` |
| 7 | userland, dispatchable now | the F3 scalar-fallback guard | `v7/emitters/2_interned_storage.dl7:170-176` |

## What this map does not say

Full DL6 parity is not reached and this document does not claim it. Four of the
eleven rows are metadata-only, partial or missing, and the clock plane is the
largest of them. Every "implemented" row above is backed by a test that runs;
no row is marked implemented from reading a comment or a header.
