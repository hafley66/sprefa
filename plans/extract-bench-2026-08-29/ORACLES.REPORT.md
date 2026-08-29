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
