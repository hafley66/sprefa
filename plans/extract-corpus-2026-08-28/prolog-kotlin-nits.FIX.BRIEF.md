# Brief: prolog meta spec table defects + kotlin `!in` (lane `fix-extract-prolog-specs`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws).

## First action
```
git merge --ff-only cec3d5c1d
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-prolog-specs sprefa-coordinator "<one line>"`.

## Defects (verified by the coordinator with a probe)
File `v6/sprefa-extract/src/lang/prolog/_0_source.rs`, fn `builtin_meta`:
1. `setof/3`, `bagof/3` rows are `[Caret, Goal]`. SWI: `setof(?Template, ^Goal, -Set)`,
   so index 0 is Data (template) and index 1 is Caret (goal under `^`).
   Probe `t(L) :- setof(p(X), q(X), L).` today: `q/1 term_arg`, no goal site.
   Expected: site `q/1`, reference `q/1 goal`; `p/1 term_arg`.
2. `aggregate_all/4` is `(+Spec, ?Discriminator, :Goal, -Result)`: goal at
   index 2; the row has it at index 1. Same probe shape with
   `aggregate_all(count, X, q(X), _)`.
3. `catch_with_backtrace` is arity 3 `(Goal, Catcher, Recovery)` =
   `[Goal, Data, Goal]`; the row registers arity 2.
4. `setup_call_cleanup/4` does not exist; `setup_call_catcher_cleanup/4` is
   `(Setup, Goal, Catcher, Cleanup)` = `[Goal, Goal, Data, Goal]`.
5. Missing: `partition/6` `(Pred, List, Less, Equal, Greater)` = `[Closure(2)]`,
   `foldl/4..7` fine, add `include/exclude` only at arity 3 (arity 2 rows are
   for predicates that do not exist; drop them).
6. `MetaTable::get` allocates `name.to_string()` per goal lookup. Key the
   maps on `(Box<str>|String, usize)` but look up without allocating
   (`HashMap<(String,usize),_>` cannot; use a two-level
   `HashMap<usize, HashMap<String, Vec<MetaSpec>>>` keyed by arity then
   `get(name)` borrows `&str`). Receipt: `extract --family call` wall on
   `/opt/homebrew/lib/swipl/library/ext/chr/chr/chr_translate.pl` before/after.
Verify every spec row against `swipl -g "forall(member(P,[setof/3,bagof/3,aggregate_all/4,catch_with_backtrace/3,setup_call_catcher_cleanup/4,partition/6,include/3,foldl/4]), (predicate_property(H, meta_predicate(M)), H=..[_|_], functor(H,N,A), P=N/A -> print(M), nl ; true))" -t halt`
(adjust the goal to print each `meta_predicate` spec; paste the output in
the commit body as the oracle for the table).

File `v6/sprefa-extract/src/lang/kotlin.rs`, `check_expression` arm:
7. `3 !in s` mints no `contains` site: the anon token is `!in`, the match
   wants `in`. Accept both `in` and `!in`.

## Fail-first
Extend `tests/1b_prolog_metacall.rs` with a fixture
`tests/fixtures/prolog/corpus_4_meta_specs.pl` covering rows 1-5 (sites and
reference positions in clause order), and `tests/48_kotlin_operator_calls.rs`
with a `!in` case. Red output in the commit bodies. Then
`cargo test --features cli --no-fail-fast`, full passed/failed. The kind_vocab
wire golden (`tests/6_kind_vocab.rs`) may drift if a corpus fixture gains
sites; regenerate by the test's documented procedure and state the hunk count.

## Files you own
`src/lang/prolog/**`, `src/lang/kotlin.rs`, the two test files above, the
new fixture, `tests/fixtures/kind_vocab/wire_golden.jsonl` (regen only).
Forbidden: everything else, including `src/lang/rust.rs` and `src/project.rs`
(another lane owns them now). No whole-crate `cargo fmt`; fmt only the files
you own. No subagents. Run the gate with `--no-fail-fast` and report the
sum over all binaries, never a partial count.
Then: push, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-prolog-specs sprefa-coordinator "PR #N, gate <p>/<f>"`.
