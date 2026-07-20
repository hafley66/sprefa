# v5 assimilation: the standing work plan (2026-07-20)

Branch `v11`, cut from `next` at `8d7b6092`. This is the live plan for the v5
engine. v6 stays a design tree whose only product is library extraction
candidates; nothing here waits on it.

Sources folded in: 211 items harvested from `v6/plans/*` + `plans/2026-07-*`
(77 files), 19 open items from `chat_log/20260718-20`, `docs/failure-modes.md`,
and the four-round graph-library research in
`~/projects/claude-research/commands/graph-libs/`.

## What the research settled, verified against the live db 2026-07-20

Root `fbabddda40d22347`, 796MB, against a 4.2MB `src/` corpus.

| bucket | MB | share |
|---|---|---|
| rel table data | 250 | 31% |
| PK autoindexes | 244 | 31% |
| demand `idx_` | 213 | 27% |
| everything else (`_strings` 62, internal) | 89 | 11% |

Indexes remain 57% of the file after the auto-index demand arc (`dc9b67b1`)
took `idx_` from 771 to 264.

Two claims from the plan docs that are now STALE and were removed from the
work list: the `_strings.norm` 66.7MB index is already gone (`_strings` carries
no index at all today), and the constant-column `repo` index on
`rel_df_node_repo_rev` was already dropped.

## The lazy-rel tier is withdrawn

`plans/2026-07-19-lazy-rel-tier.md` proposed a VIEW tier holding ~286MB at zero
bytes. Withdrawn as a default, kept as a narrow special case.

A VIEW re-runs its rule on every read. The stored rows are the cache, and the
work of producing them is the reason to keep them. Measured shape of the
tradeoff:

```
        eager                             lazy
   rel_foo TABLE                     rel_foo VIEW
     rows + PK autoidx + idx_          SQL text, 0 bytes
     evaluated 1x per tick             expanded R times per tick
     downstream joins seek an index    downstream joins have no index
```

Laziness costs no incrementality: `affected_derived`
(`src/engine/strata.rs:337-366`) is pure syntactic reachability over rule
bodies, seeded from `changed_source_rels`, and never reads storage or digests.
The per-rel digest compare in `rebuild_derived` (`src/engine/derive.rs:444-498`)
skips the WRITE only, and its result never feeds back into scoping. So a rel
becoming a view drops cleanly out of phase 3 without touching phases 1 or 2.

What it does cost is recompute multiplied by reader count R, plus the loss of
any index on the result. A rel qualifies only when R is small and the rule is
near-free. `named_call_site` (35MB with its autoindex, 2 declarations, exactly
one consumer each) is the one candidate that clears that bar today.

Open and unresearched: how Souffle, DDlog, and LogicBlox decide what to
materialize, and whether magic-sets / demand transformation applies here. The
existing survey in the lazy-rel-tier plan shopped LIBRARIES (dbsp,
differential-dataflow, Turso, DuckDB, sqlite-zstd) and never read the datalog
literature on this question. That gap is named rather than filled.

## Ranked work

Rank is by receipt strength first, then bytes or milliseconds per unit of
effort. Every row carries its source id from the harvest.

### Tier 1: measured, mechanism already shipped, needs a vouch or a DDL

| # | change | receipt | source |
|---|---|---|---|
| 1 | `pk_never_null` vouch on 5 junctions so they take `WITHOUT ROWID` | 55.3MB measured on real rows, zero NULLs found in any PK column of any candidate. All 5 confirmed still plain rowid tables today: `named_call_site` 35MB, `flow_edge` 19MB, `df_edge_src_kind` 19MB, `df_node_in_fn` 16MB, `port_reach` 15MB | H49 |
| 2 | reverse-column indexes on `rel_map_edge` (139,709), `rel_bom_edge` (25,314), `rel_port_edge` (22,154) | verified live: PK autoindex only. Reverse traversal without one measured 162.4ms against 0.04ms, `AUTOMATIC COVERING INDEX` in the plan, reproduced in all four rounds | H186 |
| 3 | drop `idx_df_edge_from` | duplicates the PK prefix; the forward plan uses the PK | H187 |
| 4 | drop orphaned `rel_port_of_reach` + its `_txt` view, then VACUUM | 7.6MB table + 8.6MB autoindex; the deleted rule left the table behind | ledger |

The `WITHOUT ROWID` path already exists and is tested
(`tests/it/storage_diet_without_rowid.rs`, 5 cases). Item 1 is a flag on 5
declarations, guarded by the classifier at `src/engine/declare.rs:134-139`.
The known regression shape is recorded: a naive rollout broke
`named_args_in_a_rule_head_resolve`, because `WITHOUT ROWID` requires every PK
column NOT NULL and a named-arg partial head can put NULL anywhere (H56). That
test is the fail-pre-fix guard for item 1.

### Tier 2: measured defect, fix not yet designed

| # | defect | receipt | source |
|---|---|---|---|
| 5 | depth cap 64 silently truncates walks on the largest relation | `rel_flow_edge` measured p99 eccentricity 79, max >= 112. Correctness, not performance | H191 |
| 6 | `refresh_dataflow_rels` replaces 15 relations on every dataflow edit, loading the whole extraction file set | 379-file run spent 5,706ms in dataflow-rels; warm counterparts still spent 759-830ms | H74, H75 |
| 7 | `refresh_call_rels` reconstructs corpus-global definition indexes on every edit | public call rels cannot be deleted per changed file: `call_edge_rev` drops the producing file, `call_name`/`call_kind` drop file and rev | H79, H80 |
| 8 | `.dl` discovery merge double-loads a file's rules | `port_reach`'s fixpoint ran 110 rounds x2 = 220 statements; the recognizer dedups rules by Debug string, the SQL path does not | H66 |
| 9 | semi-naive divergence wedges before the 100k iteration cap trips | 15-minute wedge observed at ~43k statements; a growing delta makes each iteration slower long before the cap | H147 |
| 10 | module-graph extraction is nondeterministic | same binary, same 3-repo corpus, back-to-back cold runs: 213 rows both, text sum 5999 vs 5964, mutual pairs 38 vs 108 | H146 |

Items 6 and 7 are the same shape and share a root cause with the general
"family refresh is wholesale" problem. They are the largest measured wall-time
items in the harvest and neither has a design.

Item 10 is the most alarming: nondeterministic extraction was already the
deepest of the six stacked bugs behind the beachball incident (H200), and this
receipt says a variant survives.

### Tier 3: the `_strings` dictionary

`_strings` is 62MB, the single largest object in the file.

| finding | receipt | source |
|---|---|---|
| the dictionary is 92% re-encoded coordinates, not vocabulary | sampled 300k of 939,842 rows: path 35.3%, rev-salted 33.5%, qualified scip symbol 13.2%, `file:line:col` 10.2%. Genuine short literals are 7.7% of rows and 2.6% of bytes | H26 |
| `mint_sym` costs 26.6% of `_strings` bytes, `salt_rev` costs 36.4% | measured per-helper attribution | H36 |
| ids are hash-valued i64 across the full i64 range for rows that would be dense 0..N | dense-id ratio ~1.96e13 against a 1.05x target | H25, H34 |
| coordinate composites buy zero sharing | non-rev 502,192 rows / 44.7MB, rev-salted 555,034 rows / 45.1MB, together 78% of rows and 86% of text | H53 |
| step-3 re-intern skip never fired before `8d7b6092` | 1,207,064 rows offered, 146 accepted | H31 |

This is the biggest single lever in the file and the least low-hanging. It
needs a design, not a flag. Item: stop interning coordinates at all, which
means `repo`/`rev` reach the derived layer as columns instead of being smuggled
into id strings by `salt_rev`. Only 86 of 505 tables carry `repo` or `rev`
today; 418 carry neither (H21).

### Tier 4: rails with no enforcement

Six failure-mode classes carry a law and no rail.

| class | law | missing |
|---|---|---|
| 7 | a quiet tick writes zero rows | the CI soak is not wired; nothing fails a build on a quiet-tick write |
| 8 | never hold a lock across a call that can block or take another lock | no hold-set instrumentation, no static rail |
| 9 | every channel is bounded | 2 unbounded sites unwaivered, no rail banning new ones |
| 16 | a kill is a stop order, not a restart trigger | no spawn backoff, no mid-cold-extract digest persistence, no resume test |
| 17 | a db an order of magnitude over its corpus is a defect | no ratio ceiling, no boot verdict line |
| 23 | a one-shot either evaluates its program or refuses loudly | no rail; owned by the unstarted no-daemon-split erasure |

Class 23 is a prerequisite for testing anything else from the CLI: a one-shot
`dl prog.dl` against a daemon-served root silently returns the watched program
set's rows, because `run_file_via_daemon` sends only `{"root"}`. Any test that
asserts "run this program, get these rows" can pass against the wrong program.

Class 7 and class 17 are the two that a storage arc would close.

### Tier 5: cheap correctness and surface fixes

Each is a small, self-contained defect with a receipt.

| # | defect | source |
|---|---|---|
| 11 | `+` on text lowers to SQLite `+` unconditionally, so `"https://" + host` silently returns `0` by numeric coercion, no error | H125 |
| 12 | `body_sql` runs the Neg pass before the Cmp pass, so a bind var referenced in a negation atom becomes an unconstrained local of the `NOT EXISTS` subquery | H126 |
| 13 | reserved-name collisions die at tick time instead of at `--parse-only` | H153 |
| 14 | a scan+match rule whose body also joins a derived rel runs extraction and silently ignores the join, with no bail | H122 |
| 15 | `(?i)` folds character classes too, so `[A-Z0-9]` becomes `[A-Za-z0-9]` and defeats an uppercase-boundary check | H113 |
| 16 | `.mts`/`.cts` are recognized by the module resolver and skipped by every oxc extraction path, a 4-site gap | H211 |
| 17 | HTML/YAML/TOML/CSS/JSON grammars are compiled into the binary and used by `sg`/datapath, and absent from `AST_LANG_TABLE`, so `comment_node`/`ast`/CST get zero coverage | H210 |
| 18 | `TsTypes` matches only `.ts`/`.tsx` although oxc parses plain `.js`/`.jsx`, so JS gets zero type/call/df/doc facts | H156 |
| 19 | positional rels disagree on line base (`comment_node` 1-based, `scip_occurrence` 0-based) and this is documented nowhere | H120 |
| 20 | `docs/reference/relations.md` states `scip_def` at 2 columns and `scip_ref` at 3; the engine has 3 and 4 | H106 |

Items 16, 17, and 18 are each near-zero cost and each unlock a language or file
family that currently produces no facts at all.

## What v6 keeps

Library extraction candidates only, each already measured:

| candidate | lines | status |
|---|---|---|
| `sprefa-graph` (`walk.rs` 251 + `scc.rs` 180 + `mod.rs` 14) | 445 | correct by exact set equality against an independent Tarjan on 6/6 real relations, 9 adversarial graphs, 0/400 differential fuzz. Every measured library is slower, wrong, or non-viable |
| `sprefa-extract` (typegraph 9,106 + modgraph 2,828) | 11,934 | already storage-free: extract takes `(path, content)` and returns plain facts, no DB dependency |

Note the unresolved contradiction: `v6/plans/2026-07-19-v6-table-design.md:184-189`
wants the per-language extractors pulled into a crate by family, and the already
executed `plans/2026-07-18-decomposition-normalization.md:129-131` split them
per-language inside the crate and explicitly ruled the family axis out. Both
claim to be measured. Neither cites the other.

## Sequence

Tier 1 is four DDL statements and one flag, and closes 78MB with tests that
already exist. It goes first and can land inside 0.11.0.

Class 23 (tier 4) goes next, alone, because it is a large it-suite touch and
because every later CLI-level test depends on it being honest.

Tier 2 items 6 and 7 are the wall-time work and need a design pass before any
code. Tier 3 needs a design pass before that.

Tier 5 is dispatchable in parallel at any time, disjoint files, one agent per
item or per small group.
