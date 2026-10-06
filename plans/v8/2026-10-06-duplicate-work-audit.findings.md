# Findings: duplicate-work audit

Brief: `plans/v8/2026-10-06-duplicate-work-audit.brief.md`. sprefa `65eb1714d` (main `a5f67af2f` plus
the brief), sqlite_ivm main `68cbda8`, dd engine. Counts only, no clock, no code change on main. The
instrumentation lived in scratch worktrees of both repos and was removed after the runs.

## Method

- Runs, each `DL8_ONE_ENGINE=0` and `=1`: hello = `dl8 compile plans/v8/probes/2026-10-05-one-dd-engine/0_hello.dl7`;
  registry = `dl8 build std/tsi_registry_emit.dl7`; census = `dl8 compile` of the 132 `*.dl7` under
  `fixtures examples book/src/probes std`, `-P 4`. Exits: 112 zero / 20 nonzero in both modes, the same
  programs; stdout equal between modes for all 132; registry output equal between modes.
- IR: every install dumped (`DL8_IR_DUMP`). Structural fingerprint per node (Merkle): op, canonical
  parameters (texts and term literals resolved to their values), input fingerprints (Union inputs sorted).
  A `Get` of a Let relation fingerprints as its body, of a LetRec variable as (group, index) where the group
  fingerprints its bodies with the variables as placeholders, of a Source as its name with the `pN.` prefix
  stripped, of a constructor as its name. "name level": a `Get` of any derived relation fingerprints as its
  prefix-stripped name (same operator over same-named inputs, contents may differ). `srcfree`: no Source
  in the node's subtree. Join input order is kept (no commutation), so equal joins with swapped inputs are
  not counted.
- dd (scratch `ivm-dd`): per install, per node build (`Rel::observe`) its scope and the worker's
  `peek_identifier()`; per LetRec scope its parent; per hoisted input its entered stream; per arrangement
  site (`keyed` memo miss and hit, `antijoin` both sides, `reduce`, `threshold`, `topk`, `window`) its
  scope, input stream and key. A stream resolves to the IR node built or entered on it in that scope.
  Operators of a build = identifier after the build minus identifier after the previous build, so entered
  and scope operators fall on the next build. "dd operators" below = identifiers allocated during `lower`
  (source and constructor inputs excluded; hello #1 1,785 here against 1,845 in the shrink findings).
  "arrangements" counts every site kind, not only the join arrangements `dd install` logs.
- Rust (scratch `src/dupaudit.rs`): per call of a site its item count, items equal to the previous call of
  the same site, items equal to any earlier call (hashes of `TermId` rows, `(rel, row)` changes, or
  `(engine, constructor, row)`): round rules, round seeds, round closure, `snapshot_closure` rows,
  `decode_terms_counted` constructor rows and the terms it builds, `bootstrap_changes` changes, rules each
  lowering receives. Events: every `lower_declared` (caller, `compile_rule` calls, nodes), every
  `evaluate_compiled` / `prepare` lowering (fingerprint, session reused), every `structural_fingerprint`
  and `relocate` (nodes), every comptime round.

## 1. Base counts

| run | installs | IR nodes | dd operators | arrangements | lowerings | nodes lowered | comptime rounds (after the first) |
|---|---|---|---|---|---|---|---|
| hello, one=0 | 3 (351 / 379 / 876) | 1,606 | 7,827 (1,785 / 1,871 / 4,171) | 553 (125 / 125 / 303) | 3 | 1,606 | 2 (0) |
| registry, one=0 | 3 (351 / 379 / 1,997) | 2,727 | 12,385 (1,785 / 1,871 / 8,729) | 727 (125 / 125 / 477) | 4 | 4,724 | 3 (1) |
| census, one=0 | 383 | 210,090 | 1,014,241 | 70,827 | 418 | 246,894 | 285 (38) |
| hello, one=1 | 1 (composed 1,737) | 1,737 | 8,126 | 568 | 5 | 3,123 | 2 (0) |
| registry, one=1 | 2 (1,737 / 1,002) | 2,739 | 12,369 (8,126 / 4,243) | 729 (568 / 161) | 9 | 7,131 | 3 (1) |
| census, one=1 | 232 | 243,401 | 1,135,604 | 78,914 | 840 | 444,452 | 285 (38) |

## 2. Duplicates and fixes, ranked

Ranked by the census count removed in the row's own unit (one=0 unless the duplicate exists only at
one=1); units differ between rows. Evidence lines are on `65eb1714d` / sqlite_ivm `68cbda8`.

| # | location | duplicate count (hello / registry / census, one=0; one=1 where it differs) | evidence | fix | count removed (census) | safety condition |
|---|---|---|---|---|---|---|
| 1 | relocation of a part's `Lowered` on every session call (one=1) | `relocate` clones: hello 8 calls / 5,516 nodes, registry 18 / 13,074, census 1,203 / 850,421 | `src/_6_eval/_9b_ir_eval.rs:181` (`local`), `:816` | relocate once per part session and keep it | 810 calls, 622,874 IR nodes cloned (one relocation per part session remains: 393 calls, 227,547 nodes) | the relocation map and the part's `Lowered` are fixed for the session's life |
| 2 | Rust scans of the whole closure every comptime round after the first | closure rows equal to the previous round's: registry 3,317 of 3,323; census 127,263 of 128,101 (38 rounds), each scanned by `strip_snapshot_rows`, `colon_rows`, `intern_rows`, `assemble_generated_program` | `src/_4_comptime/_2_rounds.rs:243`, `:319`, `:320`, `:321` | carry the settle delta as the round's closure change; filter and assemble over the change, keeping the edges, requests and assembled index between rounds | 4 x 127,263 = 509,052 row visits | each scan is a per-row filter except `assemble_generated_program`, whose `index` must be maintained under retraction; the stability check compares the maintained sets |
| 3 | `decode_terms_counted` reads every constructor row of the engine on every call and rebuilds every term not in `mapped` | constructor rows read that the same site read before: 0 / 15,774 of 27,163 / 379,657 of 734,713; one=1: 0 / 85,909 of 106,319 / 1,393,851 of 1,873,303. Terms rebuilt: 0 / 4,826 / 24,477; one=1 census 123,318 of 142,597 | `src/_6_eval/_9b_ir_eval.rs:1070` (starts from `mapped`, not `Core.decoded`), callers `:915`, `:1714`, `:1833`, `:2039` | keep the decoded map in `Core` (it exists, only `decode_delta` `:1388` extends it) and read constructor rows above an id watermark | 379,657 rows read, 24,477 terms built; one=1: 1,393,851 rows, 123,318 terms | dictionary ids are append-only per engine; `mark`/`rewind` already restore `Core.decoded` (`:327`, `:347`); the `engine`-keyed hash can match rows of a dropped engine at a reused address, so the one=0 totals are an upper bound (rounds after the first, same engine: 252,326 of 256,521) |
| 4 | macro rules lowered and installed twice per process: #1 macro library comptime (sealed, 569 seeds), #2 macrotime cone (`prepare`, unsealed, 6 seeds) | same 66 rules in both (131 of 131 processes with both installs). #2 nodes name-equal to #1 nodes: 338 of 379 per process; census 44,278 of 49,649. Merkle-equal: 11. Install cost #1 / #2: hello 351 nodes, 1,785 ops, 125 arrangements / 379, 1,871, 125; census 45,981, 233,835, 16,375 / 49,649, 245,101, 16,375. one=1: parts p0 and p2 of the composed install, 220,080 / 231,346 operators, 16,375 / 16,375 arrangements, 44,278 name-equal nodes | `src/_6_eval/_9b_ir_eval.rs:768` (#1), `:578` from `src/_1_macrotime/_4_expand.rs:199` (#2); one=1 `src/_8_driver/_7_one.rs:132` | one install of the 66 rules lowered unsealed with the cone's declared sources and shapes; the library round runs on it, then the cone's base enters as the signed difference from the library seeds | one install: 45,981 nodes, 233,835 dd operators, 16,375 arrangements (one=1: part p0, 220,080 operators) | the cone's rule set equals the library's (true in 131 of 131); unsealed lowering of the library round gives the same closure as sealed (shrink change 6 parity); 569 - 6 seed rows retracted in one settle; the library round runs before any unit enters |
| 5 | `structural_fingerprint` recomputed for IR already fingerprinted (one=1) | per composed install: p0's fingerprint (passed into `open`) recomputed in `EngineSession::on`, p2 and p3 recomputed again at the pool push: 3 calls, 1,737 nodes serialized. Census: 1,364 fingerprints / 899,546 nodes against 840 lowerings | `src/_6_eval/_9b_ir_eval.rs:430`, `:546` | pass the fingerprints computed at lowering and at `install_composed` | 393 calls, 227,547 nodes serialized | fingerprint is a function of the IR; the IR is not changed between the calls |
| 6 | stdlib (p3) and macro cone (p2) lowered twice (one=1): once into the pool, once by `prepare` to adopt the part by IR equality | hello 2 lowerings / 1,386 nodes; census 262 / 181,566 nodes (and their `compile_rule` calls) | `src/_8_driver/_7_one.rs:129`, `:132`; `src/_6_eval/_9b_ir_eval.rs:578`, `:517` | adopt by the lowering inputs (program, `Declared`) or hand the pooled `Lowered` to `prepare` | 262 lowerings, 181,566 nodes | `lower_declared` is a function of (program, declared, universe term ids); both calls see the same three |
| 7 | `snapshot_closure` decodes the whole visible closure every round | rows equal to the previous round's snapshot, rounds after the first: registry 4,358 of 4,653; census 155,179 of 157,532 | `src/_6_eval/_9b_ir_eval.rs:909` | decode the settle delta (`decode_delta` `:1388` does this for `apply`) and keep the decoded closure | 155,179 rows decoded | same as row 2 |
| 8 | `bootstrap_changes` rebuilds the whole seed frontier (unit, one term-map row per mapped term, every seed) on every run, then `run_session` diffs it against `last_seeds` | changes equal to the previous call: 60 of 4,478 / 5,154 of 14,437 / 135,035 of 860,283; one=1 census 420,301 of 1,220,581 | `src/_6_eval/_9b_ir_eval.rs:1240`, `:1665`-`:1695` | build the frontier from the new seeds and newly mapped terms only | 135,035 changes (one=1: 420,301) | no other writer changes these source relations between runs (the session owns them; answers go to `.answer` sources) |
| 9 | an IR node built inside a LetRec scope is built again in the outer scope when a later stratum reads it (hash-consing shares the node between an in-loop rule body and a later Let) | extra builds / dd operators / arrangements: hello 262 / 919 / 51, registry 371 / 1,211 / 63, census 33,924 / 117,230 / 6,617; one=1 census 35,387 / 114,985 / 6,848. Census readers: `__ir_intern_requests_L` (the request union over in-loop intern nodes) 20,340 builds / 62,379 ops; ordinary derived Lets 13,398 / 51,223. Every rebuild is in an ancestor scope of the first build | ivm-engine `src/1_rel.rs:321` (outer memo), ivm-dd `src/2_dd.rs:149` (scope memo starts empty, only `results` leave at `:192`); lowering `src/_6_eval/_9a_ir_lower.rs:3569` | lowering a node in the outer scope that a finished LetRec scope built: `leave()` the scope's collection instead of rebuilding (or export the request union as one more LetRec variable) | 33,883 builds, 117,072 dd operators, 6,617 arrangements (the 41 builds / 158 operators under a limit stay) | the LetRec has no `limit` (33,883 of 33,924 builds; 35 under limit 64, 6 in a nested LetRec under 64); a left collection accumulates to its value over the fixed point, which is what the outer recomputation over the LetRec outputs produces |
| 10 | `compile_rule` runs twice per rule per lowering (the recording pass compiles every rule into a cloned builder for `narrow_unions`) | calls: hello 546 (273 x 2), registry 952, census 79,934; one=1 census 133,134 | `src/_6_eval/_9a_ir_lower.rs:3139`-`:3178` | compute the narrowing unions without building nodes, or keep the recording pass's nodes | 39,967 calls (one=1: 66,567) | the real pass needs every relation's narrowing union before it compiles any rule |
| 11 | a comptime round after the first lowers, fingerprints and compares the same IR, then reuses the session | reused lowerings: registry 1 / 1,997 nodes; census 35 / 36,804 nodes (3 of 38 later rounds lowered a different IR); one=1 census 84 / 19,485 (61 in later rounds, 23 in first rounds, the S5 loop passes). Rules equal to the previous round's: census 4,596 of 4,597, also converted by `round_program` and checked by `check_resolved_rules` | `src/_6_eval/_9b_ir_eval.rs:768`, `:781`; `src/_4_comptime/_2_rounds.rs:210`, `:508` | skip conversion, check and lowering when the rule list (TermIds), the set of seeded relation keys, served relations and constructor shapes equal the previous round's | 36,804 nodes lowered and serialized, 4,596 rules converted and checked | sealed lowering depends on those four inputs only (texts hold only term-table texts, shrink change 2) |
| 12 | `ivm_ir::compose` concatenates texts and term tables per part | duplicate term entries / texts in the composed program: hello 48 / 9 per process; census 6,288 of 18,995 / 1,179 of 3,537 | sqlite_ivm `crates/ivm-ir/src/4_compose.rs:119`, `:120` | deduplicate terms and texts by value while composing | 6,288 term mints, 1,179 text interns at install | a term entry is a value: functor by name (already one relation per name) and resolved args |
| 13 | identical demand copies per stratum level (`adorned` key carries `level`): `dmd__owner__..__l2`, `__l3`, `__l6` and two `adn__` copies have equal bodies | hello 3 relations / 18 nodes / 4 arrangements (#3); registry 0; census 300 relations / 1,800 nodes / 5,500 dd operators / 400 arrangements (100 processes) | `src/_6_eval/_9a_ir_lower.rs:2565` | hash-cons Let strata: a Let whose body node equals an earlier Let's aliases it (or key the adornment without `level` when the copies' bodies are equal) | 1,800 nodes, 5,500 dd operators, 400 arrangements | equal body node means equal collection; readers of the aliased relation then share their nodes through `Builder::node` |
| 14 | `ivm_ir::compose` does not hash-cons across parts | composed nodes Merkle-equal to a node of an earlier part: hello 21, census 2,751: 1,310 `srcfree` (1,048 `Get`s of constructors and of `pN.__ir_semantic`, 262 `__ir_semantic` Unions, one per p2 and p3) and 1,441 `Get`s of same-named seed sources; arrangements on them 917, of which 524 `srcfree` (on `__ir_semantic`) | sqlite_ivm `crates/ivm-ir/src/4_compose.rs:77`, `:82`-`:104` | hash-cons `srcfree` nodes while composing (map each part node to an existing equal node) | 1,310 nodes (262 with operators), 524 arrangements | only nodes reading constructors and term literals; nodes over same-named sources need equal rows in both parts (`__ir_seed_*` hold each part's seeds) |
| 15 | outer collection arranged in the outer scope and, entered, arranged again inside a LetRec on the same key | hello 8, registry 22, census 1,068 (one=1 1,106) arrangements. All entered arrangements: census 3,605 join, 133 antijoin | ivm-dd `src/2_dd.rs:142`, `:150`, `:254` | arrange in the outer scope and enter the arrangement (`Arranged::enter`), the scope memo keyed on the outer stream | 1,068 arrangements | the input is loop-invariant (`independent_inputs` hoisted it) |
| 16 | `antijoin` arranges both sides without the `keyed` memo | same scope, same node and key, arranged again: antijoin right side 365, antijoin left vs a join's arrangement 129, reduce 2; hello 4, registry 5, census 496 (one=1 580) | ivm-dd `src/2_dd.rs:491` | memo the right side's thresholded key set per (stream, key); arrange the left side through `keyed` | 496 arrangements | same input stream and same key columns |
| - | comptime install #3 reuses no node of the macro library install #1 (one=0, separate engines) | Merkle-equal nodes in a later install of the same process: hello 21 (10 `srcfree`), census 5,682 (1,266 `srcfree`) | | row 14 under one=1 | | separate engines share nothing |

## 3. The brief's suspects

| suspect | measured | rows |
|---|---|---|
| 1. macro rules installed twice | 66 rules in both, 131 of 131 processes; 338 of 379 cone nodes name-equal, 11 Merkle-equal (the library round is sealed, the cone unsealed: their Lets differ by seed sources); a whole install each | 4 |
| 2. compose renames | Merkle-equal across parts: 2,751 nodes (1,310 `srcfree`), 917 arrangements (524 `srcfree`). The 44,278 name-equal p2 nodes are suspect 1 inside one engine | 14, 12, 4 |
| 3. demand copies | per original relation one adorned copy per (binding pattern, stratum level); copies equal in body only across levels: 1,800 nodes in 100 processes. Demand-reached nodes in hello #3: 111, of which 13 shared with non-demand relations and 98 exclusive (registry 126 / 13 / 113) | 13 |
| 4. comptime rounds | 38 later rounds in the census; each re-lowers (35 of 38 the same IR), re-converts 4,596 of 4,597 equal rules, rebuilds the seed frontier, rescans and redecodes the closure | 11, 2, 7, 8, 3 |
| 5. dd arrangement memo per scope | arranged more than once on the same (node fingerprint, key): census 8,581 of 70,827: 6,617 a node rebuilt outside its LetRec (row 9), 1,068 entered (row 15), 496 antijoin (row 16), 400 equal demand copies (row 13) | 9, 15, 16, 13 |
| 6. other | `decode_terms` re-reads, `relocate` clones, redundant fingerprints, pool double lowering, `compile_rule` twice, compose texts/terms | 3, 1, 5, 6, 10, 12 |

## Instrumentation (removed)

Scratch worktrees `sprefa-wt/duplicate-work-scratch` (sprefa, detached at `65eb1714d`) and its
`sqlite_ivm/` (sqlite_ivm worktree, detached at `68cbda8`): `src/dupaudit.rs` and calls in
`_4_comptime/_2_rounds.rs`, `_6_eval/_9a_ir_lower.rs` (`lower_declared` wrapper, `compile_rule` counter,
relation display names), `_6_eval/_9b_ir_eval.rs`; ivm-dd `DL8_DUP_DUMP` events (install, scope, enter,
node, arrangement site). Analysis scripts lived in the session scratchpad.
