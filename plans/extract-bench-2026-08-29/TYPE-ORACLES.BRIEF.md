# Lane `bench-extract-type-oracles` (opus): type-family oracles for go and rust

Read `plans/extract-bench-2026-08-29/COMMON.md` and `plans/extract-bench-2026-08-29/ORACLES.REPORT.md`. Call and module families
have compiler oracles; the `type` family has none, and `go.parse.type.tsv`,
`rust.parse.type.tsv`, `go.dietscip.type.tsv`, `rust.dietscip.type.tsv`
already exist on our side. Build the oracle side.

## First action
```
git merge --ff-only 291a87a1153135250424034d2ff98f4fd0f0e2b9
```

## Normal form for `type`
`src_path src_name dst_path dst_name` where src_name is the type or fn
declaring the reference and dst_name the referenced type, bare names. Three
kinds folded into one file, with a fifth column `kind` in
`{ref,implements,extends}`. Read the first 20 rows of `go.parse.type.tsv`
first and match its convention exactly; if its convention is ambiguous, say
so in the report with 5 example rows and pick the reading that maximises
overlap.

## Task A: go via go/types
Extend `oracle_go/main.go` (it already loads packages with
`golang.org/x/tools/go/packages`): walk `TypesInfo.Uses` for
`*types.TypeName` objects, emit `ref` rows keyed on the enclosing decl;
`implements` via `types.Implements` over every named type x every
interface in the corpus (cap at interfaces with at least one method; report
the pair count); `extends` = embedded struct fields. Output
`go.oracle.type.tsv`. Receipt: `bench.py go.parse.type.tsv go.oracle.type.tsv`
and the same against `go.dietscip.type.tsv`; recall and precision per kind.

## Task B: rust via ra_ap_ide
Extend `ra_ide_probe/main.rs` (it already loads the rust-analyzer workspace):
for every `Name` node that resolves to an ADT or trait via
`Semantics::resolve_path`, emit `ref`; `implements` from every
`impl Trait for Type` block (`hir::Impl::trait_`, `self_ty`); no
`extends` for rust (leave the kind unused). Output `rust.oracle.type.tsv`.
Same two `bench.py` receipts.

## Stall law
Both probes run in background under `timeout 900` with a log; you poll
every 30 s. Over 900 s: kill, put the last 20 log lines in the report, and
hail the coordinator.

## Ownership
`oracle_go/`, `ra_ide_probe/`, `go.oracle.type.tsv`, `rust.oracle.type.tsv`,
and a new section "13. type oracles" appended to `ORACLES.REPORT.md`.
Nothing else, no `src/`. The lane `bench-extract-tools-2` owns
`TOOLS.REPORT.md` and `tools/`; never touch them.

## Report shape (tables only)
| lang | kind | oracle rows | ours rows | overlap | recall | precision | diet scip recall | wall |
Plus 10 sample rows per difference set with a one-word class each.

## Receipt
Push `bench/extract-type-oracles`, `gh pr create --base main`, hail
`boop beep --no-wait --as bench-extract-type-oracles sprefa-coordinator "type oracles: PR #N, go type recall x%, rust type recall x%"`.
Laws: no em dashes, no words provenance/substrate/load-bearing/regime, never
"ground truth" (say oracle), commit the tsvs, no `--no-verify`.
