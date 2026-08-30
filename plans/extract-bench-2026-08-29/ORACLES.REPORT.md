# Compiler oracles and scip reach

Binary `v6/sprefa-extract/target/release/extract` at `9e2b73ef0` (sprefa), built
from `bench/extract-oracles`. Corpora read-only: TypeScript-5.9 `7e133bea1`,
typescript-go `89d5d5b2`, rust-analyzer `af4111f`. Raw tables sit beside this
file (`plans/extract-bench-2026-08-29/*.tsv`).

## Contents

1. [What was measured](#1-what-was-measured)
2. [Part A: the compiler-native oracles](#2-part-a-the-compiler-native-oracles)
3. [Part B row 1: raw scip index](#3-part-b-row-1-raw-scip-index)
4. [Part B row 2: what our resolve consumes from scip](#4-part-b-row-2-what-our-resolve-consumes-from-scip)
5. [Part B row 3: diet_scip == plain --resolve, by construction](#5-part-b-row-3-diet_scip--plain---resolve-by-construction)
6. [Part B row 4: parse resolve (no scip)](#6-part-b-row-4-parse-resolve-no-scip)
7. [Part B row 5: the ratio tables](#7-part-b-row-5-the-ratio-tables)
8. [scip record kinds we never read](#8-scip-record-kinds-we-never-read)
9. [Defects found (file:line)](#9-defects-found-fileline)
10. [What it took to run](#10-what-it-took-to-run)
11. [What stays untested and why](#11-what-stays-untested-and-why)
12. [go call family, corrected](#12-go-call-family-corrected-bare-method-names)
13. [type oracles](#13-type-oracles)
14. [Single process: every "ours" number re-measured](#14-single-process-every-ours-number-re-measured-from-one-process-per-corpus)

## 1. What was measured

| lang | corpus scope | files | oracle |
|---|---|---|---|
| ts | `TypeScript-5.9/src/**`, no `.d.ts` | 600 | `typescript@5.9.3` TypeChecker (`oracle_ts.mjs`) |
| go | `typescript-go/**/*.go`, no test-variant filter | 5,097 (105 `go/packages`) | `golang.org/x/tools/go/callgraph` cha + vta (`oracle_go/`) |
| rust | `rust-analyzer/crates/*/src/**` | 873 (1,481 incl. `test_data`) | rust-analyzer's own scip index, ALSO `ra_ap_ide` call hierarchy |

## 2. Part A: the compiler-native oracles

### ts: TypeScript's own TypeChecker

`oracle_ts.mjs` builds one `ts.createProgram` over all 13 project-reference
subprojects under `src/tsconfig.json` (646 source files once `lib.*.d.ts` is
pulled in), walks every `CallExpression`/`NewExpression`, and resolves it with
`checker.getResolvedSignature(node).declaration`. Imports resolve through
`ts.resolveModuleName` with the same compiler options. Wall: program build
1.4s, full walk 7.3s total, no toolchain beyond `npm install typescript`.

| tsv | rows |
|---|---|
| `ts5.oracle.call.tsv` | 59,356 (after `sort -u`; 84,958 raw resolved call sites) |
| `ts5.oracle.module.tsv` | 2,009 (after `sort -u`; 2,022 raw) |

### go: golang.org/x/tools/go/callgraph

`oracle_go/main.go` runs `packages.Load` (`./...`, 105 packages) + SSA build,
then both `cha.CallGraph` (class-hierarchy, over-approximate) and
`vta.CallGraph` (variable-type-analysis, seeded from cha, tighter). Module
edges come from each file's `ast.File.Imports` resolved through
`packages.Package.Imports`, `dst_path` = the imported package's directory
(matches `go_package_dir`, `src/lang/go.rs:2062`, which also resolves to a
directory not a file). Wall: package load + ssa build + both call graphs
under 30s total, no indexer subprocess.

| tsv | rows |
|---|---|
| `go.oracle.call.cha.tsv` | 172,957 |
| `go.oracle.call.vta.tsv` | 58,332 |
| `go.oracle.module.tsv` | 2,152 |

### rust: rust-analyzer is the compiler, twice over

**Leg 1, scip** (Part B row 1): rust-analyzer's own `scip` subcommand, the
same compiler-resolved index the extraction crate consumes. Counts in
[section 3](#3-part-b-row-1-raw-scip-index).

**Leg 2, `ra_ap_ide` call hierarchy** (`ra_ide_probe/`, crate `ra_ap_ide
0.0.349`): the brief's conditional was "if the lab crate at `dae353d75` shows
it links cheaply, else state the cost and skip". Measured, not guessed:

| step | cost |
|---|---|
| `cargo build --release`, cold cache, `ra_ap_ide` + `ra_ap_load-cargo` + `ra_ap_project_model` + `ra_ap_vfs` | 239 crates total (32 `ra_ap_*`), **1m57s** |
| `ra_ap_load_cargo::load_workspace_at` over rust-analyzer's own 30-crate workspace, `load_out_dirs_from_check: false`, no proc-macro server | **0.65-0.8s** |
| `Analysis::file_structure` (all defs) + `Analysis::outgoing_calls` per def, whole corpus | 1,481 files, 24,213 defs (`SymbolKind::Function \| SymbolKind::Method`), 34,678 raw edges, **12-14s** |

Cheap on every axis. Built it. First cut filtered only `SymbolKind::Function`,
silently dropping every `impl`-block method (Rust's majority call-target
shape): 14,976 defs instead of 24,213. Fixed in `ra_ide_probe/main.rs` before
the numbers below were taken; flagged here because a partial-oracle bug is
worse than no oracle, it just looks like one.

| tsv | rows |
|---|---|
| `rust.oracle.call.tsv` | 27,004 (corpus-internal only; 31,488 raw includes calls into `~/.cargo/registry`) |

## 3. Part B row 1: raw scip index

`extract --family scip --scip-timeout 900 ROOT`, index cached at
`ROOT/.dl/.state/index.scip`.

| lang | scip_def | scip_ref | scip_edge | scip_fn_edge | scip_impl | scip_local | documents |
|---|---:|---:|---:|---:|---:|---:|---:|
| ts | 52,158 | 61,325 | 3,556 | 90,962 | 877 | 0 | 599 |
| rust | 38,537 | 87,914 | 13,509 | 173,502 | 0 | 72,823 | 922 |
| go | 44,140 | 132,844 | 33,964 | 244,055 | 4,763 | 89,226 | 5,103 |

Notes, measured not assumed:
- `scip_local: 0` for ts and `scip_impl: 0` for rust are real: scip-typescript
  never emits `local ` symbols the way rust-analyzer does, and this
  rust-analyzer scip build emits zero implementation relationships over
  `crates/*/src/**` (0 `is_implementation` rows decoded into `scip_impl`).
- ts's `documents: 599` vs `600` files: one `.ts` file under `src/**` produced
  no scip document (not chased further; outside this lane's ownership).
- **ROOT scoping matters and is easy to get wrong.** `extract --family scip
  TypeScript-5.9` (the corpus root, no tsconfig.json there) makes
  `scip-typescript --infer-tsconfig` sweep the WHOLE repo including
  `tests/cases/**`: 21,107 documents, `scip_ref` 2,658,224. The numbers above
  are rerun with `ROOT = TypeScript-5.9/src` (the actual corpus scope, which
  does have its own `tsconfig.json`). No corpus-scope guard exists in the
  tool; the caller must point ROOT at the right marker-file directory.

## 4. Part B row 2: what our resolve consumes from scip

`--resolve --family call --project-root ROOT --scip-index INDEX PATH...`; the
call arm emits `resolved_edge kind=scip_override` wherever the scip-resolved
target disagrees with the plain name match.

Every language's `scip_call_target` (`src/lang/ts.rs:3502`,
`src/lang/rust.rs:1177`, `src/lang/go.rs:2093`, same shape in each) reads
exactly two scip planes: the site's own occurrence (`ScipIndex.documents[..
].occurrences`, the def+ref planes: flat kinds `scip_def`/`scip_ref`) to find
the referenced symbol, then `definition_of` + `containing_def_site` to walk
that symbol to its owning definition. A `local `-prefixed symbol (the
`scip_local` plane) is explicitly rejected (`rust.rs:1188`,
`"a local binding is df-owned; the enclosing fn is NOT the callee"`), never
used to widen the call target. `scip_edge`, `scip_impl`, and `scip_callee_type`
are decoded into flat facts by `src/scip_rows.rs` but no call/type resolve arm
reads them back in.

| lang | resolved_edge total (scip-informed run) | of which `scip_override` | scip_override / scip_fn_edge (row1) |
|---|---:|---:|---:|
| ts | 40,277 (`import_resolve` 234, `name_resolve` 34,897, `value_ref` 2,347, `scip_override` 2,799) | 2,799 | 3.1% |
| rust | 26,332 (`import_resolve` 212, `name_resolve` 25,537, `scip_override` 583) | 583 | 0.3% |
| go | not measured: the scip-informed run fell to the per-directory split floor (`go.parse.scip_override.runs.tsv`, every group `SPLIT_FLOOR`, 1 file each), so no corpus-wide `scip_override` count exists | not measured | not measured |

scip only needs to correct the name-match arm on a small slice of sites
(0.3-3.1% here): most calls in all three corpora are unambiguous enough for
plain name matching to already agree with the compiler on the CALLER side.
This is not the same claim as "parse resolve finds as many edges as scip":
see row 5, where the two arms' RECALL against the real compiler differs a lot
more than this override rate suggests.

## 5. Part B row 3: `diet_scip` == plain `--resolve`, by construction

The brief asked for `--family diet_scip`'s own counts "in scip shape, same
counts as row 1". Corrected here: `diet_scip` does not touch scip data or
emit scip-shaped records at all. `src/project.rs:491` `diet_scip()` calls the
exact same `resolve_project` the plain `--resolve` path calls
(`src/bin/extract.rs:576` `stream_resolve`), with `arms { call: true, types:
true, flow: false }` hardcoded. The code comment at `project.rs:487` says it
outright: **"`--resolve` remains the pre-existing spelling of the same pass
and is byte-unchanged... this family is the labelled entry, not a
replacement."**

Verified empirically, not just read off the comment: `diff <(sort
{lang}.parse.resolve.raw.jsonl) <(sort {lang}.parse.diet_scip.raw.jsonl)` is
EMPTY for all three languages. `<lang>.dietscip.call.tsv` and
`<lang>.parse.call.tsv` are byte-identical after normalization; both are kept
as separate deliverables per the naming convention, not because the data
differs.

## 6. Part B row 4: parse resolve (no scip)

`extract --resolve --family call,type PATH...`, split per top-2-level
directory (crate / subproject / package) to stay under the 10s-per-call law;
zero `SPLIT_FLOOR` events (no group needed a further split) across all three
languages.

| lang | resolved_edge (unique tuples) | resolved_type_edge (unique) | resolved_import | unresolved |
|---|---:|---:|---:|---:|
| ts | 55,611 (`import_resolve` 18,915, `name_resolve` 48,525, `value_ref` 4,030 raw; unique-tuple count lower) | 16,944 | 11,457 (`local` 1,156, `namespace` 403, `star` 9,898) | 0 rows (see caveat) |
| rust | 40,686 | 1,673 | 3,483 (`local` 2,769, `star` 473, `namespace` 156, `indirect` 85) | `ambiguous` 15,184, `no_corpus_def` 65,992 |
| go | 49,082 | 3,178 | **0, always** (see defect 2) | **0, always** (see defect 3) |

ts's `unresolved: 0` is not "everything resolved": it's `call_drops`
(`src/lang/ts.rs:3409`) reporting only the `ambiguous` (star-export
disagreement) bucket by design, and this corpus happened to hit zero of
those. rust's `call_drops` (`src/lang/rust.rs:1305`) reports both `ambiguous`
and `no_corpus_def`, so rust's and ts's `unresolved` counts are NOT
comparable to each other, only internally.

## 7. Part B row 5: the ratio tables

### row4/row1, row2/row1, row3/row1 (row3 == row4, see section 5)

| lang | row4 call / row1 scip_fn_edge | row2 scip_override / row1 scip_fn_edge |
|---|---:|---:|
| ts | 55,611 / 90,962 = **61.1%** | 2,799 / 90,962 = **3.1%** |
| rust | 40,686 / 173,502 = **23.4%** | 583 / 173,502 = **0.3%** |
| go | 49,082 / 244,055 = **20.1%** | not measured (split floor, section 4) |

### SET overlap against the Part A oracle (`bench.py`, call family)

| lang | a | b (oracle) | \|a∩b\| | recall (a∩b/a) | precision (a∩b/b) |
|---|---|---|---:|---:|---:|
| ts | parse resolve (`ts5.parse.call.tsv`) | TypeChecker (`ts5.oracle.call.tsv`) | 35,719 | 64.2% | 60.2% |
| ts | scip-informed resolve (`ts5.scip_override.call.tsv`) | TypeChecker | 20,979 | 69.3% | 35.3% |
| rust | parse resolve (`rust.parse.call.tsv`) | `ra_ap_ide` call hierarchy (`rust.oracle.call.tsv`) | 12,624 | 31.0% | 46.8% |
| rust | scip-informed resolve (`rust.scip_override.call.tsv`) | `ra_ap_ide` call hierarchy | 5,593 | 29.9% | 20.7% |
| go | parse resolve (`go.parse.call.tsv`) | vta (`go.oracle.call.vta.tsv`) | 2,753 | 5.6% | 4.7% |
| go | parse resolve | cha (`go.oracle.call.cha.tsv`) | 2,753 | 5.6% | 1.6% |

Three real findings here, not one:
1. **ts's scip-informed leg has HIGHER recall (69.3% vs 64.2%) but LOWER
   precision (35.3% vs 60.2%) than plain parse resolve against the same
   oracle.** The scip-informed run is a `--family call`-only invocation (no
   `type` arm), which is a smaller left-hand set (30,289 vs 55,611 unique
   rows) that happens to land more of its rows inside the oracle's set,
   consistent with scip correcting the CASES where name-match was already
   going to be wrong, not with scip finding categorically more truth.
2. **rust's overlap with `ra_ap_ide` is far lower than ts's with the
   TypeChecker (31%/47% vs 64%/60%).** Sampled the `apply`/`change.rs`
   disagreement by hand: real cross-package/method-dispatch edges the
   name-match arm doesn't reach, not a naming-convention artifact (both sides
   use bare identifiers, confirmed by grep).
3. **go's overlap with cha/vta is an order of magnitude lower than ts or
   rust (5.6%/1.6-4.7%).** vta and cha are OVER-approximating whole-program
   analyses (every interface-satisfying method is a plausible callee at every
   call through that interface); extract's go arm is a precise per-site name
   match. The two techniques answer different questions at this end of the
   spectrum, so a low overlap here is expected, not diagnostic, unlike the
   rust case above.

### module family (ts only; go/rust below)

| lang | a | b (oracle) | \|a∩b\| | recall | precision |
|---|---|---|---:|---:|---:|
| ts | `ts5.parse.module.tsv` (`resolved_import`, 2,099 unique) | `ts5.oracle.module.tsv` (2,009 unique) | 990 | 47.2% | 49.3% |
| go | N/A, see defect 2 | `go.oracle.module.tsv` | n/a | n/a | n/a |
| rust | not built this lane (scip module edges need a `--scip-deps` join, out of scope for the row-5 ask) | n/a | n/a | n/a |

## 8. scip record kinds we never read

| kind | shape | fact it would give us |
|---|---|---|
| `scip_relationship` | `symbol, related_symbol, is_reference, is_implementation, is_type_definition, is_definition` | trait/interface implementation edges without re-deriving them from occurrences; `scip_impl` is already a flattened slice of this (`src/scip_rows.rs`) but even THAT is never consumed by a resolve arm (grep confirms zero hits in `src/lang/*.rs`) |
| `symbol_roles` (the `roles` bitmask on `scip_occurrence`) | `i32`, plus the decoded bools `write_access`/`read_access`/`test`/`generated`/`forward_definition` | distinguishing a write-site from a read-site at a call/field-access location; a test-only reference from a production one |
| `scip_documentation` | `symbol, pos, text` | doc-comment text keyed to a symbol, for surfacing hover/docs without re-parsing the source |
| `scip_signature`/`scip_signature_occurrence` | `symbol, language, text` / `symbol, ref_symbol, start, end, roles` | a rendered type signature (parameter types, return type) without re-deriving it from the language's own type-annotation AST |
| `enclosing_symbol` (a field on `scip_symbol`) | `string` (parent symbol id) | the containing class/module/impl of a symbol without a separate span-containment join |
| `scip_diagnostic` | `path, start, end, severity, code, message, source, tags` | the compiler's own errors/warnings at a span, e.g. to skip resolving inside code the compiler itself rejected |

## 9. Defects found (file:line)

Ownership is `plans/extract-bench-2026-08-29/**` only; these are report rows,
not fixes.

1. **`src/scip_ensure.rs:60-103`, `INDEXERS` roster.** ts markers are
   `["tsconfig.json", "package.json"]` (line 70); go markers are `["go.mod"]`
   (line 84). `typescript-go`'s repo root carries BOTH a `go.mod` (the real
   corpus) and a `package.json` (VS Code extension tooling only, no
   meaningful `.ts` source at the root). `--family scip
   /path/to/typescript-go` ran `scip-typescript --infer-tsconfig` first,
   which swept the whole non-TS repo and burned 9+ minutes of CPU before it
   was killed by hand, never reaching `scip-go`'s turn inside the 900s
   budget. Worked around here with a read-only scratch copy of the corpus
   with `package.json` excluded (`rsync --exclude`); the corpus itself was
   never written to. A symlink-mirror alternative (no copy) was tried first
   and failed for an unrelated reason: `go list ./...` returns zero packages
   through a symlinked module root, a Go tooling limitation, not an `extract`
   one.
2. **`src/project.rs:1064-1085`, `import_facts`.** Chains only
   `cx.indexes.ts_modules` and `cx.indexes.rust_modules`; there is no
   `go_modules` arm (confirmed: zero hits for `go_modules`/`GoModuleIndex`
   anywhere in `src/`). `--resolve --family type` (which carries
   `resolved_import`) emits exactly 0 rows for go, regardless of corpus size:
   confirmed empirically over all 5,097 files / 57 groups, not inferred
   from the code alone.
3. **`src/project.rs:857-861`, go's `RESOLVE_ARMS` entry, `drops: None`.**
   The `unresolved` record is never emitted for go under `--resolve`; the
   per-language drop-classification callback is simply unset, unlike ts
   (`src/lang/ts.rs:3409`) and rust (`src/lang/rust.rs:1305`).
4. **`--family scip ROOT` has no corpus-scope guard.** Pointing ROOT at
   `TypeScript-5.9` (the repo root, no `tsconfig.json` there) instead of
   `TypeScript-5.9/src` (the actual corpus, which has one) makes
   `--infer-tsconfig` silently index 21,107 documents instead of 599, a
   35x larger, wrong-scope run that still exits 0 with no `scip_skip`
   warning. Not a crash, not caught by any existing check; a caller who
   doesn't already know the corpus's own scope has no signal this happened
   short of eyeballing the `documents` count on `scip_index`.

## 10. What it took to run

| step | wall | install |
|---|---|---|
| `cargo build --release --features cli` (extract) | over 120s (moved to background by the harness), done by the next check | none |
| `npm install typescript@5.9.3` in the lab dir | a few s | `npm`, network |
| `oracle_ts.mjs` full run (program + walk + module resolve) | 7.3s | none |
| `go mod init` + `go get golang.org/x/tools` + `go mod tidy` | a few s | `go`, network |
| `oracle_go` full run (packages.Load + ssa + cha + vta) | well under 1 min | none |
| `go install github.com/scip-code/scip-go/cmd/scip-go@latest` | ~1 min | `go`, network; installs to `$(go env GOPATH)/bin`, NOT auto-added to a subprocess's `PATH` |
| `extract --family scip` over `TypeScript-5.9/src` (599 docs) | a few min | `scip-typescript` (already on PATH via nvm) |
| `extract --family scip` over the go scratch copy (5,103 docs) | several min | `scip-go`, PLUS the rsync copy (336M) to route around defect 1 |
| `extract --family scip` over `rust-analyzer` | reused an existing `.dl/.state/index.scip` (built by a sibling lane), a few s | `rust-analyzer` (already on PATH) |
| `ra_ide_probe` cold build (`ra_ap_ide` + `ra_ap_load-cargo` + friends) | 1m57s, 239 crates | none beyond `cargo` |
| `ra_ide_probe` run (load + full-corpus call hierarchy) | 12-14s | none |
| `--resolve`/`--family diet_scip` parse runs, all 3 corpora, chunked | each well under 1 min | none |
| `--resolve --scip-index` (scip-informed) runs, all 3 corpora | several min each, every chunk reloads and re-decodes the whole scip index from disk, no caching across the chunked invocations this lane's driver makes | none |

## 11. What stays untested and why

- **rust module family against an oracle.** rust-analyzer's scip index
  doesn't carry a ready file-edge relation the way `--scip-deps` folds one
  for ts/go; building one needs the same `scip_edge`/symbol-crossing join
  `--scip-deps` already does inside the crate, which is a `src/` change and
  out of this lane's ownership.
- **A full-corpus `type` family oracle comparison.** The TypeChecker and
  `ra_ap_ide` oracles both only cover `call` + `module`; extending
  `oracle_ts.mjs`/`ra_ide_probe` to also resolve every type reference
  (`resolved_type_edge`'s `field`/`impl`/`variant`/`generic`/`uses`/`returns`
  taxonomy) is a second walker, not a rerun of this one.
- **go's `--deps`/`--scip-deps` module family**, only `--resolve`'s (absent,
  defect 2) was measured; `--deps` is a different code path this brief didn't
  ask for.

## 11. ts module family, corrected: `--deps` file_edge is the file-imports-file row

Section 7's 47% module overlap compared two different rows. `resolved_import`
is the BINDING target through barrels (`binder.ts -> checker.ts` via
`_namespaces/ts.ts`, `export *`); madge, dependency-cruiser, codeql,
stack-graphs and scip all emit file-imports-file (`binder.ts ->
_namespaces/ts.ts`). The file-imports-file row already exists:
`extract --deps --project-root <root> <files>` emits `file_edge`.

| a | b | \|a\| | \|b\| | a∩b | a-only | b-only |
|---|---|---:|---:|---:|---:|---:|
| `ts5.deps.module.tsv` (`--deps` file_edge, 600 src files) | `ts.madge.module.tsv` | 2,010 | 2,011 | 2,010 | 0 | 1 (`src/../scripts/failed-tests.d.cts`, outside `src/`) |

Command: `extract --deps --project-root ~/projects/TypeScript-5.9 $(find src -name '*.ts' ! -name '*.d.ts')`, 600 files, wall under 60 s in one process.
Both rows stay: `file_edge` answers "what does this file import", `resolved_import` answers "where does this name come from".

## 12. go call family, corrected: bare method names

Section 7's go call recall (5.6%) compared `Checker.GetAliasedSymbol`
(`go/callgraph` writes `Type.Method`) against `GetAliasedSymbol` (ours writes
the bare name). With the receiver prefix stripped on the oracle side
(`go.oracle.call.vta.bare.tsv`):

| ours (`go.parse.call.tsv`, pre-#558 binary) | oracle | both | recall | precision |
|---|---:|---:|---:|---:|
| 49,082 vs vta 55,099 | | 24,980 | **45.3%** | 50.9% |
| 49,082 vs cha 169,232 | | 25,177 | 14.9% | 51.3% |

4,132 of our rows carry a `closure@<n>` caller the oracle names by the
enclosing function; those and the fourslash test harness
(`internal/fourslash/fourslash.go`, 11,559 ours-only rows) are the bulk of
ours-only. `go.parse.call.tsv` predates #558, #560 and #562; the receipt
lane for the next go arc re-emits it on the current binary.

### 12a. concrete-one-hop-receiver excess vs codeql2: codeql-side (2026-08-30)

The `ours - codeql2` full-projection excess holds 1,348 rows of the class
(`out/go.excess.codeql2.classes.tsv`, full set). Each row's call site was
loaded with `go/types` (the `go_gap_classify` `packages.Load` scaffold, 105
packages) and the callee asserted against the receiver's type
(`types.LookupFieldOrMethod`): 1,290 of 1,348 assert-passes (95.7%), well over
the 90% bar. The class is codeql-side — both endpoints exist and the binding
is concrete; codeql2's caller simply lacks the edge (its callers carry other
edges in `go.codeql2.call.tsv`). Written into GO-PARITY.REPORT.md section 5a
instead of touching the extractor. The 58 residual rows (4.3%) are wrong-file
bindings folded into the corpus-scale defect documented there.

## 13. type oracles

Corpora and shas unchanged from section 1. Two new probes, both extensions of
the existing binaries in this directory.

### 13.1 The normal form, and where it is ambiguous

`go.parse.type.tsv`, `rust.parse.type.tsv` and their `dietscip` twins carry
FOUR columns, not five: `normalize.py:37-40` drops `resolved_type_edge`'s
`kind` on the way out. The brief's fifth `kind` column therefore cannot ride in
the file `bench.py` joins on, since `bench.py` compares whole lines
(`bench.py:9-15`). Reading picked, stated here:

| file | columns | joins with `bench.py` |
|---|---|---|
| `go.oracle.type.tsv`, `rust.oracle.type.tsv` | 4 | yes, against `*.parse.type.tsv` |
| `go.oracle.type.kinds.tsv`, `rust.oracle.type.kinds.tsv` | 5, `kind` last | no, per-kind slicing only |
| `go.oracle.type.typedecl.tsv`, `rust.oracle.type.typedecl.tsv` | 4, owners restricted to type declarations | yes |

Five rows of `go.parse.type.tsv` fixing the src/dst reading:

```
internal/api/proto.go	APIFileChangeSummary	internal/api/proto.go	DocumentIdentifier
internal/api/proto.go	APIFileChanges	internal/api/proto.go	DocumentIdentifier
_tools/customlint/testdata/unexportedapi/unexportedapi.go	Foo	..	oops
_tools/customlint/testdata/unexportedapi/unexportedapi.go	OkayEmbed	..	unexported
crates/base-db/src/change.rs	FileChange	crates/base-db/src/input.rs	CrateGraphBuilder
```

src_name is the DECLARING entity, dst_name the referenced type, both bare.
Every src_name in both our files is a TYPE name; neither language's extractor
emits a fn-owned type reference. `go.rs:296-301` says so outright ("Method/fn
SIGNATURES are NOT edge sources for go"); the rust arm
(`rust.rs:635-693`) walks `Item::Struct`, `Enum`, `Union`, `Trait` and `Impl`
only. That is why the `typedecl` column exists: the full oracle set is a
different question from the one our extractor answers, and both numbers are
reported.

`*.dietscip.type.tsv` is BYTE-IDENTICAL to `*.parse.type.tsv` for all three
languages (`cmp` returns identical). Every "diet scip recall" column below
therefore repeats its `parse` neighbour exactly; the scip index widens the
`call` family only.

### 13.2 go, via `go/types`

`oracle_go/main.go` gained `writeTypeEdges` (`main.go:258-400`). Run:
`./oracle_go/oracle_go ~/projects/typescript-go <outDir> type`.

| kind | how it is derived | code |
|---|---|---|
| `ref` | every `*ast.Ident` whose `TypesInfo.Uses` entry is a `*types.TypeName` that is not a `*types.TypeParam`, keyed on the enclosing top-level decl (fn name, type-spec name, or first value-spec name) | `main.go:227-248` |
| `implements` | `types.Implements(T, I)` or `types.Implements(*T, I)` over every non-generic package-level named type x every named interface with at least one method | `main.go:326-348` |
| `extends` | every `Anonymous()` field of a named struct, pointer-unwrapped to its `TypeName` | `main.go:350-376` |

Cost of the `implements` sweep: **2,413 named types x 98 interfaces =
236,474 pairs**, 929 of which hold. Whole run, load included, **2.26s wall**.

| lang | kind | oracle rows | ours rows | overlap | recall | precision | diet scip recall | wall |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| go | all three, full | 44,214 | 3,178 | 3,102 | 7.0% | 97.6% | 7.0% | 2.26s |
| go | all three, type-decl owners only | 6,204 | 3,178 | 3,102 | 50.0% | 97.6% | 50.0% | 2.26s |
| go | `ref` | 43,323 | 3,178 | 3,102 | 7.2% | 97.6% | 7.2% | 2.26s |
| go | `implements` | 929 | 3,178 | 11 | 1.2% | 0.4% | 1.2% | 2.26s |
| go | `extends` | 617 | 3,178 | 563 | 91.3% | 17.7% | 91.3% | 2.26s |

Recall is against the oracle's own row set per kind; precision is
`overlap / |ours|`, so the per-kind precision columns are each a slice of one
3,178-row denominator and only the "all three" row's 97.6% is meaningful.

**Ours-only, 76 rows, classed.** These are rows the oracle cannot see, plus a
small real defect:

| class | rows | why |
|---|---:|---|
| testfile | 23 | `packages.Config` carries no `Tests: true`, so `_test.go` is not loaded |
| testdata | 18 | `go build ./...` skips `testdata/`; the whole `_tools/customlint/testdata` tree is invisible to the oracle |
| buildtag-goos | 15 | `internal/fswatch/{fanotify_linux,inotify_linux,windows}.go`, gated off on darwin |
| buildtag-ignore | 5 | `internal/diagnostics/generate.go` carries `//go:build ignore` |
| local | 10 | function-local `type` decls (`internal/core/bfs.go:76` `type result struct`); the oracle keys them on the enclosing fn |
| wrong-file | 5 | ours points dst_path at the REFERRING file, not the declaring one |

The five wrong-file rows, oracle dst on the right:

| row | ours dst_path | oracle dst_path |
|---|---|---|
| `ast.go ModifierList -> ModifierFlags` | `internal/ast/ast.go` | `internal/ast/modifierflags.go` |
| `ast.go PatternAmbientModule -> Symbol` | `internal/ast/ast.go` | `internal/ast/symbol.go` |
| `emitcontext.go emitNode -> EmitFlags` | `internal/printer/emitcontext.go` | `internal/printer/emitflags.go` |
| `projectcollection.go ProjectCollection -> ConfigFileRegistry` | `internal/project/projectcollection.go` | `internal/project/configfileregistry.go` |
| `session.go Session -> Snapshot` | `internal/project/session.go` | `internal/project/snapshot.go` |

`ModifierList` at `internal/ast/ast.go:154-157` is `struct { NodeList;
ModifierFlags ModifierFlags }`: a field whose NAME equals the referenced
TYPE's name. Ours binds the dst to the same-file field rather than to the
type declared in `modifierflags.go`. Same shape in the other four.

**Oracle-only, 3,102 rows against the typedecl set, classed.** Sums exceed
3,102 because one 4-col row can carry two kinds.

| class | rows | evidence |
|---|---:|---|
| chunk | 1,908 | the row crosses a top-2-level directory boundary. EVERY one of ours' 559 cross-file hits stays inside one top-2 dir and ZERO cross it, so this is the chunked driver (`--resolve` run split per top-2-level dir, section 8), not the extractor |
| implements | 918 | ours computes no interface-satisfaction relation at all; its `TypeEdgeKind::Impl` is embedded fields and interface `type_elem`s (`go.rs:354-368`) |
| cross-file-in-chunk | 899 | cross-file, same top-2 dir, so the chunk cannot explain it |
| signature-or-alias | 185 | same-file, kind `ref`: type aliases (`internal/ast/ast.go:1262` `AnyImportOrRequireStatement = Node`, dropped by `go.rs:369` `_ => {}`) and interface method signatures |
| extends | 54 | all 54 cross-file, so a subset of chunk |

10 oracle-only rows, sampled every 4,000th, with a one-word class:

| row | class |
|---|---|
| `cmd/tsgo/api.go parseAPIFlags -> apiFlags` | fn-owner |
| `internal/ast/ast_generated.go UpdateBindingElement -> PropertyName` | fn-owner |
| `internal/checker/checker.go extendExportSymbols -> ExportCollisionTable` | fn-owner |
| `internal/checker/inference.go addIntraExpressionInferenceSite -> types.go Type` | fn-owner |
| `internal/compiler/projectreferencefilemapper.go getSourceToDtsIfSymlink -> tsoptions/parsedcommandline.go SourceOutputAndProjectReference` | fn-owner |
| `internal/execute/tsctests/sys.go WatchBackend -> watchmanager/watchbackend.go WatchBackend` | chunk |
| `internal/ls/completions.go getSingleLine... -> lsproto/lsp_generated.go Range` | fn-owner |
| `internal/lsp/lsproto/lsp_generated.go UnmarshalJSONFrom -> DidChangeConfigurationParams` | fn-owner |
| `internal/printer/factory.go NewExternalModuleExport -> ast/ast.go Node` | fn-owner |
| `internal/testutil/harnessutil/harnessutil.go CompileFilesEx -> HarnessOptions` | fn-owner |

### 13.3 rust, via `ra_ap_ide` + `ra_ap_hir`

`ra_ide_probe/main.rs` gained `write_type_edges` (`main.rs:149-235`) and three
deps (`ra_ap_hir`, `ra_ap_syntax`, plus a `[[bin]]` target the manifest lacked).
Run: `./ra_ide_probe/target/release/ra_ide_probe ~/projects/rust-analyzer 900 <outDir> type`.

| kind | how it is derived | code |
|---|---|---|
| `ref` | every `ast::Path` node in the file, resolved with `Semantics::resolve_path`, kept when it lands on `ModuleDef::Adt`, `Trait` or `TypeAlias`; dst file and name off `TryToNav::try_to_nav` | `main.rs:176-192` |
| `implements` | every `ast::Impl` with a `trait_`, `Semantics::to_impl_def` then `hir::Impl::trait_` and `self_ty(db).as_adt()` | `main.rs:193-206` |
| `extends` | not emitted; rust has no such relation | |

Two defects hit on the way and both are recorded, since either one silently
zeroes the probe:

| symptom | cause | fix |
|---|---|---|
| `no method named 'name' found for struct ast::Struct`, 9 times | `ast::HasName` is a trait and was not in scope | `main.rs:9` imports `ast::HasName` |
| panic `Try to use attached db, but not db is attached` at `ra_ap_hir_ty-0.0.349/src/next_solver/interner.rs:2487`, on the FIRST `resolve_path` | the next-solver interner reads a thread-local db; `Analysis::with_db` wraps every ide entry point in `hir::attach_db` (`ra_ap_ide-0.0.349/src/lib.rs:975`) and a hand-rolled `Semantics` walk gets no such wrapper | `main.rs:172` wraps the whole file loop in `attach_db(db, ...)` |

`resolve_path` fired **78,217** times and produced 42,950 distinct `ref` rows;
739 `impl Trait for Type` blocks resolved with both legs inside the corpus.

| lang | kind | oracle rows | ours rows | overlap | recall | precision | diet scip recall | wall |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| rust | both, full (1,481 files in vfs) | 43,134 | 1,673 | 1,646 | 3.8% | 98.4% | 3.8% | 11.0s |
| rust | both, type-decl owners only | 8,343 | 1,673 | 1,646 | 19.7% | 98.4% | 19.7% | 11.0s |
| rust | type-decl owners, `crates/*/src` only | 8,151 | 1,673 | 1,646 | 20.2% | 98.4% | 20.2% | 11.0s |
| rust | type-decl owners, `crates/*/src`, self-edges dropped | 6,756 | 1,673 | 1,637 | **24.2%** | 97.9% | 24.2% | 11.0s |
| rust | `ref` | 42,950 | 1,673 | 1,646 | 3.8% | 98.4% | 3.8% | 11.0s |
| rust | `implements` | 739 | 1,673 | 322 | **43.6%** | 19.3% | 43.6% | 11.0s |

The self-edge row matters: an `impl Foo { .. }` block makes `owner_of` name the
block `Foo` and the path `Foo` in its header resolve to `Foo`, so the oracle
mints 1,395 `X -> X` rows in the restricted set that ours never mints (9). They
are an artifact of the oracle, not a miss, so 24.2% is the number to quote.
`implements` at 43.6% is the one kind where the two sides ask the same question:
`rust.rs:678-686` binds `Item::Impl`'s `trait_` path exactly as
`hir::Impl::trait_` does. go's 1.2% is not comparable, because go's
`TypeEdgeKind::Impl` is embedding, not interface satisfaction.

**Ours-only, 27 rows.** 17 of them are the same defect as go's wrong-file
class, and rust's version is worse because the wrong file is in a different
crate:

| row | ours dst_path | oracle dst_path |
|---|---|---|
| `hir-ty/src/infer.rs InferenceContext -> Resolver` (6 rows share this dst) | `crates/hir-ty/src/infer/unify.rs` | `crates/hir-def/src/resolver.rs` |
| `hir-def/src/lib.rs ProcMacroLoc -> ProcMacroKind` | `crates/hir-def/src/nameres/proc_macro.rs` | `crates/hir-expand/src/proc_macro.rs` |
| `hir-ty/src/target_feature.rs TargetFeatures -> Symbol` | `crates/hir-ty/src/next_solver/interner.rs` | `crates/intern/src/symbol.rs` |

The remaining 10 bind a std name to a same-named local type, e.g.
`crates/syntax/src/syntax_error.rs SyntaxError -> ast/generated/tokens.rs
String`: the referenced type is `std::string::String`, and ours picked the
syntax crate's `String` TOKEN node of the same name.

**Oracle-only, 5,119 rows** (against the self-edge-free restricted set):

| class | rows | evidence |
|---|---:|---|
| chunk | 2,009 | crosses a crate boundary. `rust.parse.type.tsv` contains **ZERO** cross-crate rows out of 1,673, so no cross-crate row is reachable at all under the chunked driver |
| same-crate, cross-file | 1,689 | ours DOES resolve cross-file inside a chunk (805 of its 1,646 hits are cross-file), so these are extractor gaps, not driver ones |
| same-crate, same-file | 1,421 | dominated by impl-block bodies and enum variant payload types, neither of which `rust.rs` walks |

10 oracle-only rows, sampled every 260th of the same-crate set, one-word class:

| row | class |
|---|---|
| `base-db/src/input.rs BuiltCrateData -> Crate` | impl-body |
| `hir-def/src/import_map.rs ImportMapIndex -> lib.rs FxIndexMap` | alias |
| `hir-def/src/resolver.rs ImplId -> lib.rs ImplId` | impl-body |
| `hir-ty/src/infer.rs InferenceContext -> next_solver.rs DefaultAny` | impl-body |
| `hir-ty/src/next_solver/consts.rs BoundConst -> interner.rs DbInterner` | impl-body |
| `hir-ty/src/next_solver/interner.rs CoroutineClosureId -> def_id.rs CoroutineClosureIdWrapper` | impl-body |
| `hir/src/from_id.rs ModuleDefId -> lib.rs ModuleDef` | impl-body |
| `ide-ssr/src/lib.rs SsrRule -> parsing.rs ParsedRule` | field |
| `rust-analyzer/src/flycheck.rs Event -> StateChange` | enum-variant |
| `syntax/src/ast/generated/nodes.rs GenericParam -> LifetimeParam` | enum-variant |

`rust.rs:645-660` states the enum-variant gap outright: the variant edge is
minted as the synthetic text `Enum::Variant`, and `rust.parse.type.tsv` carries
ZERO rows with `::` in dst_name, so every one of them died unresolved.

### 13.4 The two findings worth acting on

| finding | evidence | who owns it |
|---|---|---|
| The chunked `--resolve` driver, not the extractor, caps both languages. go: every one of ours' 559 cross-file hits stays inside one top-2-level dir, ZERO cross it, and 1,908 oracle rows do. rust: ZERO of 1,673 rows cross a crate. | sections 13.2, 13.3 | the bench driver, `resolve_runs.py`, not `src/` |
| dst_path binds to the referring file when a same-named field or token exists there. 5 rows in go, 17 in rust. `internal/ast/ast.go:154-157` `ModifierFlags ModifierFlags` is the minimal case: a field whose name equals the referenced type's name. | sections 13.2, 13.3 | `src/`, a resolve arm; out of this lane's ownership |

### 13.5 What it took to run, added rows

| step | wall | needs |
|---|---|---|
| `oracle_go` rebuild with `writeTypeEdges` | under 5s | `go`, no network (deps already in the module cache) |
| `oracle_go ... type` over `typescript-go` | 2.26s, 236,474 `types.Implements` pairs included | none |
| `ra_ide_probe` cold rebuild after adding `ra_ap_hir` + `ra_ap_syntax` | **380.82s**, 239 crates, `ra_ap_hir_def` and `ra_ap_hir_ty` the long poles | `cargo`; the crates were already in `~/.cargo/registry/src`, no network |
| `ra_ide_probe` incremental rebuild (one-line fixes) | under 1s | none |
| `ra_ide_probe ... type` over `rust-analyzer` | 12.04s total, 11.00s in the walk, workspace load 0.47s from warm caches | none |

## 14. Single process: every "ours" number re-measured from ONE process per corpus

Binary rebuilt from this worktree at `f5fd6dbf9`, `cargo build --release
--features cli`, 2m43s. Corpora and shas unchanged from section 1
(TypeScript-5.9 `7e133bea1` 600 files, typescript-go `89d5d5b2` 5,097,
rust-analyzer `af4111f` 873). Recall is `overlap / |oracle|` and precision is
`overlap / |ours|`, section 13's convention; `bench.py` prints those two labels
the other way round (`bench.py:38-41`) and section 7's table carries its
spelling, so section 7's "recall" column is this section's "precision".

### 14.1 There is no `module` resolve arm

`parse_arms` accepts `call`, `type`/`types` and `flow` only
(`v6/sprefa-extract/src/bin/extract.rs:599-618`); `--family module` is a named
stop. The module row rides the other two: `import_facts` runs under
`request.arms.call || request.arms.types` (`v6/sprefa-extract/src/project.rs:346-351`),
so `--resolve --family call,type` emits all three families in one process.
`normalize.py` gained a `module` mode that folds `resolved_import` to the
4-column normal form with both names empty, matching the committed
`go.parse.module.tsv` shape.

### 14.2 What one process costs

`/usr/bin/time -l`, three runs per corpus per mode, median wall, max peak RSS.
`<lang>.<mode>.single.runs.tsv` carries one line per invocation, so `wc -l` on
it is the process count: 1 for every run below.

| corpus | files | mode | walls (ms) | median | peak RSS | JSONL lines | rc |
|---|---:|---|---|---:|---:|---:|---:|
| typescript-go | 5,097 | resolve | 9522 / 9379 / 8190 | **9.38 s** | 666 MB | 159,551 | 0 |
| typescript-go | 5,097 | diet_scip | 8987 / 8507 / 8723 | 8.72 s | 645 MB | 159,551 | 0 |
| TypeScript-5.9 | 600 | resolve | 2482 / 2449 / 2483 | **2.48 s** | 409 MB | 123,229 | 0 |
| TypeScript-5.9 | 600 | diet_scip | 3067 / 2504 / 2806 | 2.81 s | 403 MB | 123,229 | 0 |
| rust-analyzer | 873 | resolve | 2180 / 2137 / 2287 | **2.18 s** | 536 MB | 157,297 | 0 |
| rust-analyzer | 873 | diet_scip | 2377 / 2322 / 2431 | 2.38 s | 536 MB | 157,297 | 0 |

No invocation crossed 10 s, so no run is a 10-second-law defect and none needed
a fix. go is the closest at 9.52 s worst of three, so a 5 s `sample` was taken
anyway; the run is tree-sitter bound, not resolve-index bound. Top frames by
top-of-stack samples, worker threads parked in `__psynch_cvwait` (19,014)
excluded:

| frame | samples |
|---|---:|
| `ts_tree_cursor_child_iterator_next` | 1,576 |
| `ts_parser_parse` | 1,113 |
| `_nanov2_free` (libsystem_malloc) | 585 |
| `ts_lex` | 515 |
| `ts_language_next_state` | 419 |

The whole path list rides argv: go is the largest at 331 KB relative / ~400 KB
absolute, and `ARG_MAX` on this machine is 1,048,576, so no `xargs` and no
batching is needed at these corpus sizes.

### 14.3 The chunked driver emits ZERO cross-partition rows, in every family

`resolve_runs.py --chunk` on the SAME binary is the control, so the columns
below isolate the driver rather than binary drift: 550 processes for go, 147
for ts5, 36 for rust (`<lang>.resolve.chunked.runs.tsv`).

| lang | family | boundary | control cross-partition | of | single cross-partition | of |
|---|---|---|---:|---:|---:|---:|
| go | call | top-2 dir | **0** | 23,837 | 34,159 | 84,622 |
| go | type | top-2 dir | **0** | 2,725 | 1,186 | 4,411 |
| go | module | top-2 dir | 8,350 | 14,175 | 8,350 | 14,175 |
| ts5 | call | top-2 dir | **0** | 35,540 | 8,391 | 59,311 |
| ts5 | type | top-2 dir | **0** | 6,652 | 4,964 | 20,317 |
| ts5 | module | top-2 dir | **0** | 619 | 997 | 3,096 |
| rust | call | crate | **0** | 40,867 | 16,337 | 48,020 |
| rust | type | crate | **0** | 1,679 | 1,343 | 2,946 |
| rust | module | crate | **0** | 1,871 | 2,680 | 4,465 |

go's module row is the exception and it is the fix landing: PR #560 made a
`resolved_import` row depend on `go_package_dir` alone rather than on which
files the invocation carries (docs/failure-modes.md entry 96), so the control
and the single-process run agree on all 14,175 rows including the 8,350 that
cross a top-2 directory. Every other family in every language still loses
100% of its cross-partition rows to the split.

Splitting also MINTS rows a whole-corpus run declines. rust's control carries
9,184 rows the single-process run does not, across 465 distinct callee names,
led by `text_range` (376), `iter` (351), `kind` (310), `insert` (302), `map`
(280): a chunk makes an ambiguous method name look unique, and the name-match
arm binds it. Chunked precision is therefore inflated as well as chunked
recall being a floor.

### 14.4 Every "ours" row, chunked control beside single process


#### go, one process, 9.38 s wall, 666 MB peak RSS

| family | oracle / tool | oracle rows | ours chunked | chunked recall | chunked precision | ours single | single recall | single precision |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| call | `go.oracle.call.vta.bare.tsv` | 55099 | 23837 | 32.1% | 74.2% | 84622 | 75.9% | 49.4% |
| call | `go.oracle.call.cha.tsv` | 172957 | 23837 | 1.3% | 9.3% | 84622 | 3.0% | 6.2% |
| call | `go.codeql2.call.tsv` | 48529 | 23837 | 37.1% | 75.5% | 84622 | 88.7% | 50.9% |
| call | `go.joern2.call.tsv` | 31617 | 23837 | 32.0% | 42.4% | 84622 | 83.1% | 31.0% |
| type | `go.oracle.type.tsv` | 44214 | 2725 | 6.0% | 97.9% | 4411 | 9.7% | 96.8% |
| type | `go.oracle.type.typedecl.tsv` | 6204 | 2725 | 43.0% | 97.9% | 4411 | 68.9% | 96.8% |
| module | `go.oracle.module.tsv` | 2152 | 14175 | 100.0% | 15.2% | 14175 | 100.0% | 15.2% |

#### ts5, one process, 2.48 s wall, 408 MB peak RSS

| family | oracle / tool | oracle rows | ours chunked | chunked recall | chunked precision | ours single | single recall | single precision |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| call | `ts5.oracle.call.tsv` | 59356 | 35540 | 39.6% | 66.1% | 59311 | 70.0% | 70.0% |
| call | `ts.codeql2.call.tsv` | 53140 | 35540 | 42.6% | 63.7% | 59311 | 72.9% | 65.3% |
| call | `ts.joern2.call.tsv` | 24451 | 35540 | 58.1% | 40.0% | 59311 | 58.7% | 24.2% |
| type | no oracle in this lab | n/a | 6652 | n/a | n/a | 20317 | n/a | n/a |
| module | `ts5.oracle.module.tsv` | 2009 | 619 | 30.8% | 100.0% | 3096 | 50.6% | 32.8% |
| module | `ts.madge.module.tsv` | 2011 | 619 | 30.8% | 100.0% | 3096 | 50.6% | 32.8% |
| module | `ts.depcruise.module.tsv` | 2011 | 619 | 30.8% | 100.0% | 3096 | 50.6% | 32.9% |
| module | `ts.stackgraphs.module.tsv` | 1208 | 619 | 19.0% | 37.0% | 3096 | 38.5% | 15.0% |
| module | `ts.codeql.module.tsv` | 1477 | 619 | 41.9% | 100.0% | 3096 | 68.9% | 32.8% |

#### rust, one process, 2.18 s wall, 536 MB peak RSS

| family | oracle / tool | oracle rows | ours chunked | chunked recall | chunked precision | ours single | single recall | single precision |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| call | `rust.oracle.call.tsv` | 27004 | 40867 | 46.8% | 30.9% | 48020 | 50.4% | 28.3% |
| type | `rust.oracle.type.tsv` | 43134 | 1679 | 3.8% | 98.0% | 2946 | 5.1% | 74.1% |
| type | `rust.oracle.type.typedecl.tsv` | 8343 | 1679 | 19.7% | 98.0% | 2946 | 26.2% | 74.1% |
| module | no oracle in this lab | n/a | 1871 | n/a | n/a | 4465 | n/a | n/a |

#### the committed `*.chunked.tsv` files, against the same oracles

| lang | family | committed chunked rows | oracle | recall | precision |
|---|---|---:|---|---:|---:|
| go | call | 84618 | `go.oracle.call.vta.bare.tsv` | 75.9% | 49.4% |
| go | call | 84618 | `go.oracle.call.cha.tsv` | 3.0% | 6.2% |
| go | call | 84618 | `go.codeql2.call.tsv` | 88.7% | 50.9% |
| go | call | 84618 | `go.joern2.call.tsv` | 83.1% | 31.0% |
| go | type | 3178 | `go.oracle.type.tsv` | 7.0% | 97.6% |
| go | type | 3178 | `go.oracle.type.typedecl.tsv` | 50.0% | 97.6% |
| go | module | 14173 | `go.oracle.module.tsv` | 100.0% | 15.2% |
| ts5 | call | 59311 | `ts5.oracle.call.tsv` | 70.0% | 70.0% |
| ts5 | call | 59311 | `ts.codeql2.call.tsv` | 72.9% | 65.3% |
| ts5 | call | 59311 | `ts.joern2.call.tsv` | 58.7% | 24.2% |
| ts5 | module | 2099 | `ts5.oracle.module.tsv` | 49.3% | 47.2% |
| ts5 | module | 2099 | `ts.madge.module.tsv` | 49.2% | 47.2% |
| ts5 | module | 2099 | `ts.depcruise.module.tsv` | 49.3% | 47.2% |
| ts5 | module | 2099 | `ts.stackgraphs.module.tsv` | 37.7% | 21.7% |
| ts5 | module | 2099 | `ts.codeql.module.tsv` | 67.0% | 47.2% |
| rust | call | 40686 | `rust.oracle.call.tsv` | 46.7% | 31.0% |
| rust | type | 1673 | `rust.oracle.type.tsv` | 3.8% | 98.4% |
| rust | type | 1673 | `rust.oracle.type.typedecl.tsv` | 19.7% | 98.4% |

### 14.5 Two committed files already carried single-process numbers

`go.parse.call.tsv` and `ts5.parse.call.tsv` at `f5fd6dbf9` were re-emitted by
the type-oracle lane on a post-#565/#566/#567 binary and are NOT chunked
artifacts: `go.parse.call.chunked.tsv` carries 34,158 rows crossing a top-2
directory, and its single-process re-run adds only 4 rows. `ts5.parse.call`
reproduced byte-for-byte, 59,311 rows, zero either way. The genuinely chunked
committed artifacts are the `dietscip` twins and the `type`/`module` files:
`go.dietscip.call.chunked.tsv` has 0 cross-top-2 rows out of 49,082.

| committed file | rows | chunked? | single-process rows |
|---|---:|---|---:|
| `go.parse.call.chunked.tsv` | 84,618 | no | 84,622 |
| `ts5.parse.call.chunked.tsv` | 59,311 | no | 59,311 |
| `go.dietscip.call.chunked.tsv` | 49,082 | yes | 84,622 |
| `ts5.dietscip.call.chunked.tsv` | 55,611 | yes | 59,311 |
| `rust.parse.call.chunked.tsv` | 40,686 | yes | 48,020 |
| `go.parse.type.chunked.tsv` | 3,178 | yes | 4,411 |
| `ts5.parse.type.chunked.tsv` | 16,944 | yes | 20,317 |
| `rust.parse.type.chunked.tsv` | 1,673 | yes | 2,946 |
| `go.parse.module.chunked.tsv` | 14,173 | yes, and chunk-immune | 14,175 |
| `ts5.parse.module.chunked.tsv` | 2,099 | yes | 3,096 |
| `rust.parse.module` | absent | n/a | 4,465, new this lane |

`*.dietscip.*` is byte-identical to `*.parse.*` for all three languages under
one process too, both families, so section 5's "by construction" claim holds at
whole-corpus scale and every diet-scip column repeats its parse neighbour.

### 14.6 Where the headline numbers moved

Against each language's primary oracle, chunked control to single process:

| lang | family | oracle | chunked recall | single recall | chunked precision | single precision |
|---|---|---|---:|---:|---:|---:|
| go | call | `go.oracle.call.vta.bare.tsv` | 32.1% | **75.9%** | 74.2% | 49.4% |
| go | type | `go.oracle.type.typedecl.tsv` | 43.0% | **68.9%** | 97.9% | 96.8% |
| go | module | `go.oracle.module.tsv` | 100.0% | 100.0% | 15.2% | 15.2% |
| ts5 | call | `ts5.oracle.call.tsv` | 39.6% | **70.0%** | 66.1% | 70.0% |
| ts5 | module | `ts.madge.module.tsv` | 30.8% | **50.6%** | 100.0% | 32.8% |
| rust | call | `rust.oracle.call.tsv` | 46.8% | **50.4%** | 30.9% | 28.3% |
| rust | type | `rust.oracle.type.typedecl.tsv` | 19.7% | **26.2%** | 98.0% | 74.1% |

Section 13.4 finding 1 is confirmed and priced: the driver, not the extractor,
capped both type-family numbers, and it capped the call family harder still.
Finding 2 (dst_path binding to the referring file) is untouched by this lane
and still belongs to `src/`.

The ts5 and rust module rows are the one place single process costs precision
outright: `resolved_import` follows barrels to the binding target, madge and
dependency-cruiser emit file-imports-file, and a whole-corpus run resolves more
barrel hops than a chunk can. Section 11's reading stands, `--deps`'s
`file_edge` is the row that joins those tools; `ts5.deps.module.tsv` was not
re-emitted this lane because `--deps` is a per-project code path with no
partitioning to fix.

### 14.7 `resolve_runs.py` now defaults to one process

`resolve_runs.py` takes `--chunk` to reach the split path and defaults to a
single invocation per corpus per mode. Under the default the runs log has
exactly one line, so `wc -l` on it is the receipt that no split happened; under
`--chunk` the script prints `FLOOR run` to stderr and a non-zero rc no longer
triggers a bisect, because bisecting is what produced the floors entry 96
records. `--suffix` tags the output filenames, defaulting to `chunked` or
`single`. Files this lane added beside the report: `<lang>.ctl.{call,type,module}.tsv`
(the same-binary control), `<lang>.{resolve,diet_scip}.single.runs.tsv`,
`<lang>.resolve.chunked.runs.tsv`, `single_process_runs.sh` (the timed one-process
driver) and `section14.py` (the table generator).
