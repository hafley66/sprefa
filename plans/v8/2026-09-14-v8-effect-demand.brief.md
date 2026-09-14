# v8 lane hail: effects without `Host` (rewrite of PR #741 on its own branch)

## TOC
1. The decision
2. What to delete from #741
3. What stays and what changes
4. Fixtures
5. Receipts and finish

## 1. The decision (Chris, 2026-09-14)
Any relation is settable from outside. Nothing in the program annotates a relation as hosted; there is no `Host`, no `Key`-as-mode, no hosted diagnostics. The outside says what it serves, and the evaluator writes an `effect` row when a goal on a served relation has no matching row. Loading, failed and ready states are ordinary rules over `effect` and the relation's own rows. v8 owes v7 no parity, so a dl7-facing change is allowed, but this lane keeps every `v8/oracle/**` test green by making the served set a runtime input (empty for every oracle case).

## 2. Delete from #741
- `lower_host_annotation` and the `"Host" if items.len() == 2` arm in `_2_lower/_2_declare.rs` and `_3_host.rs`. The four-item `Host` form stays untouched for now; a later lane retires it with the oracle.
- Diagnostics `hosted_relation_as_head`, `hosted_key_unbound`, `hosted_relation_without_key`, and `unbound_key_diagnostics` in `_4_comptime/_4_host.rs`; the `Hosted(Relation, Position)` row shape and every reader of it.
- `Program.hosted` and its JSON transport.
- Fixtures `bad_head.dl7`, `bad_unbound.dl7` and their expected files.
Revert every touched file outside `_6_eval` to its `cf6326e74` state unless a change below needs it.

## 3. What stays, what changes
| piece | spec |
|---|---|
| served set | `Program.served: HashSet<TermId>` of relation refs, filled by the driver from `dl8 eval --serve <name>[,<name>...]` (names resolved against the program's relation refs by their declared name; unknown names are a diagnostic `served_relation_unknown`). Absent flag = empty set. `dl8 compile` learns nothing new. |
| effect branch | in `positive_solutions`, after stored rows: if the goal's relation is in `served`, has no rules in `rules_by_rel`, and `solutions` is empty, push one row `(effect <relation ref> <application>)`. `application` is the partial application term the language already mints for a curried call: the relation applied to its bound arguments, positional or named, exactly the node `_2_lower/_1_slots.rs` builds for `(PairUser Order)` in `5_curry.dl7` and `(UserHistory (name: "Ada"))` in `4_generated_call.dl7`, interned so one binding is one row. Unbound positions are simply absent. No requirement that anything be bound: `(tick)` with nothing bound is a source. Negative goals write nothing. No `free` atom, no cons list. |
| `effect` relation | a kernel-named relation, `ref(kernel(effect))`, arity 2, added to `kernel_relation`/`kernel_slot_label`/`kernel_keys` in `_2_lower/_9_kernel.rs` (keys `[0,1]`, no return positions) so the `kernel_arity` fallback resolves rule reads of it. It never enters `KERNEL_RELATIONS` in `_3_check/_5_kernel.rs` (that changes oracle rows). |
| reading `effect` in rules | `(effect ?Relation ?Application)`; a bare relation name in argument position must resolve to the relation ref the way `HostPort`'s first column does in `8_hosted.dl7`. If `resolve_argument` (`_3_check/_2_resolve.rs`) does not resolve a bare name there, make it do so and cite the line in the PR. Read bound arguments off the application with `edge_snapshot`, the walk `2_partial.dl7:29-36` does on a `Partial`. |
| runner | not this lane. |

Comment budget as before: one line per new item, no narrative. Function length cap 70 for new functions.

## 4. Fixtures, `v8/fixtures/host_effect/`, run through the real binary with `--serve`
```lisp
; 0_pending.dl7   --serve fetch_json
(: fetch_json (* (: url text) (: body text)))
(: Watch (* (: url text)))
(Watch "https://a")
(Watch "https://b")
(: Body (* (: url text) (: body text)))
(<- (Body ?Url ?Body) (Watch ?Url) (fetch_json ?Url ?Body))
```
Expected: two `effect` rows, applications `(fetch_json "https://a")` and `(fetch_json "https://b")`; zero `Body` rows.

```lisp
; 1_settled.dl7   --serve fetch_json, same plus
(fetch_json "https://a" "hello")
```
Expected: `Body "https://a" "hello"`; one `effect` row for `https://b`; none for `https://a`.

```lisp
; 2_source.dl7   --serve tick
(: tick (* (: at int)))
(: Seen (* (: at int)))
(<- (Seen ?At) (tick ?At))
```
Expected: one `effect` row `(effect tick (tick))`, the application with nothing bound, zero `Seen` rows.

```lisp
; 3_loading.dl7   --serve fetch_json, the 0_pending program plus
(: Loading (* (: url text)))
(<- (Loading ?Url)
    (effect fetch_json ?Application)
    (edge_snapshot ?Application ?_ ?Url 0))
```
Expected: `Loading "https://a"`, `Loading "https://b"`, plus the two effect rows.

```lisp
; 4_unserved.dl7   no --serve
```
Same program as 0_pending. Expected: zero `effect` rows, zero `Body` rows, zero diagnostics. This pins that oracle cases stay silent.

Expected JSON written by hand in the `term_to_json` shape and read back.

## 5. Receipts and finish
Same validation commands as the original brief: `cargo test` (all green, count before and after), `cargo clippy --all-targets` 0, `cargo fmt --check`, `git status --short v8/oracle` empty, `git diff --stat cf6326e74...HEAD` listing only `_6_eval/**`, `_2_lower/_9_kernel.rs`, `_3_check/_2_resolve.rs` if needed, `bin/dl8.rs`, tests and fixtures. Force-push the rewritten branch, edit PR #741's title to `feat(v8): effect rows for served relations` and its body to this design, then `boop beep --no-wait --as <lane> sprefa-coordinator "effect: PR #741 rewritten, tests <before>-><after>, clippy 0"`.
