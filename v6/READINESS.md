# v5 golden use-cases: readiness

The nine stopping-point programs (CLAUDE.md "v6 STOPPING POINT"), each graded
by a run in this worktree rather than by a remembered receipt. Every wall time
below is from that run, on one machine, warm caches, nothing else running.

Base: `00305ff0`, graded at `f6259a25`.

| # | program | status | receipt | wall |
|---|---|---|---|---|
| 1 | ghcacher | SMALL-GAP-FIXED | `just ghcacher-golden` | 0.6s |
| 2 | diags for LSP | READY | `just lsp-diags` | 12.3s |
| 3 | git pre-commit --changed | SMALL-GAP-FIXED | `just precommit-changed` | 18.9s |
| 4 | sprefa-extract run | GAP (v5 surface, L) | `just extraction-live` + `just rtkq-golden` + `just flagship` | 7.7 / 0.5 / 5.7s |
| 5 | auto-synced repo list | READY | `just multirepo-golden` | 5.6s |
| 6 | v5 bench parity | GAP (84x, L) | `just crawl-bench` (repo-root justfile) | 41.8s |
| 7 | rtkq examples | READY | `just rtkq-golden` | 0.5s |
| 8 | file watcher scaling | SMALL-GAP-FIXED | `just watch-scale` + `just enumerate` | 1.0 / 6.7s |
| 9 | standardized tick-log format | GAP (no spec doc, S) | two implementations agree; nothing states the format | n/a |

Four READY, three SMALL-GAP-FIXED in this lane, two standing GAPs plus one
inside program 4. Every row's receipt exits 0 as of this commit.

## The rows

1. **ghcacher.** `GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1`. The gate byte-diffs
   three ways (expected vs oracle, expected vs emitted, oracle vs emitted) over a
   five-tick hermetic schedule; tick 5 is the point, where a late response for a
   departed witness replaces its raw host row and produces no public delta. It was
   RED at this base and is fixed below.

2. **diags for LSP.** `LSP DIAGS HOLDS`, both engine-side appear/retract and the real
   `dl --lsp --diag-db` over real stdio JSON-RPC. Line numbers are honestly zero
   until byte spans can enter programs. The run leaks a hung `dl --lsp` (v5 answers
   `shutdown` then does not exit on `exit`+EOF; the script SIGKILLs it and says so).

3. **git pre-commit --changed.** `GIT-FACT DIAGS RAIL HOLDS`, four real commits driven
   through the served engine, every assertion a sorted row-set equality, with a clean
   new file as the control that a fires-on-everything rail would fail. The program
   existed and its receipt was RED; both are fixed below.

4. **sprefa-extract run.** Extraction is live end to end (real edit, atomic save,
   content-addressed zero-tick, delete retraction, restart, `kill -9` mid-extraction
   exactly-once) and two extraction programs grade against pinned expectations. What
   is not ready is the v5 SURFACE the phrase "scan/scanwork, repo/rev, lazy finding,
   lazy heads" names: see the priced gap.

5. **auto-synced repo list.** `MULTIREPO CRAWL GRADED: 4/4 rels byte-identical, 0
   classified, 1 named gap, 0 unclassified` over a pinned four-repo corpus, with v5
   running `examples/version-skew.dl` byte-unmodified. The one gap costs two witness
   columns and no rows: `dep_ver` needs min/max over text, which v6 refuses
   (`aggregate_operand_not_number(min, _, text)`), so skew MEMBERSHIP is a self-join
   instead and grades in full.

6. **v5 bench parity.** The yardstick runs, against the real 389-repo grafana corpus,
   and the distance is the finding: see the priced gap.

7. **rtkq examples.** Five ticks, `responseRows [9,6]`, 2 processes, 2 demands, exact
   half-open byte spans against committed expected rows. Four ast-grep patterns batch
   through ONE extractor process per file digest.

8. **file watcher scaling.** Correctness at scale is now gated (below). Duplicate
   notifications, edits, identical re-saves and deletes at 100 and 1000 files:
   `correct: true`, zero duplicate/stale/missing rows, final relations pinned by
   sha256. Ticks stay FLAT at 3 and write amplification is identical (1.185) across
   the 10x; wall goes 37.6ms to 248.1ms (6.6x) and statements 191 to 1316 (6.9x), so
   the curve is sublinear in files over this range. `just enumerate` separately proves
   the file-set feed against this repo: 273 rows equal to `git ls-files` exactly, and
   973 `node_modules` .ts files on disk with 0 in the answer.

9. **standardized tick-log format.** The format is real and two independent
   implementations agree on it byte for byte across the whole corpus. It is not
   WRITTEN DOWN anywhere a third implementation could read: see the priced gap.

## Fixed in this lane

**1. The ghcacher golden was RED at this base** (`63d9fa17`). `17fe0d4b` made
`dl6_oracle.pl`'s JSON arrival mapping type-directed and moved `read_schedule/2` to
`/4`; the ghcacher runner, promoted out of `labs/` earlier at `1acd3478`, kept the old
call and died with `Unknown procedure: read_schedule/2`. A `green-all` member had been
failing since. Fixed by calling `/4` with the parsed program and its bindings. All
three of the gate's byte diffs pass unchanged against the committed expected files, so
the type-directed mapping reproduces the same log.

**2. Program 3 existed, was unwired, and its receipt was RED** (`f6259a25`).
`v5-git-diags.dl6` and its four-commit receipt landed at `321a3d4c` and no battery ran
them. Running it, every rail assertion passed and one failed: the receipt's own
whitespace-decode DEFECT WITNESS, which now proves the opposite of what it was written
to prove. `7c338827` fixed `parseWhitespace`'s line-count collapse by giving the grid
reading precedence, so the two host encodings agree at two files again, and the failure
message said exactly that and asked for the block's deletion.

It was inverted into a regression guard rather than deleted. The reason is the bug's
shape: the collapse was a function of ROW COUNT (wrong at exactly 2, right at 1 and 3),
so it was invisible to any receipt whose corpus was the wrong size, which is how it
shipped in the first place. `tests/hostDecode.test.ts` pins cardinalities 0/1/2/3 at the
decode seam; this pair is the only check of the same cardinalities END TO END, through a
real git repository, a real subprocess host, and two declarations differing only in
output encoding. It costs two assertions and no new rel. The program's prose still
described the defect as live and was rewritten.

**3. Three recipes wired.**

| recipe | runs | battery |
|---|---|---|
| `precommit-changed` | `v5-git-diags.sh` | green-all |
| `watch-scale` | `5_file-watch-scale.ts` at 100 and 1000 files | green-all |
| `v5-parity` | `v5-parity.sh` | none, deliberately |

`watch-scale` earns its battery seat: it exits 1 on any duplicate, stale or missing row
and pins both final relations by sha256, so it gates CORRECTNESS at scale even though
its cost columns are only reported. `v5-parity` stays out of every battery because it
regenerates a tracked table under `plans/`.

## Priced gaps

**Program 4, the v5 surface (L).** `plans/2026-07-30-v5-parity-table.tsv`, regenerated
by `just v5-parity`, is the honest inventory: 156 things v5's own 129-file example corpus
uses, of which **132 absent, 20 covered, 4 partial**. `scan` is covered (105 files, the
single most-used construct). `scanwork` and `finding` do not appear in the table at all.
Of the 112 built-in relations only 6 are covered, and all six are accidental name
collisions (`file`, `call_site`, `df_node`, `df_arg`, `df_edge`, `df_param`) because v6
has ZERO built-in relations by the `spine_residency` ruling, so a covered rel is covered
BY A PROGRAM. The backlog clusters, largest first:

| cluster | examples | weight |
|---|---|---|
| codegen and drawable sinks | `gen` 24 files, `graph_node`, `graph_edge`, `hover_note` | highest-usage construct with no v6 spelling |
| comment-marker regions | `comment` 20 files, `comment_node` 6 | no v6 spelling |
| structural matching | `match_ast` 14 files, `sg`, `ast_yaml` | highest-usage REFUSED construct |
| graph algorithms | `closure` 17 files, `scc`, `node2vec` | the standing graph-algo queue item |
| scalar functions | `replace_re` 16, `split` 14, `replace` 9 | 14 of 16 absent |
| repo/rev relations | `repo` 12, `changed` 7, `head` 5, `rev` 2, `git_ref` 1 | program 5 covers the crawl shape, not these names |

Each cluster is its own arc. Nothing here is a small gap and none of it was attempted in
this lane.

**Program 6, the parity distance (L).** `just crawl-bench` against `~/orgs/grafana`, my
own run:

| engine | files | repos | wall | files/s |
|---|---:|---:|---:|---:|
| v5, full 389-repo org | 42,739 | 389 | 13.09s | 3,265.01 |
| v6, served + extraction, first 8 usable repos | 779 | 8 | 20.15s | 38.66 |

**84x** on measured throughput, and 187x against the 7,244 files/s historical v5 figure
the memory doc records for the same corpus. Two named causes, both structural rather
than tuning: `commit_ms` at roughly 10.8ms/file is the dominant remaining per-file cost
and is unowned; and **v6 has no org fan-out spelling at all**, so `crawl-bench.sh`
supplies it as a shell loop over repos and the v6 leg is capped at 8 of 250 usable repos
because a linear projection of the full crawl is 1,050s. The missing fan-out is a
language gap, not a harness convenience: it is what stops the v6 leg from being written
as one program the way the v5 leg is (`src(path, rev) <- scan(...), repo(...)`).

**Program 9, the format is undocumented (S).** The envelope is
`{"tick":N,"deltas":{"rel":{"add":[[...]],"del":[[...]]}}}`, rels ascending, rows in
declared column order, add and del each sorted by their own JSON text, json columns
canonicalized with sorted keys and no whitespace, LF endings. Two independent
implementations produce it and agree byte for byte:
`v6/prolog/conformance/ticklog.pl` and `v6/tsv2/runtime/ticklog.ts`. Every other
producer reuses one of those two rather than reimplementing.

What does not exist is a document stating the format. It lives only in the two
source-file comment headers. `v6/prolog/compile/TICK-MODEL.md` specifies the SEMANTICS
(the semirings, lifecycle as sign decomposition, the refusal theorems) and never gives
the envelope grammar. A third runner in rust or python would have to read prolog and
TypeScript to learn the contract that exists precisely so it would not have to. Writing
the spec is a transcription of two comment headers plus the `json_ticklog_encoding`
ruling, and the two agreeing implementations are the conformance corpus for it.

**Program 8 residual, cost is ungated (S).** `watch-scale` gates correctness at scale
and REPORTS wall and RSS. There is no cost floor for the watch path the way
`scripts/7_scale-floor.sh` gates the emitter, so a watcher change that keeps every row
correct while doubling the work would pass. `scale-floor.sh` is the shape to copy; its
own history file currently holds one line.

## Two notes for whoever runs this next

`just lsp-diags` can leave a hung `dl --lsp` behind, by v5 defect. `pkill -f "dl --lsp"`
after it.

In a git worktree, symlinking `v6/tsv2/node_modules` at the main checkout is not enough
and is quietly wrong: the `sprefa-store-engine` entry inside it is a RELATIVE link
(`../../sprefa-store/js`), so it resolves against the MAIN checkout and every run reads
main's store source instead of the worktree's. `pnpm install` in each of `v6/tsv2`,
`v6/dl` and `v6/sprefa-store/js` is the fix and takes under a second each against the
warm store. The symlink also fails `just enumerate` outright, because its
node_modules-never-walked assertion counts files with `find`, which does not descend a
symlinked argument.
