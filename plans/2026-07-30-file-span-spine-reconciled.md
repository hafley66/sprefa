# File span spine, reconciled against the locked single-rel model

Reconciles four documents that predate `locked(single_rel_type_system)` and the
landed relation-reference runtime:

| doc | role after this reconciliation |
|---|---|
| `plans/2026-07-29-file-span-design.md` | user intent record; its model section is adopted, its 3 cards resolved below |
| `plans/2026-07-29-file-identity-span-spine.md` | superseded on physical storage (its own header says so); its identity analysis is adopted; 3 `todo(decision)` resolved below |
| `plans/2026-07-29-file-span-storage-lab.md` | measured selection; adopted whole; 2 `todo(decision)` resolved below |
| `plans/2026-07-29-file-span-kernel-host-boundary-lab.md` | current-compiler receipts; D1/D2/D3/D5 resolved, D4 stays open |

Base: `6c3a7e2d26854a4a646a578b672dd6a18f54c6d0`, branch `lane/filespan-reconcile`.
Every receipt below was produced in this worktree under
`SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1` with scratch databases.
Baseline on this tip: conformance **175 PASS / 0 other**.

This lane wrote no production code. Section 5 says why.

---

## 1. What the locks and the landed runtime settle

### 1.1 The nested `file_span` reference is already an ordinary rel column

`locked(single_rel_type_system)` says a rel column naming another rel denotes a
generated relational edge normalized into target rows and integer keys. Commit
`9a245a2e` landed that runtime. There is no new spelling to invent: the whole
spine is `rel` declarations whose column types name other rels.

The spine in **current syntax**, no construct that is not already parsed,
checked, and lowered today:

```
rel repo(name: text).
rel path(text: text).
rel file(repo: repo, path: path).

rel revision(repo: repo, identity: text) key(1, 2).
rel committed(revision: revision, oid: text) key(1).
rel worktree(revision: revision, root: text, base: revision) key(1).

rel blob(digest: text, byte_len: int) key(1).
rel git_blob(blob: blob, repo: repo, oid: text) key(1, 2).
rel stored_blob(blob: blob, bytes: text) key(1).

rel rev_file(revision: revision, file: file, blob: blob) key(1, 2).
rel file_span(file: rev_file, start: int, end: int).

rel newline(blob: blob, offset: int) key(1, 2).
```

Analysis facts reference one `file_span` column and carry no path, digest, or
coordinate of their own:

```
rel df_node(at: file_span, kind: text).
rel diag(at: file_span, severity: text, code: text, msg: text).
```

Construction and dereference are the same relation-shaped term in argument
position; no `ref(...)`, no `Key(...)` wrapper, no JSON:

```
file(repo(repo_name), path(path_text)) <- observed(repo_name, path_text).
df_node(file_span(rf, start, end), kind) <- extracted(rf, start, end, kind).
coord(path_text, start, end) <- file_span(f, start, end), decode(f, {path: p}), decode(p, {text: path_text}).
```

`0_relation_edge_expand.pl` adds the target membership atom to any rule whose
head carries a relation-shaped value, so target existence is a visible
dependency in stratification and the level fixpoint. Target rows stay public and
queryable in the same tick — fixture
`relation_reference_target_and_parent_share_tick`.

### 1.2 Receipt: keyed spine rels cannot be level heads

Measured this lane, exact refusal text:

```
rel revision(repo: repo, identity: text) key(1, 2).
revision(repo(repo_name), rev) <- observed(repo_name, rev).
  -> unsupported_construct(keyed_level_head(revision/2))
```

The same rel headed by an edge rule compiles:

```
blob(digest, byte_len) <+ raw(digest, byte_len).      -> compiles
```

Consequence for the spine, not a card: every keyed spine rel (`revision`,
`committed`, `worktree`, `blob`, `git_blob`, `stored_blob`, `rev_file`) is
world-fed or edge-headed. That is what the design already wants — these rows come
from extraction batches and the watcher — but a stage-2 author writing
`blob(...) <- ...` will hit the refusal, so it is stated here.

### 1.3 `file_span` needs no key decl

`locked(file_span_shape)` gives `file_span(rev_file, start, end)` with
`key(rev_file, start, end)`. That key is the full row, and an unkeyed set rel
already uses full-row identity (`locked(identity_policy)`). Declaring the key is
therefore optional and costs a `keyed_level_head` refusal if the rel is ever
level-headed. Recommendation carried into stage 3: declare `file_span` unkeyed.

---

## 2. Card table

### 2.1 Settled

| # | card, as written | closed by | answer |
|---|---|---|---|
| D1-C1 | `text()` through a world host reading bytes **vs** a stored file-content plane | `locked(optional_capabilities)` (rel-edge lane) + storage-lab decision 13 + measurement | **Both, as two additive rels.** `git_blob` and `stored_blob` are memberships over one `BlobRef`; a blob may carry either or both; absence needs no NULL column. Measured 300 blobs / 8.50 MB x3: git `cat-file --batch` 58.25-58.88 ms, `stored_blob` 12.85-12.88 ms at 8.67 MB. The "vs" dissolves. |
| D1-C2 | `file` as both rel and type, or unified | `locked(single_rel_type_system)` + `locked(one_declaration_family)` + commit `9a245a2e` | **Unified, one rel.** `file` is a rel; naming it in column position generates the edge; its membership stays public. There is no type plane to be a member of. |
| D1-C3 | rev on the file value now, or later | `locked(file_span_shape)` + `exit_order(4, file_revision_blob_span_spine)` | **Now.** The locked shape is `file_span(rev_file, start, end)`; `rev_file` carries the revision. All three later docs agree; the exit-audit row names revision in the spine. |
| D2-S1 | confirm the semantic names File, Blob, RevFile, FileSpan | `locked(file_span_shape)` (`file_span`, `rev_file`) + `locked(optional_capabilities)` (`blob`) + D1-C2 (`file`) | **The four named in the card are fixed.** See open card **A** for the three names the same card's spirit covers that no lock touches. |
| D2-S3 | user-visible declaration spelling for stdlib value views, separate from their rel declarations | `locked(surface_freeze)` + `locked(single_rel_type_system)` + `locked(one_declaration_family)` | **No separate spelling exists or may be introduced.** A value view is an ordinary rel plus ordinary rules. Card dissolves. |
| D3-T1 | surface spelling for generic relation-reference columns; accept or reject `ref(Relation)` | `locked(single_rel_type_system)` ("no relation-like intermediate type") + `locked(ref_current_verdict)` ("no ref surface construct") + landed spelling | **The spelling is the target rel name in column-type position.** `ref` is dead. Card dissolves; `PLANS.md` row `file-span-storage-lab.md:531` is closable. |
| D3-T2 | authoring spelling for registered typed host executors | `locked(host_relation_surface)` + `locked(host_contract)` (executor key is a contract field) + `locked(host_shape)` | **No authoring spelling.** Executors are registered internally; two already exist (`host_executor(shell)`, `host_executor(sprefa_extract)`). A host relation is written as an ordinary RHS atom and its contract selects demand-response lowering. Card dissolves; `PLANS.md` row `file-span-storage-lab.md:534` is closable. |
| D4-D1 | relation-domain columns: automatic typed edges vs alternatives | landed `9a245a2e` + `locked(reference_is_edge)` | **Automatic typed edges**, with the depth limit in section 3. |
| D4-D2 | `ref` options 1-5 | `locked(ref_current_verdict)` | **Option 5 / option 1.** Scanning a target captures `__id`; typed variables forward it without a rejoin; `decode` joins `__ref_<target>` when fields are needed. |
| D4-D3 | strings: universal `string` rel vs domain rels | measurement (3,019 paths x20 refs; 39,642 name occurrences over 200 files; **zero** path/name text overlap; universal and separate dictionaries both 1,728,512 B) | **Domain rels with plain `text` columns.** Interning stays a physical store policy. Note: the kernel-host lab's own signature block contradicts its D3 by writing `path(text: string)`; section 1.1 uses `rel path(text: text).` and that is the spelling that lands. |
| D4-D5 | provider binding options 1-4 | `locked(host_relation_surface)` + `locked(host_contract)` + `locked(host_shape)` | **Option 1/2 as already implemented.** Existing demand/response plan, registered executor by contract, `bind` stays the continuous world source. No `host rel`, no arrow-return surface. |

### 2.2 Open

| # | card | why no lock answers it | what it decides |
|---|---|---|---|
| **A** | Spelling of the three spine names the locks do not fix: `repo`, `path`, and the revision family. | Three docs spell the revision family three different ways: `rev` (spine doc), `committed_rev` / `work_rev` (storage lab), `revision` + `committed` / `worktree` (kernel-host lab). `locked(file_span_shape)` fixes only `rev_file` and `file_span`. Rel names are program identifiers, so `surface_freeze` does not reach them. | Every program's join text, every fixture, the `PLANS.md` decision row `file-identity-span-spine.md:363`. Picking one silently is exactly the default this lane is forbidden to pick. Section 1.1 shows the kernel-host spelling **as a proposal, not a selection**. |
| **B** | Where line and column live: provider emits `newline(blob, offset)` rows and DL6 owns line/col rules (kernel-host D4 option 3), **or** the provider returns `span_position(...) -> start_line, start_col, end_line, end_col` (storage-lab host section, D4 option 1). | The two docs answer oppositely and both are user-facing. `plans/2026-07-29-file-span-design.md` states line/col "Belongs in-language", which is option 3; the storage lab lists `span_position` as a provider output, which is option 1. No lock arbitrates. | Whether `newline` is a durable relation (measured 212,892 B of index for 300 blobs, so a real table at corpus scale) or a provider-side cache; whether the LSP line numbers and the flow referee's translator are replaced by DL6 rules or by host output columns. Blocks stage 4 only. |
| **C** | Work-revision identity: one stable `rev_id` whose `rev_file` rows change, **or** a new observation revision minted per accepted watcher batch. (`PLANS.md` row `file-identity-span-spine.md:366`.) | The kernel-host lab defers it explicitly ("according to its existing key and clock policy"). No lock, no measurement. | Whether every editor save churns `file_span` identity for the whole file (mint-per-batch) or only the placement row moves (stable rev). Interacts with the extraction-live content-addressed zero-tick receipt and with `c7_durable_carry`. Blocks stage 3. |

Three cards open. Per the task contract this lane **stops at the plan**.

Two of the five `PLANS.md` decision rows survive: `file-identity-span-spine.md:366`
(card C) and a new row is owed for card A and card B. The other three
(`file-identity-span-spine.md:363`, `:370`, `file-span-storage-lab.md:531`, `:534`
— four rows, three cards) are closable as stage-0 bookkeeping.

---

## 3. Receipts from this lane: two silent divergences at the spine's own shape

The spine is depth-3 to depth-4 by construction
(`file_span` -> `rev_file` -> `revision` -> `repo`). Relation-value patterns at
depth >= 2 are broken in **both** directions today, silently, with zero fixture
coverage. This is the reason stage ordering below puts a defect before the decls.

### 3.1 Relation pattern at depth >= 2: oracle right, emitter empty

Program (`l3f.dl6`, depth 2):

```
rel repo(name: text).
rel path(text: text).
rel file(repo: repo, path: path).
rel span(file: file, start: int, end: int).
rel raw(repo_name: text, path_text: text, start: int, end: int).
repo(repo_name) <- raw(repo_name, _, _, _).
path(path_text)  <- raw(_, path_text, _, _).
file(repo(repo_name), path(path_text)) <- raw(repo_name, path_text, _, _).
span(file(repo(repo_name), path(path_text)), start, end) <- raw(repo_name, path_text, start, end).
coord(path_text, start, end) <- span(file(_, path(path_text)), start, end).
```

One arrival `raw('acme','src/a.rs',10,20)`.

| rel | oracle (`dl6_oracle.pl`) | emitted SQL against real sqlite3 |
|---|---:|---:|
| repo | 1 | 1 |
| path | 1 | 1 |
| file | 1 | 1 |
| span | 1 | **0** |
| coord | 1 | **0** |

Depth-1 construction is correct:

```sql
INSERT OR IGNORE INTO "file" ("repo","path")
  SELECT b1."__id", b2."__id" FROM "raw" b0, "repo" b1, "path" b2
  WHERE b1."name" = b0."repo_name" AND b2."text" = b0."path_text"
```

Depth-2 construction is not. `b1."repo"` is the INTEGER `__id` the statement
above just wrote:

```sql
INSERT OR IGNORE INTO "span" ("file","start","end")
  SELECT b1."__id", b0."start", b0."end" FROM "raw" b0, "file" b1
  WHERE json_extract(b1."repo", '$.fn') = 'repo'
    AND json_extract(b1."repo", '$.args[0]') = b0."repo_name"
    AND json_extract(b1."path", '$.fn') = 'path'
    AND json_extract(b1."path", '$.args[0]') = b0."path_text"
```

`json_extract(<integer>, '$.fn')` is NULL, measured. The WHERE is never true. The
rel is permanently empty, with no refusal and no warning. The same shape appears
in the body read (`coord`), nested one level further.

Root cause, read from source: `0_relation_edge_expand.pl:45-61`
(`missing_head_target_atoms` / `head_target_atoms`) walks the head's **direct**
arguments only and never recurses; `lower.pl:341` `bind_reference_target_identity`
binds one whole body atom to `alias."__id"`; `lower.pl` `compile_pattern_arg` on a
compound argument against a `ref(...)`-typed column falls through to JSON-path
matching. Every piece is a one-level implementation.

### 3.2 The accidental guard, and why it is not a guard

The phantom `json_extract` is typed `text`. When it lands against an `int`
column the pre-existing `join_column_type_mismatch` refusal fires:

```
rel alpha(x: int).
rel pair(alpha: alpha, beta: text).
rel outer(pair: pair, tag: text).
seen(x, tag) <- outer(pair(alpha(x), _), tag).
  -> unsupported_construct(join_column_type_mismatch(
       'json_extract(b1."alpha", \'$.args[0]\')', text, 'b0."x"', int))
```

Against a `text` column it fires nothing and the program compiles to zero rows.
`path` is `text`, so the spine's most common read is exactly the silent case.
The refusal is a coincidence of column typing, not a check on ref columns.

### 3.3 Chained `decode` at depth 2: emitter right, oracle empty

The other dereference spelling inverts the divergence.

```
coord(path_text, start, end) <-
  span(f, start, end), decode(f, {path: p}), decode(p, {text: path_text}).
```

The emitter produces the correct two-hop indexed join, verified against sqlite3
with seeded spine rows (1 row returned, `src/a.rs|10|20`):

```sql
SELECT b2."text", b0."start", b0."end"
FROM "span" b0, "__ref_file" b1, "__ref_path" b2
WHERE b1."__id" = b0."file" AND b2."__id" = b1."path"
```

The oracle derives no `coord` row at all for the same program and schedule.

### 3.4 Why nothing caught this

The corpus covers depth-2 relation values through **world arrivals** only
(`struct_nested_value_renders_whole_tree`: `place(file: text, at: span)` arriving
as an `obj(...)`, read with a single top-level `decode`). Arrival ingress goes
through `normalize_relation_reference_rows/3`, a different and working path. No
fixture constructs or destructures a depth->=2 relation value in a rule.

Both divergences belong to the already-active
`task(rel_edge_clock_fixpoint, labbing, ...)` (`chat_log/20260729.4`), whose
iteration 2 is in progress and whose iterations 3 and 5 name exactly this work.
This lane hands over the receipts and does not touch `lower.pl`.

---

## 4. What the migration touches

### 4.1 Programs shedding path-beside-span columns and concat identities

`v6/dl/fixtures/flagship-flow.dl6` is the whole wart list in one file:

| current | after |
|---|---|
| `rel span(start: int, end: int).` | `rel file_span(file: rev_file, start: int, end: int).` |
| `rel df_node(path: text, at: span, kind: text).` and 5 siblings (`df_edge`, `df_param`, `df_arg`, `call_span`, `type_callable`) | `path` column deleted from each; the file rides inside `at` |
| 7 x `concat([path, ':', start, ':', end])` in `df_direct`, `flow_arg_edge`, `flow_ret_edge`, `flow_node_type` | deleted; node identity **is** the `file_span` edge, so `flow_edge(from: file_span, to: file_span)` |
| 12 x `decode(at, {start: start, end: end})` unpacking coordinates | kept only where a comparison needs the ints; the path half goes away |
| header "NAMED STOP host_struct_output_type: the current text door refuses every span-typed host output" | **stale**. `task(struct_host_output_seam, done)` (ARCH.pl:701) landed decl-B host output columns admitting declared type names. The header must be rewritten or deleted in the same commit that migrates the program. |

`v6/dl/fixtures/diag-rail.dl6`: `line`, `col`, `end_line`, `end_col` are literal
`0` in both `diag_v5` clauses, with a 12-line header explaining the honesty of
the zero. Those become real values, and the header paragraph is rewritten, once
card **B** is decided and stage 4 lands. The 9-column `diag_v5` shape itself does
not change — `src/lsp.rs:545` reads it positionally.

Comment rails (`task(comment_rail_wiring, unbuilt)`, ARCH.pl:741): the grep host
demotes to an optimization once `slice(span, from, to)` exists. The kernel-host
lab already measured that slice is ordinary arithmetic plus guards, and bounds
checking is an ordinary comparison, so slice needs **no new construct** — it is a
rule over `file_span`. Not blocked on any card; blocked on stage 3.

### 4.2 Flow-rig referee

`v6/tsv2/scripts/flagship-flow-classify.py:38-56` is the coordinate translator:
a per-corpus-file newline index, `bisect` over it, 1-based line and 0-based col,
so v6 byte spans can be compared against v5's `path:line:col:kind` keys.
`ARCH.pl:604` records the decision to translate inside the classifier rather than
bend either engine. That decision was correct **while line/col were unavailable
in-language**; card **B** decides whether it survives. If B selects option 3
(`newline` rows plus DL6 rules), those ~20 lines delete and the flow rig compares
line/col computed by the engine under test — which is a stronger receipt, and
also removes the classifier as a place where a translation bug can hide a
parity miss.

### 4.3 Bookkeeping

- `PLANS.md` decision rows: close `file-identity-span-spine.md:363` and `:370`,
  `file-span-storage-lab.md:531` and `:534`; keep `:366`; add rows for cards A
  and B. Closing means editing the `todo(decision)` comments in the source docs
  and regenerating via `dl examples/gen-plans-index.dl`.
- ARCH rows: `file_span_redesign` (unbuilt) points at this plan;
  `file_span_storage_lab` and `file_span_kernel_host_boundary_lab` (labbed) fold
  per `user_ruling(receipt_folding)`; `rel_edge_clock_fixpoint` gains the two
  section-3 receipts.
- `extractor_gap(file_blob_repo_revision)` in the closeout ledger stays a gap and
  stays untouched. Nothing in this plan asks sprefa-extract for a dl6-specific
  shape: the wire keeps bare byte ranges, and the **host boundary** pairs each
  record span with the demand's `rev_file` (the demand already binds path and
  digest). That is `locked(sprefa_extract_scope)` and
  `locked(extractor_trait_boundary)` honoured without an extractor edit.

---

## 5. Staged execution order

Stage numbering deviates from the dispatch brief on purpose: the brief's
"stage 1 = the decls expressible today" is not reachable, because section 3
proves the decls are **not** expressible today past depth 1. The defect goes
first.

### Stage 0 — bookkeeping, no code. Not blocked.
Close the four answered `todo(decision)` comments; regenerate `PLANS.md`; point
the ARCH rows at this document.
**Receipt:** `PLANS.md` decision rows for the two labs drop to card C plus new
rows A and B; `dl examples/gen-plans-index.dl --check` exit 0.

### Stage 1 — nested relation-value patterns at depth >= 2. Not blocked. Owner: `rel_edge_clock_fixpoint`.
1. Fail-first fixtures, both directions, from section 3: a depth-2 construction
   whose oracle emits a row and whose emitter emits none; a depth-2 chained
   `decode` whose emitter emits a row and whose oracle emits none.
2. Recurse `head_target_atoms/4` in `0_relation_edge_expand.pl` so a nested
   relation value contributes target membership at every level.
3. Recurse the pattern lowering so a compound argument against a `ref(...)`
   column joins the target table instead of `json_extract`-ing the endpoint.
4. Convert the accident in 3.2 into a real guard: a `ref(...)`-typed column that
   would be read by `json_extract` is a **named refusal**, regardless of the
   other operand's type. Without this, the text-column case stays silent.

**Receipts:** the two fail-first fixtures flip red -> green; `l3f.dl6` grades
oracle-identical; conformance >= 177 with the prior 175 unmoved; sweep
compiled/identical unchanged outside the new fixtures, 0 wrong; `EXPLAIN QUERY
PLAN` shows SEARCH on `__id` at every hop (per the count-test law, this path was
never quadratic but it is a formerly-silent join, so the plan receipt stands in);
`plunit`, TEXT_DOOR, roundtrip at baseline.

### Stage 2 — spine decls without revision. Blocked by stage 1 only.
`repo`, `path`, `file`, `blob`, `git_blob`, `stored_blob`, plus one analysis rel
referencing a span. Keyed rels are world-fed or `<+`-headed per 1.2.
**Receipts:** storage-lab semantic fixtures 1 ("identical bytes at two paths
produce one blob and two files") and 4 ("a blob span located through two
placements returns two file spans", degraded to the no-revision case); oracle and
emitter byte-identical; `EXPLAIN` covering SEARCH on `path.normalized_path`,
`file(repo,path)`; a bytes-per-fact number to compare against the lab's 32.24
baseline on the same census distribution.

### Stage 3 — revision, `rev_file`, `file_span`. **Blocked by card C.**
Adds `revision` plus its two variant rels, `rev_file`, `file_span`, and the
`file_span`-referencing analysis rels. Card C decides whether a watcher batch
replaces the `rev_file` row under a stable revision or mints a new revision.
**Receipts:** storage-lab semantic fixtures 2, 3, 5, 6; a retraction receipt on
`rev_file` replacement (the incremental minus-delta must match the oracle, per
the review-A4 lesson); `file_span` bounds refusal on `start > end`; no fact-row
schema contains path text, digest text, enum text, line, or column.

### Stage 4 — text, line, column through the host boundary. **Blocked by card B.**
`span_text` and either `newline` rows or `span_position` outputs, on the existing
demand/response plan with a registered typed executor, batched by blob and repo.
**Receipts:** provider process count bounded by (repo, batch), **not** by span
demand — the before-numbers are the ledger's own `host_overuse` rows
(`flagship_flow`: seven per-path subprocess declarations plus one project
resolve; `flagship_callgraph` and `diag_rail`: two per path and digest); byte-
bounded cache saturation at the measured 1,048,564 B ceiling; text/line/col
identical after the working path changes.

### Stage 5 — program migration. Blocked by stages 3 and 4, and by card A for names.
`flagship-flow.dl6` sheds path columns and all seven concat identities;
`diag-rail.dl6` gets real line numbers; comment rails get `slice`; the referee
translator deletes if card B chose option 3.
**Receipts:** `just flagship` still grades 0-unclassified against v5 with the
migrated program; `just lsp-diags` HOLDS with non-zero line numbers observed at
the real v5 LSP client (the sabotage receipt in that arc showed engine-side
checks are not discriminating — the v5 leg is); `just green-all` exit 0.

---

## 6. Ownership and what this lane did not do

- No production edit. No new syntax was invented, proposed, or needed: every
  spelling in section 1.1 parses and compiles today.
- `lower.pl`, `0_relation_edge_expand.pl`, and `structPlane.ts` are owned by the
  active `rel_edge_clock_fixpoint` lane; section 3 is a handover, not a patch.
- `v6/prolog/compile/3_clock_check*`, `compile.pl`, `serveHost.test.ts`, and the
  roundtrip fixtures were not touched (concurrent lanes).
- `v6/sprefa-extract` was not touched and this plan asks nothing of it.
- Scratch programs and databases used for section 3 live in this session's
  scratchpad and are not committed.
