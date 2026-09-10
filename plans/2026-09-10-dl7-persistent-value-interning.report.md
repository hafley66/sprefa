> Historical read-only report. Proposals are superseded by 2026-09-10-dl6-as-dl7-userland.md; this is evidence, not implementation authority.

# DL7 layout to persistent value interning: adapter design

0. Scope boundary · 1. Finding · 2. Signatures · 3. Bridge · 4. Ownership · 5. Example · 6. Encodings · 7. Sums and unsupported shapes · 8. Compatibility checks · 9. Files · 10. Slice · 11. Unpreservable semantics

## 0. Scope boundary (user, m-1832a171, supersedes the brief where broader)

DL6 is programmed in DL7 **userland**. Layout-to-plan derivation is policy and belongs in a `.dl7` emitter over the existing `storage_*` rows; Rust and Prolog carry mechanical backend execution only. No DL6 policy or semantics moves into bespoke Prolog or Rust. Any new hosted facility, predicate or Prolog->DL7 binding, and any DL7 kernel change, STOPS for user review with its exact contract stated. Read-only checkpoint; no implementation is authorized.

**Sections 3 and 9 below predate this boundary and are held for review.** They propose a new Prolog emitter, a new CLI mainer, and Rust-side plan construction. Under the boundary the derivation moves into a `.dl7` emitter and Rust keeps only deserialize + execute. One open contract question for the user: serializing an emitter artifact to the plan JSON the Rust runtime reads — is `json_write_dict`, already used at `v7/src/4_tool/4_sqlite_query_mainer.pl:124`, an existing host capability, or does a plan-JSON writer count as a new hosted facility? Nothing is built until that is answered.

## 1. Finding

The persistent value interner already exists. `v6/sprefa-engine-rs/src/struct_plane.rs:272 intern()` is child-first, batched, returns persistent integer ids, and is driven by `StructTypePlan` — built today only by `v6/prolog/lower.pl:3460 struct_type_plan/5` from DL6 decls. The gap is one adapter: DL7 layout rows -> `StructTypePlan` + `TextInternPlan` + DDL + `IncrementalRelationPlan`. No second interner is needed.

## 2. Reusable signatures (exact, unchanged)

| symbol | file:line | role |
|---|---|---|
| `struct_plane::intern(&SqliteSeam, &[StructTypePlan], &HashMap<String,Vec<Option<String>>>, Cow<[Arrival]>, &[IncrementalRelationPlan], Option<&TextInternPlan>, &TickWork) -> BoundaryResult<Cow<[Arrival]>>` | struct_plane.rs:272 | child-first intern, rewrites ref columns to ids |
| `text_plane::intern(&SqliteSeam, &TextInternPlan, Cow<[Arrival]>) -> BoundaryResult<Cow<[Arrival]>>` | text_plane.rs:141 | scalar dictionary, 2 statements per batch |
| `incremental::apply_arrivals(&SqliteSeam, &[Arrival], &[IncrementalRelationPlan], &TickWork)`; `TickWork::probe(&SqliteSeam, &[IncrementalRelationPlan])` | incremental.rs:798, :108 | called inside `intern_type`; one relation plan per type name, one probe per batch |
| `SqliteSeam::{open,begin_tick,commit_tick,rollback_tick,variable_limit,size_statement_cache,distinct_sql_texts,count_prepares}` | sql.rs:128,273,282,288,211,194,218,188 | file db, caller transaction, batch evidence |
| `StructTypePlan{name,columns,refs,key_indices,conflict_sql,intern_sql,lookup_sql}`; `TextInternPlan{intern_sql,lookup_sql,rel_columns}` | types.rs:400, :393 | the two contracts the adapter fills |
| SQL and DDL to reproduce verbatim | lower.pl:3478/:3489/:3492 (struct intern/conflict/lookup), :3110/:3113 (text), :1303 `"__id" INTEGER PRIMARY KEY, cols, UNIQUE(cols)`, :2945 `__str` | `json_each(?)`, flat in batch size; surrogate-key law |
| `emit_compiled(prolog(Callable), CompiledUnit, Artifact, Diagnostics)`; `sqlite_query_from_view/3` dict artifact; `json_write_dict(current_output, Artifact, [width(0)])` | v7/src/3_emit/1_artifact_emitter.pl:49; 1c_sqlite_query_emitter.pl:44; v7/src/4_tool/4_sqlite_query_mainer.pl:124 | emitter door, JSON artifact precedent, public CLI JSON contract |

## 3. Bridging signature (minimal, new) — HELD, see section 0

```rust
pub struct StoragePlan {                                       // storage_plan.rs
    pub policy: String,
    pub types: Vec<StructTypePlan>,                            // topological, children first
    pub text: Option<TextInternPlan>,
    pub relations: Vec<IncrementalRelationPlan>,
    pub ref_columns: HashMap<String, Vec<Option<String>>>,
    pub ddl: Vec<String>,
    pub projections: HashMap<String, Vec<ProjectionField>>,    // storage_projection rows
}
pub fn plan_from_layout(json:&StorageLayoutJson, policy:&str) -> Result<StoragePlan, LayoutError>;
pub fn intern_values(seam:&SqliteSeam, plan:&StoragePlan, root:&str, values:&[serde_json::Value])
    -> BoundaryResult<Vec<i64>>;                               // one batch, one TickWork::probe
pub fn decode_value(seam:&SqliteSeam, plan:&StoragePlan, type_name:&str, id:i64)
    -> BoundaryResult<serde_json::Value>;                      // reverse projection
```
`intern_values` synthesises one carrier arrival per value (`rel:"__root", row:[Text(json)]`), calls `text_plane::intern` then `struct_plane::intern`, and reads the rewritten integer back. No kernel, product-apply or phase change.

## 4. State, table ownership, transaction lifetime

One table per selected type (name = type name) plus `"__str"` for the dictionary domain; the adapter owns their DDL and is the only writer. `"__id" INTEGER PRIMARY KEY` + `UNIQUE(cols)` is the identity, `INSERT OR IGNORE` the reuse. No state survives a call: `ids: HashMap<semantic,i64>` lives for one `intern_values`. The caller wraps `begin_tick`/`commit_tick`/`rollback_tick` (sql.rs:273-288); the adapter never begins or commits, and rollback drops every row minted in that span while the seam stays usable.

## 5. Worked example (RefTransitionPolicy, batch of one)

```
step 0  value  {"watched_ref":{"repository":{"url":"git@a"},"name":"main"},"old_sha":"a","new_sha":"b"}
step 1  __str         <- ["git@a","main","a","b"]      ids {git@a:1, main:2, a:3, b:4}
step 2  Repository    INSERT OR IGNORE [[1]]           lookup -> __id 1
step 3  WatchedRef    INSERT OR IGNORE [[1,2]]         lookup -> __id 1
step 4  RefTransition INSERT OR IGNORE [[1,3,4]]       lookup -> __id 1
step 5  return [1]    steady state: reopen repeats 2-4, 0 new rows, same ids
```
2 statements per dictionary batch + 3 per type per batch, flat in value count.

## 6. Required field encodings

`Value::Text(json_object)` per value; keys must equal `plan.columns` exactly, a missing or extra key panics (struct_plane.rs:63,73). Child columns carry the nested object, marked `refs[i]=Some(child_type)`. Scalars: number -> `Integer`/`Real`, bool -> `Bool`, string -> `Text`, null -> `Text("")`. `representation` maps: `reference-local-id` -> `refs[i]=Some(target)`; `dictionary-local-id` -> `refs[i]=None` plus a `true` flag in `TextInternPlan.rel_columns[type]`; `scalar` and `inline` -> `refs[i]=None`, no flag. `storage_field_layout.position` is the column order; `key_indices` = all positions (structural identity).

## 7. Sums, and known unsupported shapes

Sums are not supported by the runtime: `enum_plane::intern` (enum_plane.rs:34) is a reference-shape check only, and `decode_row`/`decode_deltas` are identity. `EnumTypePlan.identity: Option<EnumIdentityPlan>` (types.rs:432) carries intern/lookup SQL that nothing reads, so `SumPolicy`/`SpanChoice` have layout rows and no runtime path; building one is a kernel-adjacent design question for the user. Also unsupported: a policy with more than one dictionary domain; an `inline` field whose target is a product; recursive type cycles (no topological order exists); `bytes` at the text seam (text_plane.rs:83).

## 8. Layout compatibility checks (before any write)

Every `storage_type_layout.kind == "product"`; `representation` in the four known strings; per owner the positions are `0..n-1` with no gap or duplicate; `storage_dependency` acyclic and yielding a topological order; exactly one dictionary per policy in `storage_dictionary_layout`; `storage_identity_domain.local_identity == "catalog-local-intern-id"` with `semantic_identity == "constructor-and-ordered-fields"`; every row filtered to one `policy`. Failure returns `LayoutError` before DDL and before any statement.

## 9. Files to change — HELD, see section 0

Written before the userland boundary, and wrong under it where it puts derivation in Prolog: `v7/src/3_emit/1d_storage_plan_emitter.pl` (layout rows -> JSON dict artifact, modeled on `1c_sqlite_query_emitter.pl`) and `v7/src/4_tool/6_storage_plan_mainer.pl` (CLI JSON writer). Under the boundary that derivation is a `.dl7` emitter beside `v7/emitters/2_interned_storage.dl7`, deriving plan rows from the `storage_*` rows it already emits, and the Prolog side shrinks to serialization if the user rules that an existing capability.

Unchanged by the boundary, mechanical backend only: `v6/sprefa-engine-rs/src/storage_plan.rs` (serde deserialize of the emitted plan, validation, and the calls into the existing `struct_plane`/`text_plane`), `v6/sprefa-engine-rs/tests/dl7_storage_plan.rs`, and one `mod` line in `v6/sprefa-engine-rs/src/lib.rs`. Validation placement is itself open: if the section 8 checks are policy, they belong in the `.dl7` emitter and Rust only rejects a malformed plan. No moves. `sprefa-extract` and TypeSpec untouched.

## 10. Smallest executable vertical slice

`RefTransitionPolicy` over `v7/test/fixtures/16_interned_storage.dl7`: 3 products, 6 field rows, one dictionary, no sum, no plain field. Emit the plan JSON from that fixture, build `StoragePlan`, intern two values sharing a `WatchedRef` into a file-backed db, reopen, intern again, assert identical ids and zero new rows.

## 11. Caller semantics the adapter cannot preserve

1. **JSON key order changes the in-batch semantic key.** `collect` keys on `type_name + serde_json::to_string(value)` (struct_plane.rs:35,96). Two orderings of one object produce two `Collected` entries, one stored row, and one entry in `semantics_by_tuple` (:191, later insert wins), so the loser hits `relation reference normalization lost the id` (:222). The adapter must canonicalise key order to `plan.columns` before handing values in; persistent identity is unaffected, the in-batch panic is not.
2. **Errors are panics, not values.** `struct_plane` panics on shape mismatch and on `relation_reference_conflict`; `text_plane.rs:158` `.expect`s its writes. Validation runs before the call, since a panic mid-batch leaves the caller's transaction open for the caller to roll back.
3. **`intern_values` is not a tick.** It skips levels, edges and boundary reads, so `?` outputs and derived rels do not move. A caller wanting both still runs `GenProgram::run_tick`.
