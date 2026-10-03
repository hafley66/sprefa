# Findings: the 2232 CREATEs of the c15-shaped install

sqlite_ivm `1483ccc`, corpus `crates/ivm-sqlite/tests/corpus/8_c15_program.json` (4011 IR nodes,
164 strata, 93 outputs, bundled into one plan by `4_engine.rs:194-220`).

## Method

Real census, one run. Scratch worktree at `1483ccc` (removed afterwards), `hafley-rs` symlinked,
`RCARGO_OFF=1 cargo test -j 4 -p ivm-sqlite --test 6_install_growth c15_shaped -- --nocapture
--test-threads=1`. Temporary instrumentation, scratch only:

- `count_create` (`tests/6_install_growth.rs:14`) appended every traced CREATE statement to a file.
- `3_nodes.rs`: `#[track_caller]` on `integrate`/`integrate_at`/`image` recorded each integrate call
  site per SQL node; `observe` recorded the creating IR node per SQL node; `compile` dumped per SQL
  node `inline/integrated/viewed/join/union/source/rec/owner/indexes`, every body and fill, the SCC
  `(v, b, r)` triples and the source map.

Result: `creates=2232 objects=2230` (pin holds). Python joined the CREATE names (`frontier_*_nK_*`)
to SQL nodes, SQL nodes to IR ops, and readers from `ivm_nK_d`/`ivm_nK_i` references in bodies/fills.
creates - objects = 2: the catalog DDL runs twice as `IF NOT EXISTS` (`catalog.rs:617-619` in
`catalog_row`, `catalog.rs:785-789` via `push_catalog_ddl` at `catalog.rs:321`).

## Census by object kind

| kind | count | emitted at |
|---|---|---|
| `_d` delta tables | 992 | `3_nodes.rs:1604-1607` (every node not `inline`) |
| `_i` image tables | 744 | `3_nodes.rs:1658-1663` (every node with `integrated`) |
| `_x` indexes on `_i` | 215 | `3_nodes.rs:1664-1674` (keys not served by the PK or a longer index) |
| host source tables | 188 | `4_engine.rs:240-257` (162 `__ir_seed_n*`, 15 `__ir_prewarm_*`, 7 `__ir_intern_request_seed_*`, unit, term_map, plain_list_seeds, bad_lists) |
| SCC scratch `_hold/_nx/_del/_acc` | 56 | `3_nodes.rs:1633-1642`, `1693-1700` (14 SCCs x 4) |
| Reduce `_g` | 7 | `3_nodes.rs:1042-1047` |
| dictionaries + constructor tables | 6 + 15 | `2_terms.rs:55-61`, `2_terms.rs:82-84` |
| catalog (2 objects, 4 statements) | 4 | `catalog.rs:617-619`, `catalog.rs:785-789` |
| stage, stage index, output delta, 2 output views | 5 | `catalog.rs:801-845` |

## Census by cause (disjoint, sums to 2232)

| cause (SQL node role) | total | split | code path |
|---|---|---|---|
| Mint | 410 | settle `_d` 164, loop `_d` 69, `_i` 72+21 loop, `_x` 72+12 loop | `3_nodes.rs:597-634`; fill is 3 statements (`:632`); no `view()` call, so every join-read Mint image is stored (`image` falls to `integrate_at`, `:421-427`) |
| body joins (rule-body binary joins) | 320 | settle `_d` 152, `_i` 1; loop `_d` 85, `_i` 47, `_x` 35 | `3_nodes.rs:831-914`; settle `_d` stays physical when read by >1 statement (`:513-518`); loop-owned nodes never inline (`:513`, owner check) and never view (`:435`, `:889`) |
| SCC copies of outer inputs | 289 | `_d` 128, `_i` 128, `_x` 33 | `3_nodes.rs:450-467`, `:469-476`; stash/restore `:1633-1649` |
| source `_i`/`_d` | 206 | `_i` 188, `_x` 10, `_d` 8 | `3_nodes.rs:325-330` integrates every source; `_d` when read by > `SOURCE_READS` (`:97`, `:523-525`) |
| host source tables | 188 | see above | `4_engine.rs:240-257` |
| threshold-input images (per relation) | 159 | Union `_i` 126, `_x` 20, Threshold-attributed `_i` 13 | `3_nodes.rs:1148-1150` |
| antijoin-internal joins | 123 | `_d` 123 | `3_nodes.rs:967-989` (`l + -(l ⋉ threshold(keys))`) |
| LetRec result `r` | 79 | `_d` 37, `_i` 31, `_x` 11 | `3_nodes.rs:1318-1321`, finish `:1722` |
| LetRec variable `v` | 77 | `_d` 37, `_i` 37, `_x` 3 | `3_nodes.rs:1294-1295` |
| LetRec body `b` | 74 | `_d` 37, `_i` 37 | `3_nodes.rs:1303-1307` |
| ground-constant joins | 73 | settle `_d` 50, loop `_d` 21, `_i` 1, `_x` 1 | dl8 `mapped_ground` `_9a_ir_lower.rs:217-233`, joined at `:1372-1384` |
| SCC scratch | 56 | 14 each | `3_nodes.rs:1636-1642`, `:1693-1700` |
| recursive antijoin + other loop images | 81 | Antijoin loop `_d` 33 + `_i` 6; loop `_i`/`_x` attributed to Join 22, Mfp 20 | `3_nodes.rs:919-965`; rec views disabled `:435` |
| stored unions (inline-join trade) | 37 | `_d` 37 | `3_nodes.rs:478-490`, `:520-522` |
| fixed per database | 28 | dict 6, ctor 15, catalog 4, stage/index/delta 3 | above |
| Reduce | 21 | `_d` 7, `_g` 7, `_i` 7 | `3_nodes.rs:994-1145` |
| constructor Get | 8 | `_d` 4, `_i` 4 | `3_nodes.rs:577-588` |
| bundle output | 3 | `_i` 1, views 2 | `3_nodes.rs:1328-1331`, `catalog.rs:834-845` |

### Detail on the largest rows

- Join `_d` that have one reader yet stay physical (196): the physical statement at the end of the
  inline chain is a Mint for 116 of them (110 antijoin-internal, 6 body). Mint reads its input delta
  in two statements (`insert_ctor` `3_nodes.rs:625-627` and the body `:614-617`), so the delta's
  read weight is 2 and `place` rejects it (`:509-514` requires one reading statement whose fill is a
  single `FILL_ANCHOR` statement). The other 80: 58 reach two physical readers, 6 the bundle, rest
  Union/Join.
- Body-join `_d` read by >1 statement: 63 are read by one Join and one Union, 25 by two Joins
  (shared prefix via join dedup, `3_nodes.rs:851-853`).
- Mint `_i` (93): 32 are probed only on passthrough columns, 61 on the minted id column.
- Mint settle `_d` (164): 90 have one reader and no image; 67 have one reader plus an image
  (integration reads the `_d`); 7 have two readers.
- SCC copy outer inputs: 48 copy an input that already has a stored `_i` (43 sources, 3 LetRec
  results, 2 constructor Gets); 80 copy an input whose image is a view (56 Mfp, 11 Join,
  8 Threshold, 4 Mint, 1 Union).
- LetRec `r`: 31 `r._i` are read by join terms in later strata; `v._i` holds the same rows once the
  loop ends. `r._d` is filled once from `_acc` (`:1722`), and `_acc` is cleared immediately
  after (`:2087`).
- Seeds: dl8 creates one `__ir_seed_n{rel}_a{w}` Source per relation key, whether or not the program
  has seed facts for it (`_9a_ir_lower.rs:2437-2453`), and unions it into the relation body
  (`:2722`, `:2752`). Each one costs a host table and an `_i` (162 + 162, plus 5 `_d`, 9 `_x`).
  `mapped_ground` uses `Get(__ir_term_map)` + `Mfp(c0 = lit)`, not seed sources (the brief's
  wording). The corpus carries no seed facts, so the count of empty seed relations is unknown.
- Every reinstall issues all 2232: `IrSession::evaluate` reopens a session whenever the lowered IR
  differs (`_9b_ir_eval.rs:209-223`, `matches` `:109-111`), and `Engine::install` opens a new
  in-memory connection (`4_engine.rs:311-312`).

## Inherent vs choice, with removable counts

| # | category | now | removable | change | risk | file:line |
|---|---|---|---|---|---|---|
| 1 | whole plan per reinstall | 2232 per reinstall | up to the std share of 2232 per reinstall (the corpus does not show how much is std vs user) | install std once as a producer program, user rules as consumer (`composition.rs` has producer/consumer plumbing) and reuse it across comptime rounds and builds | high: dl8 unions std and user rules per relation in one program | `_9b_ir_eval.rs:209-223`, `4_engine.rs:311-312` |
| 2 | per-relation seed sources (+ prewarm, request seeds) | 162+15+7 host, same `_i`, 5 `_d`, 9 `_x` | 161 host + 161 `_i` (seeds) + 14+6 host and `_i` (prewarm, request) ≈ 362 | one shared seed source keyed by relation id, each relation reads `Mfp(c0 = rel)` (the bundle pattern), or no seed source for keys with no facts and not served | low-medium; the per-row cost stays the same (sqlite-costs: one-table vs many is a wash); only DDL drops | `_9a_ir_lower.rs:2437-2453`, `:2722`, `:2752` |
| 3 | n-way join per rule body (delta query per input) | body joins 320, ground joins 73 | ≤ 320 (all intermediate join `_d`/`_i`; `_x` move to base images); 152 settle + 167 loop | IR `Join` is already n-ary (`ivm-ir/src/0_ir.rs:177`); sqlite rejects arity != 2 (`3_nodes.rs:832-835`); dl8 emits binary left-deep (`_9a_ir_lower.rs:1366-1369`) | high: new term generator, loses shared-prefix dedup, DRed rounds, work counters | `3_nodes.rs:831-914` |
| 4 | source `_i` duplicates host table | 188 `_i` (+10 `_x`) | 188 (`_x` move to host tables) | host table already holds the post-image (written before settle, `4_engine.rs:376`); use `H_a⋈Δb + Δa⋈H_b − Δa⋈Δb` on source sides, Threshold pre-weight = host − Δ | medium | `3_nodes.rs:325-330`, `:857-884` |
| 5 | Mint deltas | settle 164 | ≤ 157 (90 one-reader, +67 once #7 drops their image) | Mint body as inline CTE in its one reader; `insert_ctor`/`insert_term` stay as steps | medium: reader must follow the minting steps (it does by node order) | `3_nodes.rs:614-632`, `:513-518` |
| 6 | join deltas under a Mint reader | 116 (110 antijoin-internal) | 116 | let `place` inline a delta read by the 2 statements of one Mint; costs evaluating the CTE twice, so count work in one statement only | medium: CPU per settle, counter parity | `3_nodes.rs:509-518`, `:625-627` |
| 6b | antijoin lowering | 123 `_d` | 0 unless output inline; a NOT EXISTS fill like the recursive path (`:919-965`) moves the `_d` to the antijoin node | single-SELECT antijoin delta (Δl absent in keys + l_i of flipped keys) inline | medium | `3_nodes.rs:967-989` |
| 7 | Mint images | 93 `_i`, 84 `_x` | 72 settle `_i` (`_x` relocate to input image / ctor PK) | Mint image as view `I(x) ⋈ ctor ON args`, as `decode` does (`:651-652`); ctor rows are never deleted (`:581`), so the view holds under retraction | low-medium | `3_nodes.rs:597-634` |
| 8 | ground constants as seed joins | 165 IR joins → 73 objects | 73 (+165 IR Joins, 165 projection Mfps, 1 term_map index) | constant as a filter literal: an install-resolved term expr (precedent `Expr::Text` → `ivm_text_id('..')`, `3_nodes.rs:213-216`); universe term ids are stable (`_9a_ir_lower.rs:2228`) | medium: the term_map row must exist before the first settle | `_9a_ir_lower.rs:217-233`, `:1372-1384` |
| 9 | LetRec result duplicates var | `r`: 37 `_d`, 31 `_i`, 11 `_x` | 68 | later strata read `v._i` as post-image (same formula as #4); `r._d` as CTE over `_acc` with the clear moved to settle clears | medium | `3_nodes.rs:1318-1321`, `:1722`, `:2087` |
| 10 | loop-owned images (views off under recursion) | `_i` 47+21+12+10, `_x` 65 | ≈ 90 `_i` | allow `view()` for rec nodes | high: DRed `active` tracking and per-round integration | `3_nodes.rs:435`, `:889` |
| 11 | SCC copies | 289 | ≤ 48 `_i` (copies of stored inputs) | copy image as outer image + applied phase delta | high | `3_nodes.rs:450-467`, `:1633-1649` |
| 12 | constructor Get | 8 | 4 `_i` | `_i` is a copy of the ctor table up to the last max; keep a one-row watermark | low | `3_nodes.rs:577-588` |
| 13 | duplicate catalog DDL | 2 statements | 2 statements (0 objects) | run catalog DDL once | none | `catalog.rs:617-619`, `:785-789` |
| - | temp tables for `_d` | 992 `_d` | 0 | `CREATE TEMP TABLE` is still a CREATE (lands in `sqlite_temp_master`) | - | `3_nodes.rs:1605` |
| - | stored unions | 37 | 0 net | each trades 1 union `_d` for ≥2 inlined join `_d` | - | `3_nodes.rs:478-490` |

Inherent to incremental maintenance as built (no change proposed): threshold-input images 159
(one consolidated image per relation; this is the "one table per relation" for non-recursive
relations), DRed state `v` 77, `b` 74, scratch 56, recursive antijoin `_d` 33, Reduce 21,
dictionaries and constructor tables 21, stage/delta/views 5, bundle output `_i` 1.

Overlaps: #2 and #4 both remove the 161 seed `_i`; together ≈ 188 `_i` + 181 host. #3 covers #8's
join objects and the body-join part of #6. #5 and #7 interact (the 67 image-bearing Mints).
