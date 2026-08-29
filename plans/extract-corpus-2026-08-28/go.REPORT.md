# go arm: extract corpus battery report (2026-08-28)

TOC
- [Setup](#setup)
- [Step 1: per-file default](#step-1-per-file-default)
- [Step 2: per-file by family](#step-2-per-file-by-family)
- [Step 3: --resolve per module](#step-3---resolve-per-module)
- [Step 4: diet_scip](#step-4-diet_scip)
- [Step 5: scip](#step-5-scip)
- [Perf](#perf)
- [Extra go checks](#extra-go-checks)
^- [Findings](#findings)
- [Fixes](#fixes)
- [What stays untested and why](#what-stays-untested-and-why)

## Setup

- Corpus: `~/go/pkg/mod/` (331M), 7831 `.go` files incl. `_test.go`, 60 module dirs with 2+ files.
- Binary: `v6/sprefa-extract/target/release/extract`, built at merge 8e946ada9.
- Raw tables committed next to this report: `go.runs.tsv` (step 1), `go.resolve.tsv` (step 3), `go.diet.tsv` (step 4), `go.rss.txt` (RSS).

## Step 1: per-file default

| metric | value |
|---|---|
| files run | 7831 |
| rc=0 | 7827 |
| rc=124 (timeout 10s) | 4 |
| rc other | 0 |
| wall total | 8 parallel loops, ~13 min wall |

All 4 timeouts are the two generated Unicode-table files of `golang.org/x/text`, both versions:

| path | bytes | ms | stdout lines at kill |
|---|---|---|---|
| x/text@v0.22.0/collate/tables.go | 2,255,220 | 10324 | 1,192,449 |
| x/text@v0.22.0/date/tables.go | 1,937,268 | 10502 | 1,167,592 |
| x/text@v0.3.4/collate/tables.go | 2,255,220 | 10724 | 1,956,148 |
| x/text@v0.3.4/date/tables.go | 1,937,268 | 10640 | 1,852,149 |

`--bench` on the same files finishes in 1.9-2.0s with facts=2,255,221 / 1,937,286, so the parse + projection fit the budget; the 10s is spent formatting and streaming ~2M JSONL rows to stdout.

## Step 2: per-file by family

200-file sample (largest 100 + random 100).

| family | sum of lines | files where fam > default |
|---|---|---|
| default (all kinds) | 37,006,410 | - |
| cst | 38,746,908 | 4 |
| type | 8,745 | 0 |
| call | 107,398 | 0 |
| df | 359,581 | 0 |

The 4 cst-exceeds-default files are the step-1 timeout files: the default run was truncated at 10s while the cst-only run completed. Artifact of the timeouts, not a family defect.

## Step 3: --resolve per module

60 module dirs, all `.go` files each, `timeout 10`:

| metric | value |
|---|---|
| modules rc=0 | 58 |
| modules rc=124 | 2 (x/text@v0.22.0, x/text@v0.3.4) |
| resolved_edge total | 162,419 |
| unresolved total | 0 |

Zero `unresolved` rows in every module. The schema's unresolved reasons (dynamic-import, computed-member-call, spread-call-args) are TS-shaped; the go arm emits no unresolved records. Package-qualified callees (`yaml.Marshal`) resolve by trailing-name match, so imports are not needed for a hit.

## Step 4: diet_scip

Same 60 module dirs. `resolved_edge` under diet_scip equals the step-3 count on all 58 comparable modules (they share the name-matching engine), plus `resolved_type_edge` (e.g. x/tools 711, protobuf 1925). x/text@v0.3.4 rc=124 (10,082ms); x/text@v0.22.0 completed in 7,777ms, near the deadline.

## Step 5: scip

`scip-go` installed into scratch GOBIN (`go install github.com/scip-code/scip-go/cmd/scip-go@v0.2.7`; `sourcegraph/scip-go@latest` fails: module renamed to `scip-code/scip-go`).

scip_skip row recorded verbatim on all 3 roots in the mod cache:

```json
{"record":"scip_skip","lang":"go","bin":"scip-go","reason":"failed","detail":"scip indexer failed: go mod download gopkg.in/check.v1"}
```

Root cause: module-cache roots are read-only and their `go.sum` lacks test-dependency entries; scip-go runs `go mod download`, which needs a writable root. After copying the root to writable scratch and running `go mod download gopkg.in/check.v1` (GOFLAGS=-mod=mod), scip-go succeeds and `--family scip` consumes the cached index:

| root | scip_def | scip_name | scip_ref | scip_fn_edge | resolved_edge (step 3) |
|---|---|---|---|---|---|
| yaml.v3@v3.0.1 | 1009 | 991 | 1270 | 3303 | 1301 |
| repr@v0.4.0 | 100 | 99 | 86 | 176 | 118 |
| uuid@v1.6.0 | 227 | 208 | 274 | 414 | - |

20 scip_fn_edge rows with no matching resolved_edge (yaml.v3), classified:

| class | count of 20 | example | why |
|---|---|---|---|
| go test synthesized main | 4 | main -> benchmarks. | scip synthesizes the generated `_testmain.go` main; no source site exists |
| closure attribution | 4 | TestMarshalTypeCache -> marshalerType | site sits inside a `func(){}` literal; resolve names the caller `closure@<byte>`, scip keeps the enclosing method name |
| member-access edge | 12 | dropNode -> Node#Kind., Value., Tag. | scip emits field/type-ref edges; the resolve `call` arm emits call edges only |

## Perf

- bytes/ms computed per file from step 1 (`go.runs.tsv` joined with byte sizes).
- 5th percentile floor is ~2 bytes/ms across hundreds of files; every sampled bottom file is a tiny testdata file (e.g. `x/tools/.../testdata/src/time/time.go`, 56 bytes, 28ms). The floor is process startup (~25ms), no slow construct.
- 20 largest files under `/usr/bin/time -l` (`go.rss.txt`): max RSS 939,671,552 bytes (x/text@v0.3.4/collate/tables.go, 2,255,221 facts). Second cluster: the 4 gofont `data.go` files (1.1MB byte-array literals) at ~340-360MB RSS, 366k cst nodes each, driven by 173,248 `int_literal` rows per array.

## Extra go checks

| construct | probe result |
|---|---|
| method receivers as definitions | `func (b Base) Hello()` emits type node kind=method and call node kind=method (go.rs:151-167) |
| interface method calls | `i.Hello()` emits a call site (callee "Hello") |
| embedded structs | `type Deriv struct{ Base }` emits resolved_type_edge Deriv -> Base kind=impl (go.rs:349-357) |
| generics | `type Gen[T any]` emits struct entity; `Gen[int]` call site and sigs emitted; see finding G1 |
| init() | emits type node kind=function and call node kind=function, name "init" |
| cgo / build-tag files | `//go:build linux` file parses clean, rc=0, facts emitted |
| dot imports | `. "strings"` -> specifier kind=namespace (go.rs:676) |
| blank imports | `_ "os/exec"` -> specifier kind=side_effect (go.rs:673) |
| aliased imports | `fmtpkg "fmt"` -> kind=named, module=fmt |

## Findings

| lang | class | path:line | repro | observed | expected |
|---|---|---|---|---|---|
| go | timeout | x/text@v0.22.0/collate/tables.go:1 | `timeout 10 extract <path>` (go.runs.tsv) | rc=124 at 10.3s; --bench 1.9s / 2,255,221 facts | stream 2.2M JSONL rows under 10s, or emit less |
| go | timeout | x/text@v0.22.0/date/tables.go:1 | same | rc=124 at 10.5s | same |
| go | timeout | x/text@v0.3.4/collate/tables.go:1 | same | rc=124 at 10.7s | same |
| go | timeout | x/text@v0.3.4/date/tables.go:1 | same | rc=124 at 10.6s | same |
| go | timeout | module dir | step 3/4 loop over x/text dirs | `--resolve` rc=124 both versions; diet_scip rc=124 on v0.3.4 | module-size resolve under 10s |
| go | unresolved | module-cache roots | `extract --family scip ~/go/pkg/mod/gopkg.in/yaml.v3@v3.0.1` | scip_skip failed: `go mod download gopkg.in/check.v1` | scip index built (works after writable-root copy + `go mod download`) |
| go | wrong_fact | tests/fixtures/go/corpus_1.go:10 | `extract v6/sprefa-extract/tests/fixtures/go/corpus_1.go` | sig{owner=Get,slot=ret,ty="T"} | receiver-declared type params excluded from sigs; parity-pinned with v5 `go_fn_type` (src/graph/typegraph/go.rs:317), so no v6-only fix |
| go | perf | gofont/gomono/data.go:1 | `extract --bench <data.go>` | 366k cst rows, 346MB RSS for a 1.05MB file (173k int_literal rows) | literal-element rows dominate; caller decides whether to filter |

No crash, no parse_error, no missing_fact, no rc!=0 besides the timeouts; 0 of 7827 non-timeout files failed.

## What stays untested and why

- `--scip-build` end-to-end inside extract: every mod-cache root is read-only with incomplete go.sum, so the spawned scip-go always fails there (finding above). The 3 indexed roots were pre-built by scip-go manually in writable scratch; extract's own `--scip-build` spawn path from a writable root is untested here.
- Cross-module resolution: each `<mod>@<ver>` dir was resolved in isolation; edges into other modules in the cache (e.g. x/tools -> x/mod) are out of scope for parse-based resolve.
- cgo files: the corpus holds none beyond generated stubs; `.go` files with `import "C"` were not separately enumerated.
- Windows/plan9 build-tag content: only the `//go:build` line's parse was checked, not per-tag evaluation (out of scope; extract is tag-blind by design).
- scip_fn_edge vs resolved_edge precision beyond the 20-row sample on yaml.v3.
## Fixes

Lane `fix-extract-go-corpus`, base 5a13c36bb. Red-first per fix; whole-crate gate `cargo test --features cli` 356 passed / 0 failed.

| finding | before | after | test |
|---|---|---|---|
| wrong_fact: receiver type params in sigs (corpus_1.go) | sig{owner=Get,slot=ret,pos=0,ty="T"} | excluded; receiver `type_arguments` identifiers joined into the exclusion set (src/lang/go.rs `go_type_param_names`) | tests/44_go_receiver_type_params.rs |
| timeout: JSONL emission (x/text collate/tables.go, 2,255,221 rows) | /dev/null 3.67s, piped 3.44s | /dev/null 2.73s, piped 2.22s; one 256 KiB BufWriter + `serde_json::to_writer`, no per-row println/to_string | tests/45_emit_throughput.rs |
| timeout: --resolve on x/text module dirs (486 files) | rc=124 (report) | rc=0, 3.34s wall, 7,707 rows; fixed by the emission change, no new src change; growth pin wall(400)/wall(200)=1.58 (765/1961/3093ms at n=100/200/400) | tests/46_resolve_scaling.rs |

Commit receipts: 43bae3a35 (F1), 32a439f04 (F2, includes pre-fix profile top frames and the 20-file byte-identical diff receipt: files=20 diffs=0), F3 commit (count test, no src change).

Out of scope, unchanged: scip_skip on read-only module cache; data.go 346MB RSS (literal rows are a caller filter decision).
