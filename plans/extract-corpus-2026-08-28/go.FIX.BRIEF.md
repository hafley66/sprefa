# Brief: fix the go corpus findings (lane `fix-extract-go-corpus`)

Read `/Users/chrishafley/projects/sprefa/plans/extract-corpus-2026-08-28/COMMON.md`
(style laws, 10-second law, forbidden list) and the findings you are fixing:
`plans/extract-corpus-2026-08-28/go.REPORT.md` section "Findings" (in your
worktree; base sha carries it).

## First action
```
git merge --ff-only 5a13c36bb
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-go-corpus sprefa-coordinator "<one line>"`.

## Three fixes, each its own commit, each fail-first
Order: F1, F2, F3. Every fix: write the failing test FIRST, run it red
(`cargo test --features cli <name>`), paste the red output into the commit
body, then fix, then green.

### F1 receiver type params in sigs (go arm, `src/lang/go.rs`)
Fixture `tests/fixtures/go/corpus_1.go` already exists and states the
expected fact in its header. Today `extract --family type` on it emits
`sig{owner=Get,slot=ret,ty="T"}`. Expected: no sig row whose `ty` is a type
parameter declared in the receiver's `type_arguments` (`func (g Gen[T]) Get() T`).
Extend the exclusion set that today reads only the method's own
`type_parameters`. Test file: `tests/25_go_specifiers.rs` or a new
`tests/44_go_receiver_type_params.rs`. The v5 parity oracle at
`tests/fixtures/go/*.v5.jsonl` may need a regenerated row; if the parity
matrix (`tests/33_v5_parity_matrix.rs`) refuses, record WHY in the commit
body and waive by the mechanism that file documents, never by deleting a row.

### F2 JSONL emission throughput (`src/wire.rs`, `src/bin/extract.rs`)
Repro: `timeout 10 extract ~/go/pkg/mod/golang.org/x/text@v0.22.0/collate/tables.go`
rc=124 at ~10.3s while `extract --bench <same>` finishes in 1.9s with
2,255,221 facts. The 8s gap is formatting + writing rows. Target: the default
run on that file under 4s (`/usr/bin/time` wall, stdout to `/dev/null` AND to
a pipe `| wc -l`, both under 4s).
Steps, in order, measure after each, keep only what moves the number:
1. Confirm stdout is wrapped in ONE `BufWriter` (>= 256 KiB) held for the
   whole run, one `lock()`; no per-row `println!`/`flush`.
2. Serialize with `serde_json::to_writer` into that writer, never
   `to_string` + `writeln!`.
3. Profile before guessing further: `cargo flamegraph` or `samply` on the
   repro; paste the top 5 frames into the report.
Test: `tests/45_emit_throughput.rs` runs the binary over a synthetic
200k-row input (generate a `.go` file with many literals into a tempdir),
asserts wall under a fixed budget, and pins the row count unchanged
(`--bench` count == piped line count).
Output bytes must stay byte-identical: `diff <(old extract f) <(new extract f)`
on 20 corpus files, paste the empty diff receipt.

### F3 `--resolve` timeout on `x/text` module dirs (`src/project.rs` or `src/lang/go.rs` only)
Repro: `timeout 10 extract --resolve $(find ~/go/pkg/mod/golang.org/x/text@v0.22.0 -name '*.go')`
rc=124. First measure whether F2 already fixes it (the same rows stream). If
still over 10s, profile, find the superlinear step, fix. COUNT test: rows
emitted and wall time for n=100,200,400 files must grow linearly (ratio of
wall at 400 to wall at 200 under 2.5).

## Out of scope (record, do not touch)
scip_skip on read-only module cache; `data.go` 346MB RSS (literal rows are a
caller filter decision).

## Deliverables
- 3 commits as above; `cargo test --features cli` whole-crate count in the last commit body.
- Append a "Fixes" section to `plans/extract-corpus-2026-08-28/go.REPORT.md`:
  table finding / before / after / test name.
- `gh pr create --base main` from your branch.
- `boop beep --no-wait --as fix-extract-go-corpus sprefa-coordinator "go fix: PR #N, F1 <status> F2 <before>-><after>ms F3 <before>-><after>ms, gate <passed>/<failed>"`.

## Forbidden
Any file outside `v6/sprefa-extract/src/{lang/go.rs,wire.rs,project.rs,bin/extract.rs}`,
`v6/sprefa-extract/tests/**`, and `plans/extract-corpus-2026-08-28/go.REPORT.md`.
Other language arms, `v6/prolog/**`, `v6/sprefa-engine-rs/**`, `CLAUDE.md`.
No subagents. No `--no-verify`. No push to main.
