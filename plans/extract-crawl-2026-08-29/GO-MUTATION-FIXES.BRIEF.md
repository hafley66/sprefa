# Lane `fix/extract-go-mutation-findings` (glm53): F2 duplicate def, F3 relocation

First action: `git merge --ff-only 2dde9d4bfc418430f4e63388baf3b500c530f938`. Failure = STOP, beep the coordinator.

## Goal, one sentence

Turn mutation-battery findings F2 and F3 green for go: a duplicate def in the
same package must flip the corpus_unique edge to ABSENT (today it keeps it),
and a call edge must survive def relocation (today it breaks) — un-ignore the
two battery tests, fix the go resolver, ratchet go floors must hold.

## The findings (tests/90_mutation_battery.rs, PR #622)

- F2: new file `package docs` declaring a second `func Trim`; the edge
  `docs.go MakeEngine -> docs.go Trim origin=corpus_unique` survives. The
  uniqueness count misses package-mates. Look where go's resolve leg joins
  `DefIndex` / `GoModuleIndex::resolve_type_in_dir` for the call plane.
- F3: relocating a def to another file in the same package breaks the call
  edge (types survive). Diagnose which leg loses it (same-file leg not
  falling through to the package leg after the move?).
- The exact mutations are IN the test file; un-ignore, run, paste the red.

## Method

1. Un-ignore F2/F3 tests, red receipt (paste output).
2. Fix in `v6/sprefa-extract/src/lang/go.rs` (+ `go_modules.rs` if the
   package join lives there; smallest diff).
3. Battery fully green; origin-conservation tests are the bystander guard.
4. Suite `cargo test --features cli,rust-checker,ts-checker` rc=0 direct.
5. Gate: BEEP THE COORDINATOR for the go before running anything —
   `boop beep --no-wait --as fix-extract-go-mutation-findings sprefa-coordinator "ready for ratchet go"` —
   then ONLY the go leg: `cargo test --release --features cli --test
   ratchet_recall -- --exact --ignored --nocapture ratchet_go`. All go rows
   must hold (measure signature: go.call.syntax vs codeql2 on ts-go(5,097f)
   recall floor 98.96% = 48,025/48,529 oracle edge-rows). A dropped guess may
   LOWER ours-count; recall floors must still hold — if a floor breaks, the
   dropped edges were carrying real matches: STOP, report, do not tune.

## Files you own

- `v6/sprefa-extract/src/lang/go.rs`, `go_modules.rs`
- `v6/sprefa-extract/tests/90_mutation_battery.rs` (un-ignore only, never
  weaken an assertion)
- FORBIDDEN: rust/ts/python lang files, tests/bench/**, RATCHET.tsv,
  src/types.rs.

## Receipts (PR body)

Red-then-green battery output; go ratchet leg all-rows-hold paste; suite rc=0
direct; diff-stat = owned files. Every percent with its fraction and units
(COMMON.md measure signature).

## Laws

Banned words prose+identifiers: provenance, substrate, load-bearing, regime,
ground truth (say oracle), honest, signal. Commit per finding; push;
`gh pr create`; beep the coordinator with the PR number.
