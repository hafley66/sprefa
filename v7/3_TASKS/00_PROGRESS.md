# DL7 minimal kernel progress

Updated: 2026-08-28 20:55 EDT

## Basement restart

The next implementation boundary is every layer before comptime fixpoint
goals. Plan: `v7/2_DESIGN/2_BASEMENT_TO_DATALOG.PLAN.md`.

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
- Wrote `v7/2_DESIGN/1_MINIMAL_VERTICAL_SLICE.PLAN.md`.
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
  `v7/3_TASKS/results/0_KERNEL_CONTRACT.md`; `git diff --check` passed.
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
  `v7/3_TASKS/results/1_CONTRACT_CRITIQUE.md`.

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
swipl -q -g "load_files(['v7/0_SWIPL/test/0_reader.test.pl'],[silent(true)]),run_tests,halt"
```

No other suite run.

Milestone 2 (nested root lowering):

- Added `v7/1_DATALOG/0_basement.pl`, exporting `lower_datalog/4`. The next
  static-check milestone extends this same module instead of adding another
  production file.
- The module has 271 nonblank, noncomment lines.
- The immutable unit plus reader node identity mints module, product, and sum
  owners.
- Every nested constructor receives one `scope_parent/2`; every bind retains
  owner, name, pending target, and zero-based ordinal.
- Facts and rules are ground compiler data using pending `name/2`,
  `var/1`, and `const/1` terms.
- The direct receipt over nested products, a sum, one fact, and two recursive
  rules produced:

```text
receipt([0,1,2],counts(6,5,11,4,1,2,26))
```

The fields are top-level bind indices followed by node, parent, edge,
relation, seed, rule, and origin counts. `ground(Program)` succeeded and
`git diff --check` passed. No suite or test file was added.
