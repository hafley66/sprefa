# v8 lane: `Host` as a node annotation, `effect` rows from the evaluator

## TOC
1. Goal
2. Base and first action
3. Read first
4. Files you own, files you never touch
5. Design (decided with Chris, do not redesign)
6. Steps with receipts
7. Validation commands
8. Style laws, comment budget
9. Finish protocol

## 1. Goal
A hosted relation is an ordinary product whose declaration label is the constructor application `Host` with ONE product: `(: fetch_json (Host (* (: (Key "url") text) (: body text))))`. Its `Key` columns are the inputs. A rule may read it, never derive it. When a hosted goal runs with every key bound and no stored row matches, the evaluator writes one `effect` row instead of failing. A settled row (a seed of the hosted relation) feeds readers like any fact. Two real-binary fixtures. All 24 suites and every oracle fixture stay byte-identical.

## 2. Base and first action
- Base sha `cf6326e741499c0a7204f1ec65272af75c408cda` (`origin/main`).
- Branch `feature/v8-host-effect-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only cf6326e741499c0a7204f1ec65272af75c408cda`. Failure = stop, `boop beep` the coordinator.
- Crate `v8/`, package `dl8`. Every command runs from `v8/`.

## 3. Read first
| path | why |
|---|---|
| `chat_log/20260913.2.dl8-host-annotation-want-rows-share-macro-self-host.md` in `/Users/chrishafley/projects/sprefa/` (untracked, read it from that path) | the settled design; every `want` there reads `effect` (decision at its line 52) |
| `v8/src/_2_lower/_2_declare.rs:174-241` | `lower_target`; the `"Host" if items.len() == 4` arm at `:196` is the old four-item form |
| `v8/src/_2_lower/_3_host.rs` | `lower_host_target`, `host_metadata_rules`, port specs |
| `v8/src/_4_comptime/_4_host.rs` | `host_schema`, `host_rows`, `validate_hosted_relations`, `erase_host_planning_rows` |
| `v8/src/_3_check/_4_mode.rs` | `Bound`, `kernel_name`, `underconstrained` at `:166`: the mode diagnostic family you reuse |
| `v8/src/_6_eval/_1_program.rs`, `_5_evaluate.rs:126-290` | `Rule`, `Goal`, `positive_solutions`, `demand`, the condition at `:192-195` |
| `v8/src/_6_eval/_6_json.rs` | program JSON in, closure JSON out |
| `v7/test/fixtures/11_host_source_sink.dl7`, `8_hosted.dl7`, `5_curry.dl7`, `2_partial.dl7` | the four-item form in the oracle corpus, currying, `Key` on an edge label |
| `v7/prelude/2_constructor_rules.dl7:22-30`, `3_derived_rules.dl7:50-55` | `Key` constructor and `keyed_edge` |

## 4. Files you own
| file | change |
|---|---|
| `v8/src/_2_lower/_2_declare.rs` | one arm: `"Host" if items.len() == 2` |
| `v8/src/_2_lower/_3_host.rs` | `lower_host_annotation` for the one-product form; emits the product bind plus one `Hosted` compiler row naming the relation and its key positions |
| `v8/src/_4_comptime/_4_host.rs` | `host_rows` reads the new row shape too; `validate_hosted_relations` adds two diagnostics: `hosted_relation_as_head`, `hosted_key_unbound` |
| `v8/src/_3_check/_4_mode.rs` | hosted keysets join kernel keysets in the mode check when the checker has the rows; otherwise the check lives in `_4_host.rs`, you decide from the phase order and say which in the PR |
| `v8/src/_6_eval/_1_program.rs` | `Program.hosted: HashMap<TermId, Vec<usize>>` relation → key positions |
| `v8/src/_6_eval/_5_evaluate.rs` | the `effect` branch in `positive_solutions` |
| `v8/src/_6_eval/_6_json.rs` | `hosted` in and out of program JSON; `effect` rows print like any closure row |
| `v8/tests/_14_host_effect.rs` | new, real binary |
| `v8/fixtures/host_effect/*.dl7`, `*.expected.json` | new |
Never touch: `v8/src/_9_runtime/**` (another lane), `v8/src/bin/dl8.rs`, `v8/Cargo.toml`, `_6_eval/_0_term.rs`, `_3_table.rs`, `_4_kernel.rs`, `v8/oracle/**`, `v7/prelude/**`, `v7/`, `v6/`.

## 5. Design (decided)
- The four-item `Host` form STAYS. Every compile oracle case with `8_hosted` or `11_host_source_sink` must stay byte-identical; the one-product form is added beside it. Removal of the old form waits for the oracle's retirement (coordinator's note, not yours).
- No new keyword. `Host` in label position, `Key` on the edge label, both already lower. The lowerer's new arm mints the product exactly like `"*"` at `_2_declare.rs:191` and adds one compiler row `(Hosted <relation ref> <key positions list>)`.
- Key positions: every edge whose label is a `Key` application, in edge order. `Key` is spelled `(Key "url")` today; keep that spelling.
- Checker: a hosted relation as a rule head is `hosted_relation_as_head`; a goal on a hosted relation where a key argument is not bound by earlier goals is `hosted_key_unbound` (same variable-flow test `Bound` at `_4_mode.rs:22` already runs for kernel keysets). No stratum change is needed: a hosted relation has no rules, so it is a base relation to the stratifier.
- `effect` is NOT declared in the prelude (that would change every oracle's bytes). The evaluator mints its rel term as `ref(kernel(effect))` through `Universe` and writes rows `(effect <host relation ref> <key list> <rule origin>)` into the same sink as derived rows; `closure_to_json` prints them unchanged. No rule may read `effect` in this lane; the reader side (the `share` rules) is a later lane.
- Evaluator branch, inside `positive_solutions` after stored rows are collected: if `program.hosted` has the goal's relation, and every key position in `pattern` is `Some`, and `solutions.is_empty()`, push one `effect` row with the key list built by `u.list(&keys)` and the rule origin as `u.int(rule_index)`, deduplicated by the store like any row. The goal still yields no solutions. Negative goals on hosted relations write nothing.
- Semi-naive: an `effect` row is a derived row of the current round; it is retracted the way nothing else is retracted today, so this lane does not retract. When the settled seed exists, the branch never fires, which is fixture 1.

## 6. Steps with receipts
1. Lowerer arm and `Hosted` row. Receipt: `dl8 compile v8/fixtures/host_effect/0_pending.dl7` prints zero diagnostics and one `Hosted` compiler row; all oracle tests green.
2. Diagnostics. Receipt: two negative fixtures under `v8/fixtures/host_effect/bad_*.dl7`, each asserted by name through the real binary's diagnostics JSON.
3. `Program.hosted` through JSON, evaluator branch. Receipt: fixtures 0 and 1.

```lisp
; 0_pending.dl7
(: fetch_json
   (Host (* (: (Key "url") text) (: body text))))
(: Watch (* (: url text)))
(Watch "https://a")
(Watch "https://b")
(: Body (* (: url text) (: body text)))
(<- (Body ?Url ?Body)
    (Watch ?Url)
    (fetch_json ?Url ?Body))
```
Expected closure: two `effect` rows, one per url, zero `Body` rows.

```lisp
; 1_settled.dl7   same, plus one settled seed
(fetch_json "https://a" "hello")
```
Expected closure: one `Body "https://a" "hello"`, one `effect` row for `https://b`, none for `https://a`.

```lisp
; bad_head.dl7
(<- (fetch_json ?Url "x") (Watch ?Url))      ; diagnostic hosted_relation_as_head
; bad_unbound.dl7
(<- (Body ?Url ?Body) (fetch_json ?Url ?Body))   ; diagnostic hosted_key_unbound
```
Expected JSON files are written by hand in the `term_to_json` shape and read back before commit.

## 7. Validation commands
```bash
cd v8 && cargo test 2>&1 | grep -E "^test result|FAILED|panicked"
cd v8 && cargo clippy --all-targets 2>&1 | grep -c "^warning\|^error"   # 0
cd v8 && cargo fmt --check
git status --short v8/oracle          # empty
git diff --stat cf6326e741499c0a7204f1ec65272af75c408cda...HEAD   # owned files only
```
Every test runs through the real `dl8` binary. No unit fakes.

## 8. Style laws, comment budget
- COMMENT BUDGET IS HARD: a comment states only a constraint the code cannot show, one line, no narrative, no design references, no "per the session". Expected comment lines added across the lane: under 12. The coordinator counts them.
- Tests only under `v8/tests/`, files `_<n>_name.rs`, everything `pub`, no `eprintln!`, no em dashes, banned words (provenance, substrate, load-bearing, regime, honest, ground as a verb, refusal, "ground truth"), no shortnames (`relation` not `rel` in new identifiers, `position` not `pos`), no function over 70 lines. Language words: rxjs, prolog, SQL only; "support" is banned.
- Commit message ends with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## 9. Finish protocol
Commit, push, PR to `main` titled `feat(v8): Host annotation and effect rows`, body with: the four fixtures, which phase carries the key check and why, test counts before and after, clippy count, comment lines added, `git status --short v8/oracle` output. End with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`. Then `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "host effect: PR #<n>, tests <before>-><after>, clippy 0"`. Never merge, never spawn subagents.
