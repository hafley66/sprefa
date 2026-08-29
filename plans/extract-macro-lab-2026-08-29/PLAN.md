# Rust macro-expansion lab (2026-08-29)

Last copy of the lab crate (`v6/sprefa-extract/labs/macro_expand/`): commit
`dae353d75`. The lab crate is deleted in the follow-up commit; this hash is
the only remaining record of its code.

## TOC

- [Corpus counts (re-measured)](#corpus-counts-re-measured)
- [Option 1: ra_ap_mbe + ra_ap_syntax + ra_ap_tt](#option-1-ra_ap_mbe--ra_ap_syntax--ra_ap_tt)
- [Option 2: ra_ap_hir_expand](#option-2-ra_ap_hir_expand)
- [Option 3: rustc -Zunpretty=expanded](#option-3-rustc--zunprettyexpanded)
- [Option 4: --family scip (rust-analyzer index)](#option-4---family-scip-rust-analyzer-index)
- [Option 5: syn](#option-5-syn)
- [Candidate comparison](#candidate-comparison)
- [Recommendation](#recommendation)

## Corpus counts (re-measured)

Bucket: `crates/*/src/**` of `~/projects/rust-analyzer` (873 files; the
17,184/941 figures in `plans/extract-crawl-2026-08-29/rust.REPORT.md` used a
different bucket; these are the numbers this lab measured).

| count | value | command |
|---|---|---|
| src files | 873 | `find crates -name '*.rs' -print \| grep '/src/' \| sort -u \| wc -l` |
| `macro_rules!` defs | 1,551 (410 distinct names) | `grep -rhoE 'macro_rules!\s*[a-zA-Z_][a-zA-Z0-9_]*' $(files) \| wc -l` |
| invocations (`name!` + delimiter) | 18,720 | `grep -rhoE '[a-zA-Z_]\w*!\s*[\(\[\{]'` + python counter |
| - local (name defined in-repo) | 13,560 (359 names) | python join vs the def-name set |
| - external/std/other | 5,160 (99 names) | remainder |
| `#[derive(...)]` | 2,137 | `grep -rhoE '#\[derive\('` |
| proc-macro-ish attrs | serde 281, salsa::tracked 158, proc_macros::identity 39 | `grep -rhoE '^#\[[a-z_]+'` |

Top invocations: expect! 2,994, T! 2,376, format! 1,299, vec! 904,
matches! 847, assert_eq! 676. RA version on PATH during the lab:
1.97.0-nightly, updated to 1.100.0-nightly (2026-08-28) when nightly was
reinstalled for option 3.

## Option 1: ra_ap_mbe + ra_ap_syntax + ra_ap_tt

What it is: in-process `macro_rules` expansion. Parse the file with
`ra_ap_syntax`, convert each `macro_rules!` body and each invocation's token
tree with `ra_ap_syntax_bridge::syntax_node_to_token_tree`, parse the def with
`mbe::DeclarativeMacro::parse_macro_rules`, expand with `mbe::expand`, convert
the expanded token tree back to text, splice it into the original file text at
the invocation's byte range, re-run the extract call walker on the spliced
file.

Needs: deps `ra_ap_{syntax,mbe,tt,parser,span,syntax-bridge,intern}` +
`salsa 0.28` (mbe's `expand` takes `&dyn salsa::Database`; a 3-line db struct
suffices). No toolchain, no subprocess.

Span mapping: every expanded token carries a `Span { range, anchor, ctx }`.
Measured on the fixtures: tokens copied from macro arguments keep call-site
ranges (mapped), tokens from the macro definition body keep def-site ranges
(partial). Spliced expanded text lands at the invocation's byte range, so
spans are recomputable, but they are offsets into the SPLICED file, not the
original: partial, by construction.

Fixture table (fixpoint = repeated passes until no expansion remains):

| fixture | sites orig -> expanded | defs orig -> expanded | spans mapped | wall ms |
|---|---|---|---|---|
| f1 local macro, call in body | 0 -> 2 | 2 -> 2 | partial (2 call-site, 1 def-site tokens) | 0 |
| f2 macro defined in other file | 1 -> 1 | 1 -> 1 | n/a (def invisible per-file) | 0 |
| f3 nested invocations | 0 -> 1 | 2 -> 2 | partial (1 call-site, 2 def-site) | 0 |
| f4 format!/vec!/assert! | 1 -> 1 | 2 -> 2 | none (builtins not expandable here) | 0 |
| f5 #[derive(Debug, Clone)] | 1 -> 1 | 1 -> 1 | none (proc-macro) | 0 |
| f6 #[derive(Serialize)] attr | 2 -> 2 | 2 -> 2 | none (proc-macro) | 0 |
| f7 macro mints a fn | 1 -> 2 | 2 -> 3 | partial (1 call-site, 3 def-site) | 0 |
| f8 include! | 1 -> 1 | 1 -> 1 | none (builtin) | 0 |

Corpus table (941 -> 873 src files, `timeout 10` per file, xargs -P 8,
log `labs/macro_expand/opt1.battery.log`):

| metric | value |
|---|---|
| files run | 873, 0 failures, 0 timeouts |
| invocations found (name + token tree) | 13,868 |
| in-process expand wall, total | 758 ms |
| max per-file expand wall | 56 ms (crates/intern/src/symbol/symbols.rs) |
| call sites orig | 133,102 |
| call sites after expansion | 137,945 |
| sites gained | 4,843 (in 33 files) |
| peak RSS (largest exercised file) | 7.96 MB |

The 33 gain files are exactly the kink-2 shape: `config.rs` +226,
`lang_item.rs` +992, `inert_attr_macro.rs` +416, `symbols.rs` +5,391.

## Option 2: ra_ap_hir_expand

What it is: rust-analyzer's full expander (builtins, eager expansion,
proc-macro server). Measured as a link-cost probe (`src/opt2.rs`), not an
integration.

Needs: `ra_ap_hir_expand` + `ra_ap_base_db`; a real integration must
implement `base_db::SourceDatabase` (file loader, crate graph, source roots,
file text), and for proc macros run a `proc-macro-srv` child process per
toolchain.

| metric | value |
|---|---|
| cold `cargo build --release` with the dep | 30 s (dep tree: hir, la-arena, vfs, salsa, ...) |
| binary size | 2,062,832 -> 4,380,656 bytes (+2.32 MB) |
| probe binary startup | 0.17 s real, 1.77 MB RSS |
| pointing at a workspace | SourceDatabase impl + CrateGraph + proc-macro server; not built in this lab |

No fixture/corpus expansion rows: the probe only proves the link.

## Option 3: rustc -Zunpretty=expanded

What it is: nightly rustc pretty-prints the crate after macro expansion and
desugaring; one `cargo rustc -p X -- -Zunpretty=expanded` run per crate.

Needs: nightly toolchain (RA declares `rust-version = 1.98`; local nightly
1.97 fails, 1.100.0-nightly works), a cargo build of every dependency.

Corpus table (44 workspace crates, warm cargo cache):

| metric | value |
|---|---|
| crates expanded | 42 / 44 |
| failures | 2 (`proc-macro-srv-cli`, `rust-analyzer`: multi-target packages reject `cargo rustc` extra args without `--lib`/`--bin`) |
| wall total | 46.9 s warm (ide alone 36.3 s) |
| expanded bytes vs src bytes | 25.3 MB vs 35.5 MB (pretty-printer normalizes whitespace; byte ratio misleading) |
| span mapping | none emitted: the pretty-printer drops all span info |

Span-mapping ambiguity (diff-based, measured on the fixture crate): the
expanded text is a plain re-print. `twice!(helper())` re-prints as two bare
`helper()` lines with no source marker; the only mapping is re-parse +
name matching, which is ambiguous exactly when a macro mints repeated calls
(the case that matters for kink 2). Builtin expansions re-print as unstable
intrinsics (`vec!` -> `box_assume_init_into_vec_unsafe`), which the phase-1
walker cannot relate to source.

Fixture: the 8 fixtures as one crate (`fixtures/opt3crate`), one expansion
run, 3 s wall: call sites 26 vs 6 summed from the originals (the whole point:
the calls become visible), defs 18 vs 12; spans unmapped.

## Option 4: --family scip (rust-analyzer index)

What it is: the exact mode already in extract. `rust-analyzer scip` builds a
SCIP index; extract decodes it to `scip_*` relations.

Run: `extract --family scip --scip-timeout 1500 ~/projects/rust-analyzer`
from the lab worktree, log `labs/macro_expand/opt4.rc`:

| metric | value |
|---|---|
| exit | 0 (rc=0), fresh index built (`reused:false`, 922 documents) |
| tool | rust-analyzer 1.100.0-nightly (the 1.97 on PATH initially was pre-update) |
| `No generics for EnumVariantId` panic | did NOT reproduce with 1.100.0-nightly |
| `scip_fn_edge` rows | 173,502 |
| symbol-reference occurrences (raw index, decoded) | 647,232 |
| references inside macro invocation spans | 44,455 |
| of those, symbol is an fn_edge callee | 17,568 |

The last row is kink 2 measured from the exact side: 17,568 call occurrences
inside macro spans that SCIP resolves and the phase-1 walk never sees.
Span mapping: exact (SCIP occurrences carry line/col ranges; converted to
byte offsets against the source and joined with the lab's macro-span dump
`opt1.macro_spans.tsv`, 13,868 invocation spans).

Fixture scale: on `fixtures/opt3crate` the index builds (1 document) but
emits 0 `scip_fn_edge` rows; the fixture table is n/a for scip, the corpus
numbers above carry it.

## Option 5: syn

One row: syn parses and does not expand. The kink survives because syn gives
the whole macro invocation a single opaque `Item::Macro`/`Expr::Macro` node
and the call walker's expression walk (`syn::visit::visit_file` at
`src/lang/rust.rs:1624`) never enters it. No work done; no numbers.

## Candidate comparison

| | 1 mbe in-process | 2 hir_expand | 3 rustc -Zunpretty | 4 scip | 5 syn |
|---|---|---|---|---|---|
| expands macro_rules | yes | yes | yes | yes (facts only) | no |
| expands builtins (format!, vec!) | no | yes | yes | yes | no |
| expands proc/derive macros | no | yes (needs server) | yes | yes | no |
| new deps | 5 ra_ap crates + salsa | hir_expand + base_db (+2.32 MB bin) | none (toolchain) | none (existing) | none |
| subprocess/toolchain | none | proc-macro server | nightly per crate | rust-analyzer per root | none |
| corpus wall | 758 ms / 873 files | n/a (link probe) | 46.9 s / 42 crates | index build, budget 1500 s | 0 |
| sites gained (corpus) | +4,843 | n/a | n/a (no span mapping) | 17,568 callee refs already exact | 0 |
| span mapping | partial (call-site tokens; spliced offsets) | full (by construction) | none (printer drops spans) | exact | n/a |
| per-file purity preserved | yes | yes | no (whole-crate) | no (whole-root) | yes |

## Recommendation

- Tier 1: **Option 1**. Cheapest path that closes the kink-2 shape for
  `macro_rules`: +4,843 call sites on RA's src bucket for 758 ms of
  in-process work, no toolchain, no subprocess, per-file purity preserved.
  It plugs into `project_call` (`src/lang/rust.rs:1575`): expand each
  invocation, splice into the file text, re-run the same
  `CallCollector` walk (`src/lang/rust.rs:1619-1624`) on the spliced text,
  and mark minted sites with the invocation's span.
- Tier 2: **Option 4**. The exact-mode answer is already exact
  (17,568 macro-span callee refs, panic gone on rust-analyzer
  1.100.0-nightly). The seam is the existing `--family scip` relation set;
  the work is a join of `scip_fn_edge`/occurrences against macro invocation
  spans, not new expansion.
- Option 3 is rejected on span mapping (the printer drops spans; diff-based
  mapping is ambiguous precisely on repeated mints). Option 2 is rejected on
  integration cost (+2.32 MB binary, full SourceDatabase + proc-macro
  server) for facts tier 1 and tier 2 already cover.
