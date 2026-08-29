# DL7 minimal kernel progress

Updated: 2026-08-29 10:59 EDT

## Tree-sitter parser replacement

- Added `v7/justfile`; `just build` is the first V7 build gate.
- Added `v7/tree-sitter-dl7/grammar.js` and generated C parser metadata.
- The grammar owns parenthesized nesting, strings, comments, token boundaries,
  source coordinates, and recoverable syntax errors.
- A maximal `bare_token` preserves the existing delimiter law. Classification
  into names, integers, symbols, variables, or diagnostics remains an adapter
  responsibility.
- Added one corpus case covering nested prefix syntax, strings, symbols,
  variables, integers, empty expressions, and malformed whole-token examples.
- `just build`: 1 parse, 1 successful, 0 failed.
- Existing fixtures `0_minimal.dl7`, `2_partial.dl7`, and prelude
  `0_types.dl7` parse without `ERROR` or `MISSING` nodes.
- Added the C ABI declaration `tree_sitter_dl7()` and verified the header with
  C, C++, and Zig frontends.
- The current SWI parser remains connected until the canonical syntax adapter
  reproduces `read_dl7/5` output.

## Basement restart

The next implementation boundary is every layer before comptime fixpoint
goals. Plan: `v7/design/2_BASEMENT_TO_DATALOG.PLAN.md`.

```text
root datums
    -> nested bind/product/sum lowering
        -> reference resolution + Datalog checks + dependency graph
            -> later comptime fixpoint evaluator
```

Current datum law:

```text
atom(Name)       unresolved reader spelling
ref(Target)      resolved semantic name
var(Identity)    logical variable
const(Value)     literal data
```

Nested `:` is an owner edge and namespace operation. Nested `*` and `+` forms
create owners with parent-scope edges. Future dot resolution traverses the
canonical colon edges. Future punning expands into the same explicit edge
shape and therefore receives no second representation.

Issue DAG:

- `@dl7-root-datums`, GLM53F, S, head of line.
- `@dl7-datalog-lower`, GLM53F, M, blocked by root datums.
- `@dl7-datalog-checks`, GLM53F, M, blocked by lowering.

No evaluator, type fixpoint, interning, engine, Rust, or TypeScript work is in
these three cards.

## Current state

- Plan committed: `52c6d203f`
- Issue DAG committed: `6b82a9d83`
- Active epic: `@dl7-minimal-kernel`
- Spawnable head: `@dl7-kernel-contract`
- Production code added: 0 files
- Tests added: 0
- V6 engine files changed: 0

## Completed

- Read Boop favorites 26 through 37 covering binding, prefix syntax,
  application, interning, compiler phasing, and shared fixpoint semantics.
- Wrote `v7/design/1_MINIMAL_VERTICAL_SLICE.PLAN.md`.
- Capped the first slice at four production modules and one exact test.
- Made `Partial`, `Pick`, and `Exclude` dependent userland proof goals.
- Created the issuectl epic and eleven task cards with model, size, lane,
  collision, and blocker metadata.
- Verified the DL7 scheduling DAG has one head of line.

## Active

- Sol documentation landed on main as `7e3303be5`.
- `@dl7-kernel-contract` is `needs-info` and blocked by
  `@dl7-contract-critique`.
- Opus report landed on main as `4018330a1` and
  `@dl7-contract-critique` is `done`.
- `@dl7-kernel-contract` remains `needs-info`.
- No reader, evaluator, kernel, prelude, fixture, test, or engine lane has been
  spawned.
- Reader and evaluator cards now explicitly depend on the unresolved kernel
  contract, so issuectl exposes no DL7 implementation head of line.

## Hitches

- Initial `git push` failed because sandbox DNS could not resolve GitHub.
- Escalated push was rejected by the approval reviewer because the remote was
  treated as unverified external egress. Agent worktrees will use explicit
  local base `652f3fde1`; no push workaround will be attempted.
- Repository-wide `issuectl doctor` reports three pre-existing findings outside
  the DL7 epic. No DL7 issue was reported.
- Sol lane diagnostics at 00:24 EDT:
  - supervisor opened the Codex ACP session and loaded the 1,508-byte brief;
  - `boop beep ps` reports PID `0` while tmux still contains the supervisor;
  - worktree is clean at `a8bcda72c`;
  - `boop debug` reports no assistant or tool turn;
  - a resume hail was claimed for the next turn boundary;
  - a 30-second result wait returned no result.
- No second worker has been started against the same card.
- The stalled first turn recovered at 00:26 EDT and began editing the plan.
- At 00:30 EDT Sol asked for a semantic identity ruling:
  - A: `named(ModuleHash, Kind, Name)`, preserving the DL6 identity shape and
    requiring a pinned module-hash input;
  - B: `named(module(ModulePath), Kind, Name)`, using the V7 file owner and
    requiring a portable, collision-free definition of `ModulePath`.
- Selecting either form changes semantic TypeIds. No selection was made.
- A direct attempt to send a selection was rejected by the approval reviewer
  because the user's stop rule requires this choice to return to the user.
- Sol committed `297d90b9a`; it was reviewed and cherry-picked as `7e3303be5`.
  The diff changes only the plan and
  `v7/tasks/results/0_KERNEL_CONTRACT.md`; `git diff --check` passed.
- Coordinator review found three claims requiring Opus receipts before a user
  ruling:
  - phase ownership is inferred from a `primitive(type)` return;
  - lowering inserts `intern/3` only for type-returning callables;
  - normalized `Name/Arity` relation references are module-unqualified.
- The critique card was moved ahead of the blocked contract and expanded to
  compare both identity forms without selecting one.
- The first Opus exit was `rc=4` because the report existed but had not been
  committed. The lane was revived only to commit the reviewed report as
  `0ee0597c1`; it was cherry-picked as `4018330a1`.
- Opus found five contract blockers before code:
  1. `Partial` cannot request `Option(MemberType)` after `MemberType` becomes
     ground during evaluation, so its edge rule derives zero rows.
  2. Automatic `intern/3` insertion runs before its arguments are ground and
     gives type-returning callables a special lowering path.
  3. Compiler ownership inferred from the return column misses relations with
     a type input and scalar output.
  4. Bare `Name/Arity` relation references collide between the source module
     and prelude.
  5. `evaluate/4` emits compiler construction requests, giving the supposedly
     shared evaluator a compile-time transport behavior.
- Opus also listed nine deletions for the first proof, including inserted
  interning, duplicate request rows, stored specialization arity, strata and
  negation, unlowered sum syntax, and unused graph rows.
- Full receipts and the identity comparison are in
  `v7/tasks/results/1_CONTRACT_CRITIQUE.md`.

## Next DAG edges

```text
contract critique [done]
    -> user rulings [STOPPED HERE]
    -> kernel-contract correction [Sol]
    -> prefix-reader [GLM53F] || shared-evaluator [Sol]
    -> symbol-graph [GLM53F]
    -> Partial [GLM53F]
    -> one oracle [Flash 4]
    -> Luna review
    -> Pick/Exclude [GLM53F] || engine seam [Terra]
    -> engine smoke [Flash 4]
```

## Test ledger

Milestone 1 (root datums) reader changes:

- `1_reader.pl` lexes `'Name` as `literal(symbol(Name))` with
  `expected_symbol_name` / `invalid_symbol_name` diagnostics; symbols never
  enter name resolution.
- Fixture `0_minimal.dl7` now pins an empty form, nested product forms, bare
  atoms, symbol data (`'kind`, `'spot`), and the existing variable sharing.
- `0_reader.test.pl` snapshot regenerated for the extended fixture
  (digest `f2ae0a30...`, nodes 0-47, sources 0-47).
- Gate run after the reviewed symbol-diagnostic correction: all four
  `dl7_reader_foundation` tests pass.

```text
swipl -q -g "load_files(['v7/test/0_reader.test.pl'],[silent(true)]),run_tests,halt"
```

No other suite run.

Milestone 2 (nested root lowering):

- Added `v7/src/2_comptime/0_compiler.pl`, exporting `lower_datalog/4`. The next
  static-check milestone extends this same module instead of adding another
  production file.
- The module has 262 nonblank, noncomment lines.
- The immutable unit plus reader node identity mints module, product, and sum
  owners.
- Every bind retains owner, name, pending target, and zero-based ordinal. A
  nested node's containing node is recoverable from the same edge, so lowering
  stores no second scope-parent relation.
- Facts and rules are ground compiler data using pending `name/2`,
  `var/1`, and `const/1` terms.
- The direct receipt over nested products, a sum, one fact, and two recursive
  rules produced:

```text
receipt([0,1,2],counts(6,11,4,1,2,26))
```

The fields are top-level bind indices followed by node, edge, relation, seed,
rule, and origin counts. `ground(Program)` succeeded and
`git diff --check` passed. No suite or test file was added.

Milestone 3 (resolve, check, graph):

- Ruling applied (Boop favorite 26): `node/1` is the identity carrier;
  `module(Id)`, `product(Id)`, and `sum(Id)` are ordinary classifier relation
  rows, and `':'(Owner, Name, Target, Index)` remains the primordial public
  edge. Kind is not encoded inside `node/2` and no `edge(Id, kind, Kind,
  Index)` row exists; classification-as-edge stays open.
- Namespace ruling applied: namespace and scope are neither node kinds nor
  classifier relations; any node with outgoing named edges is
  namespace-capable. Lexical containment resolves transiently only from
  `pending_edge(Parent, Name, target(Child), Index)` (`parent_owner/3`,
  `0_compiler.pl`); resolved `':'/4` edges are never reversed for
  containment. `module/1` identifies the compiler-created file root;
  `product/1` and `sum/1` retain algebraic meaning. No point in the current
  basement requires a `namespace/1` or `scope/1` relation.
- Added `check_datalog/4` to `v7/src/2_comptime/0_compiler.pl`. The module has
  515 nonblank, noncomment lines and still exports only the two production
  entry points.
- Resolution walks local owner edges, then reverse binding edges to containing
  owners, and resolves `int`, `text`, `any`, and `type` at a module owner to
  the pinned `ref(primitive(Name))` targets.
- Successful output emits one `node(Id)` identity row plus applicable
  `module(Id)`, `product(Id)`, and `sum(Id)` classifier rows, canonical
  `':'(Owner, Name, Target, Index)` edges,
  `relation(ref(Target), Arity)` rows, resolved `call(ref(Relation), Args)`
  seeds and rules, distinct `depends(HeadRef, BodyRef, positive)` rows, and
  `stratum(Relation, 0)` per declared relation. Edge, relation, depends, and
  strata rows are sorted by standard term order; seeds and rules keep authored
  order.
- Checks: duplicate bind names, duplicate bind indices, non-dense zero-based
  indices, explicit relation use, call arity, ground seeds, and head vars
  occurring in positive body calls. Diagnostics are
  `diagnostic(check, OriginNode, Reason)`, sorted by standard term order, and
  no `Checked` value survives any diagnostic.
- Direct SWI receipt (nested product and sum edges, parent-edge resolution,
  recursive graph, undeclared use, arity mismatch, unsafe head variable):

```text
result(good,checked_datalog(root_graph([module(module(unit(gate,content(good)))),product(owner(unit(gate,content(good)),3)),product(owner(unit(gate,content(good)),23)),sum(owner(unit(gate,content(good)),43)),product(owner(unit(gate,content(good)),48)),product(owner(unit(gate,content(good)),57)),product(owner(unit(gate,content(good)),63))],[:(module(unit(gate,content(good))),edge,ref(owner(unit(gate,content(good)),3)),0),:(module(unit(gate,content(good))),from_ref,ref(owner(unit(gate,content(good)),3)),3),:(module(unit(gate,content(good))),node,ref(owner(unit(gate,content(good)),23)),1),:(module(unit(gate,content(good))),reachable,ref(owner(unit(gate,content(good)),63)),4),:(module(unit(gate,content(good))),result,ref(owner(unit(gate,content(good)),43)),2),:(owner(unit(gate,content(good)),3),from,ref(primitive(text)),0),:(owner(unit(gate,content(good)),3),to,ref(primitive(text)),1),:(owner(unit(gate,content(good)),23),id,ref(primitive(text)),0),:(owner(unit(gate,content(good)),23),label,ref(primitive(text)),1),:(owner(unit(gate,content(good)),43),err,ref(owner(unit(gate,content(good)),57)),1),:(owner(unit(gate,content(good)),43),ok,ref(owner(unit(gate,content(good)),48)),0),:(owner(unit(gate,content(good)),48),value,ref(primitive(text)),0),:(owner(unit(gate,content(good)),57),message,ref(primitive(text)),0),:(owner(unit(gate,content(good)),63),from,ref(primitive(text)),0),:(owner(unit(gate,content(good)),63),to,ref(primitive(text)),1)]),datalog_program([relation(ref(owner(unit(gate,content(good)),3)),2),relation(ref(owner(unit(gate,content(good)),23)),2),relation(ref(owner(unit(gate,content(good)),48)),1),relation(ref(owner(unit(gate,content(good)),57)),1),relation(ref(owner(unit(gate,content(good)),63)),2)],[call(ref(owner(unit(gate,content(good)),3)),[const("a"),const("b")]),call(ref(owner(unit(gate,content(good)),3)),[const("b"),const("c")])],[rule(call(ref(owner(unit(gate,content(good)),63)),[var(v(84)),var(v(85))]),[call(ref(owner(unit(gate,content(good)),3)),[var(v(84)),var(v(85))])]),rule(call(ref(owner(unit(gate,content(good)),63)),[var(v(94)),var(v(95))]),[call(ref(owner(unit(gate,content(good)),3)),[var(v(94)),var(v(98))]),call(ref(owner(unit(gate,content(good)),63)),[var(v(98)),var(v(95))])])]),[depends(ref(owner(unit(gate,content(good)),63)),ref(owner(unit(gate,content(good)),3)),positive),depends(ref(owner(unit(gate,content(good)),63)),ref(owner(unit(gate,content(good)),63)),positive)],[stratum(ref(owner(unit(gate,content(good)),63)),0),stratum(ref(owner(unit(gate,content(good)),57)),0),stratum(ref(owner(unit(gate,content(good)),48)),0),stratum(ref(owner(unit(gate,content(good)),23)),0),stratum(ref(owner(unit(gate,content(good)),3)),0)]),[])
result(bad,[diagnostic(check,r0,unsafe_head_var(a)),diagnostic(check,r1,arity_mismatch(edge,2,1)),diagnostic(check,s0,unresolved_name(missing))])
```

The undeclared-use diagnostic surfaces as `unresolved_name(missing)` because
no owner edge resolves the spelling; `undeclared_relation/1` covers a name
that resolves to a non-relation target. `git diff --check` passed. No suite or
test file was added.

Review correction at 23:58 EDT:

- Restored the promised `node(Id)` carrier beside each classifier row.
- Added `(Owner, Index)` collision diagnostics and rejected negative indices.
- Duplicate ordinal receipt:
  `result([],[diagnostic(check,b0,duplicate_bind_index(m,0))])`.
- All 6 existing reader and entrypoint tests still pass.

## Shared libtime evaluator

- Added `v7/src/1_libtime/0_evaluator.pl` with one public `evaluate/4`.
- The evaluator accepts the checked `rule/2` and `call/2` data used by both
  compiler and runtime callers. It has no phase option or phase branch.
- SWI tabling closes positive recursion. A per-call evaluation identity keys
  installed facts, rules, and tabled subgoals.
- Reified `var(Identity)` terms become fresh SWI variables per rule proof;
  repeated identities inside that proof share one variable.
- `setup_call_cleanup/3` abolishes the evaluation table and erases temporary
  clauses after closure is copied and sorted.
- Direct receipt:

```text
receipt([call(ref(edge),[const(a),const(b)]),call(ref(edge),[const(b),const(c)]),call(ref(pair),[const(a),const(a)]),call(ref(pair),[const(a),const(b)]),call(ref(reach),[const(a),const(b)]),call(ref(reach),[const(a),const(c)]),call(ref(reach),[const(b),const(c)]),call(ref(same),[const(a)])],[],[call(ref(ping),[const(ok)])],[],temporary(0,0))
```

The receipt proves duplicate seed collapse, two-hop recursive closure,
same-variable matching, an isolated second invocation, and zero remaining
temporary rule or seed clauses. No test file was added.

## Type graph and userland Partial

- Added phase-independent kernel relations `cons/3` and `intern/3` to libtime.
  `cons/3` builds a closed ordered argument list; `intern/3` returns the
  structural `application(Constructor, Arguments)` identity.
- The checked compiler now exposes `node/1`, `module/1`, `product/1`, `sum/1`,
  `:/4`, `cons/3`, and `intern/3` as relation-shaped kernel nodes with ordinary
  product classifiers and colon edges.
- Added `v7/src/2_comptime/1_type_compiler.pl`. Root graph rows become ordinary
  evaluator seeds, authored rules close through libtime, and the artifact is
  `compiled_unit(TypeGraphFacts, RuntimeProgram, CompilerFacts)`.
- Added `v7/prelude/0_types.dl7`. Partial and Option construction, node
  classification, and edge mapping are authored entirely as DL7 rules.
- `v7/test/fixtures/2_partial.dl7` declares `User(id: int, name: text)` and one
  ground Partial request. No test module was added.
- Normalized direct receipt:

```text
receipt([],[],59,partial(user,[mapped(id,option(int)),mapped(name,option(text))]),repeat(true,true))
```

The receipt checks both generated edges, canonical application identities,
ordinary node and product facts, and byte-equal terms from two compilations in
one SWI process.

## Consolidated vertical oracle

- Extended `v7/test/1_entrypoints.test.pl` with one Partial vertical test.
- Its one expected term covers 59 compiler rows, both mapped edges, node and
  product classification, runtime counts `28/25/11/1/5/10/11`, normalized
  checked call shapes, zero remaining evaluator clauses, and identical repeated
  compilation.
- Focused V7 command: 7 passed, 0 failed, 0 choicepoint warnings.

## Boundary review

- Review: `v7/tasks/results/7_LUNA_REVIEW.md`.
- Production inventory: 8 modules, 1,575 physical lines, 1,357 nonblank and
  noncomment lines.
- Evaluator inventory: 1 export, 1 comptime call site, 0 phase arguments, and
  0 compiler/runtime branches.
- Kernel inventory: 7 relation-shaped nodes and 2 constructive libtime
  clauses (`cons/3`, `intern/3`).
- Partial implementation occurrences in reader, compiler, and evaluator: 0.
- Runtime checked data is retained; runtime runner modules in this slice: 0.
