# Call-family owner-scoped delta refresh

## Context

The release micro-probe now gives a trustworthy scaling shape for the resolved
call graph. A one-file edit stays on the incremental path, parses one file, and
matches a fresh rebuild exactly, but grows from 14 ms at 10 files to 73 ms at
1,000 files. The baseline and probe are recorded in
[reproducible reactivity evidence](2026-07-15-reproducible-reactivity-evidence.md#scrappy-call-graph-micro-baseline-release-two-workers).

The source-resident self-map `.dl/call-refresh-map.dl` successfully mapped the
current slice with the release `dl` binary. Its bounded corpus contained 22
files, 243 functions, 3,562 raw call sites, and 332 resolved edges. It exposed
the operational chain:

```text
refresh_call_rels
  ├─ extract_file_set → moved_extract_revs → cached_facts
  ├─ scip_name_defs / scip_occ_index
  ├─ module_import_map / module_binding_resolved_map
  ├─ rebuild corpus-global by_name / sym_at / sym_file / def_by_file
  ├─ resolve every definition and site
  ├─ refresh_rel_for_revs(call_def_rev, call_edge_rev)
  ├─ refresh_rel(call_site, call_name, call_kind)
  └─ rebuild_legacy_call_rels
       └─ DELETE + INSERT call_def and call_edge
```

Physical parsing is already cached correctly. The remaining edit cost comes
from rebuilding global resolver indexes, revisiting every site, replacing the
whole call-family tables, and recreating both legacy projections
([call refresh](../src/engine/extract/call.rs#L13)).

The public set relations cannot simply delete by changed file. `call_edge_rev`
does not retain the producing site file; `call_name` and `call_kind` do not
retain file or revision; the legacy `call_def` and `call_edge` collapse rows
across revisions. A public row may therefore have several producers. Correct
incrementality requires internal ownership and last-owner retraction.

## Decisions

1. **Delta one family first.** Call extraction is the measured vertical slice.
   Type, dataflow, module, and generic relation refresh stay unchanged.
2. **SQLite owns durable incremental state.** Add internal owner tables and
   indexes; do not create a custom mmap store or keep the corpus graph resident
   in Rust collections.
3. **Definitions and sites are the canonical owned inputs.** A file/revision
   owner replaces only its raw definition and site rows. Resolved/public rows
   are projections of those owned inputs.
4. **Resolved edges retain site ownership internally.** Public call relations
   remain sets. A row retracts only after its last internal owner disappears.
5. **Affected names are the global invalidation unit.** A definition bucket
   change re-resolves sites that call that name; an ordinary body edit resolves
   only sites in the edited file.
6. **Use flat TEMP-key joins and bounded chunks.** No nested correlated-query
   pipeline and no corpus-sized Rust queue. SQLite TEMP tables hold affected
   keys and may spill; Rust processes a bounded page of owners at a time.
7. **Keep a loud full-family fallback.** Slice 1 supports body/site edits in a
   WORK file only when definitions, imports, modules, and SCIP inputs are
   unchanged and the callee bucket is uniquely resolved. Definition changes,
   add/delete/rename, Git-revision changes, SCIP index changes, repository
   topology changes, ambiguity, and module-wide invalidation stay on the
   existing full refresh with an attributed reason until their reverse
   dependency rails exist.
8. **Do not generalize prematurely.** The call-specific delta API may inform a
   later `ExtractFamily` refresh protocol only after its scaling and ownership
   semantics are measured.
9. **Use compact surrogate identity throughout physical storage.** Entity and
   occurrence tables use SQLite `INTEGER PRIMARY KEY` surrogates. Repeated
   repo/rev/path/symbol/name/kind/callee values use the existing 64-bit
   `_strings.id` representation, never duplicated TEXT. Pure junction tables
   use composite integer endpoint keys rather than paying for a redundant row
   surrogate plus a second uniqueness index.

Rejected alternatives:

- **Delete public rows by file:** incorrect for relations that omit file/rev
  and for duplicate edges with several producing sites.
- **Rebuild global HashMaps from cached facts:** saves parsing but preserves the
  observed corpus-scaled edit cost and resident-memory pressure.
- **Adopt the test-only dataflow owner schema wholesale:** its ideas are useful,
  but its fact shape and ownership keys are specific to `df_node`.
- **Replace SQLite or add mmap:** unrelated to the measured invalidation bug and
  would erase the current storage seam.
- **Generalize every extractor now:** too large to attribute or roll back.

## Data model

<!-- todo(perf): add durable call definition/site/edge ownership tables and indexes without changing the public call relation schemas -->

<!-- todo(perf): enforce the call storage-key invariant with schema and dbstat rails: integer surrogates for entities/occurrences, StringId integers for repeated identities, and no raw TEXT in owner/provenance/TEMP hot tables -->

### Physical key invariant

The logical names below describe meaning; the physical columns are compact
integers. The existing public call tables already satisfy the string side of
this contract: non-raw `text` and `path` columns are declared interned by
`Col::interned`, encoded through `SymSink`, and stored as SQLite INTEGER values
that resolve through `_strings` ([spine](../src/spine.rs#L43),
[call declarations](../src/engine/decls.rs#L543)). The 10-file probe database
confirmed INTEGER storage for every identity column in `call_def_rev`,
`call_site`, `call_edge_rev`, `call_name`, and `call_kind`.

```text
entity / occurrence identity:
  owner_id, def_id, site_id, resolution_id  INTEGER PRIMARY KEY

repeated semantic identity:
  repo_sid, rev_sid, path_sid, sym_sid,
  name_sid, kind_sid, callee_sid             INTEGER (_strings.id)

small values:
  line, end, site_ordinal, support_count     INTEGER

bulk bytes:
  source text                                outside these tables
```

Every identity-bearing table has one narrow surrogate primary key and a
separate semantic uniqueness rail. The surrogate is local storage identity;
the semantic key remains the replay/correctness identity:

```text
_call_owner       UNIQUE(repo_sid, rev_sid, path_sid)
_call_def_owner   UNIQUE(owner_id, def_ordinal)
_call_site_owner  UNIQUE(owner_id, site_ordinal)
_call_resolution  UNIQUE(site_id, caller_sid, callee_sid, kind_sid)
```

Pure support/junction tables whose row is nothing beyond its endpoints use
`PRIMARY KEY(integer_endpoint_a, integer_endpoint_b)` and `WITHOUT ROWID` when
measurement shows it is smaller. Adding `support_id` there would retain the
composite unique index and add another B-tree, so it is explicitly not called
an optimization.

This rule covers every new call-family relation:

| relation shape | physical identity |
| --- | --- |
| reusable entity or occurrence | `INTEGER PRIMARY KEY` surrogate plus semantic `UNIQUE` rail |
| pure edge, support, or junction | composite integer primary key; no redundant surrogate |
| bounded affected-set/TEMP relation | compact integer key, normally `WITHOUT ROWID` |
| public set-valued relation | existing semantic tuple key; a row ID would not replace tuple dedup |
| bulk payload | raw `TEXT`/BLOB outside owner, provenance, and affected-set hot keys |

“Proper surrogate keys” therefore does not mean adding `row_id` to every set
table. That would keep the tuple uniqueness index and add another B-tree.

TEMP affected-set tables copy the same integer types. Text enters at the
extractor boundary, is interned once in a batch, and every resolver/write join
afterward compares fixed-width integers. `_strings` collision checking remains
the integrity boundary; no open-coded decimal/string conversion is allowed.

The exact names may change during migration, but the logical schema is:

```text
_call_owner
  owner_id, repo_sid, rev_sid, path_sid, content_digest, committed_generation

_call_def_owner
  def_id, owner_id, def_ordinal, sym_sid, name_sid, kind_sid, line, end
  indexes: owner_id; (repo_sid, rev_sid, name_sid); owner-local sym

_call_site_owner
  site_id, owner_id, site_ordinal, callee_sid, line
  indexes: owner_id; (repo_sid, rev_sid, callee_sid)

_call_site_resolved
  resolution_id, site_id, caller_sid, callee_sym_sid, edge_kind_sid, call_kind_sid
  indexes: site_id; public integer edge key; public integer kind key
```

`site_ordinal` is only identity within one file/revision owner generation. A
changed owner is replaced atomically, so it need not survive arbitrary edits.
The public relations remain the documented API:

```text
call_def_rev, call_site, call_edge_rev, call_name, call_kind
call_def, call_edge
```

Each public row is a `DISTINCT` projection of one or more owner rows. A touched
public key is deleted and reinserted from the remaining owners in the same
transaction. This preserves a duplicate edge/name/kind until its final owner is
gone without maintaining a second Rust-side reference count.

The current source-stage design is reused as a transaction pattern—prepare,
seal, verify base generation, apply, commit—not as the call schema itself
([source stage](../src/engine/pipeline/source_stage.rs#L360),
[path apply](../src/engine/path_reconcile.rs#L182)).

## Incremental algorithm

### Current

```text
changed file
    │
    ▼
parse 1 file ── good
    │
    ▼
load cached facts for N files
    │
    ▼
build all resolver maps + resolve all sites
    │
    ▼
replace all call tables + legacy tables
```

### Target

```text
changed owner(s)
    │
    ├─ read old def-name buckets + old public keys
    ├─ parse changed files outside the write transaction
    └─ stage new defs/sites, including an explicitly empty owner
             │
             ▼
      _call_changed_name       old names ∪ new names
      _call_impacted_site      changed owners
                             ∪ sites whose callee is a changed name
             │
             ▼
      replace raw rows for changed owners
      re-resolve only _call_impacted_site in bounded pages
             │
             ▼
      refresh only touched public keys from remaining owners
             │
             ▼
      commit owner generation + digests atomically
```

The affected-set rules are:

- **Body-only edit:** every site in the edited file. Definition buckets that
  remain byte-identical do not fan out.
- **Definition add/delete/rename:** every site in the same `(repo, rev)` whose
  bare callee is in the old-or-new name buckets, plus every site in the edited
  file. This covers unique↔ambiguous transitions.
- **Definition span change:** every site in the edited file, because caller
  containment may move even when names do not.
- **File delete:** stage an empty completed owner, retract its raw rows, then
  process its old definition buckets and old resolved public keys.
- **Rename:** old-path empty owner plus new-path owner in one generation. Public
  rows never expose the half-renamed state.
- **Duplicate resolved edge:** remove the changed site's edge owner; keep the
  public edge when another site owner still projects the same key.
- **Import/alias edit in a source file:** re-resolve all sites in that file
  after module bindings commit for the same tick.
- **Ambiguous-name narrowing:** any changed definition bucket re-resolves all
  sites for that name, because `sym_file` and import narrowing may select a new
  candidate.

### Backpressure and memory

`_call_impacted_site` is a keyed SQLite TEMP table, not a `Vec` of the corpus.
Resolution reads it in deterministic pages (initially 512 sites), writes staged
resolved rows in the same bounded batches, and releases each batch before the
next. The maximum resident queue is therefore one extraction batch plus one
resolution page. The database page cache retains its existing configured
budget; no mmap or custom allocator is introduced.

## Fallback boundary

<!-- todo(perf): route WORK call-family path deltas through the owner-scoped refresh and report every unsupported widening reason -->

Slice 1 takes the delta path only for raw site/body changes in literal WORK
files from one repository when the definition set, imports, module bindings,
and SCIP inputs are unchanged; each affected callee bucket must be unique and
require no narrowing. The owner/provenance generation and extractor schema
must also be complete and current. It takes the current full-family path with
a stable reason for:

```text
non-WORK or dynamic revision set
definition add/delete/rename or file rename/delete
ambiguous callee bucket or import/module narrowing
index.scip / SCIP occurrence data changed
repository registration/root topology changed
manifest change or module_full_work
module binding change whose affected importer files are unavailable
call owner schema/version mismatch or incomplete generation
program/extractor digest change
```

The fallback is correctness behavior, not an invisible optimization choice.
The micro-probe uses `PathTickFallbackPolicy::Forbid`, so a supposedly supported
edit cannot be benchmarked after widening.

## Sequence

1. **Attribute the 73 ms edit.** Add temporary phase counters around cached-fact
   access, index construction, site resolution, each canonical write, and
   legacy projection. Re-run the existing release probe; do not add a new
   harness.
2. **Land owner schema and full-refresh dual write.** Full refresh populates both
   the existing public tables and new internal owner tables. Compare public
   output before enabling delta reads.
3. **Implement changed-owner staging.** Stage replacements atomically,
   including explicitly empty owners and generation verification, but enable
   only body/site edits with unchanged definitions in slice 1.
4. **Implement affected-name/site resolution.** Use TEMP key tables and bounded
   pages; record exact owner/site/public-key counts.
5. **Replace legacy whole-table rebuilds.** Refresh only touched `call_def` and
   `call_edge` keys from their rev-aware/owner projections.
6. **Enable the conservative slice behind one call-family switch.** Unsupported
   scopes remain loud fallbacks. Keep the old full path as rollback until all
   gates pass.
7. **Add definition dependency slices deliberately.** Only after reverse
   bucket/import/module provenance is permanent, enable definition add/delete,
   rename, ambiguity transitions, and module-aware narrowing one class at a
   time.
8. **Re-run 10/100/1,000 and then stop.** Walk through output before considering
   type/dataflow adoption or a generic family protocol.

## Verification

Permanent suite rails:

- Supported one-file path reports `Incremental`; forbidden widening does not
  execute a full tick.
- Slice-1 body/site edits with unchanged definitions equal a fresh rebuild.
- Before slice 2 is enabled, definition add/delete/rename and span-only changes
  equal a fresh rebuild while reporting their expected full-fallback reason.
- Unique→ambiguous and ambiguous→unique name transitions update every affected
  caller.
- Removing one of two sites for the same public edge preserves that edge;
  removing the final site retracts it.
- PRAGMA/schema inspection finds no TEXT affinity in call owner, resolution,
  affected-set, or support hot tables; repeated identities are `_strings.id`
  INTEGERs and entity/occurrence tables use integer surrogate primary keys.
- `dbstat` records bytes/row and index bytes for every new table. A surrogate
  layout is rejected when it is larger than the measured composite-key layout
  without buying a required lookup or ownership property.
- Two revisions/files owning the same public name/kind/site row preserve it
  until the final owner disappears.
- An explicitly empty owner retracts old rows and commits successfully.
- Failure during staged apply rolls back owner rows, public rows, generation,
  and digests together.
- SCIP, manifest, module-wide, and non-WORK changes report their exact full
  fallback reason.

Measured gates, using the existing release probe with two workers:

```text
incremental call graph = clean rebuild call graph
files parsed = 1
fallback count = 0 for supported WORK body/site edits
fallback reason = expected for every unsupported definition/rename/ambiguity case
unrelated public rows written = 0
resident resolution page <= 512 sites
10→1000 one-file edit work-count growth <= 20%
10→1000 one-file edit wall-time target <= 2x initially, then <= 20%
```

Tests are regression rails. The release measurements and exact work counters
are the performance evidence.

Permitted commands remain bounded:

```text
CARGO_BUILD_JOBS=2 DL_RAYON_THREADS=2 cargo test --test it <named-call-delta-test>
CARGO_BUILD_JOBS=2 DL_RAYON_THREADS=2 cargo build --release --example reactivity_probe
just perf-reactivity
```

No production workspace, external repository, daemon, dataflow-heavy map, or
Linux-kernel run is part of this arc.

## Staffing

- Base SHA: `2b10fbd6159b786ef008a6e3d48698821dd44c4b` plus the current dirty
  call-probe/fallback worktree; no worktrees because agents share this workspace.
- Root owns schema choice, affected-set semantics, integration, measurement,
  and stopping-point walkthroughs.
- A bounded implementation worker may own only the owner-schema/full-refresh
  dual-write slice after root fixes exact table names and migrations.
- A bounded worker may own only the permanent body-edit/fallback/duplicate-edge
  rails in a separate test file; definition/delete/rename delta rails wait for
  slice 2.
- A harder review worker audits ambiguity/import/SCIP correctness and rollback
  before the delta switch is enabled.
- No two workers edit `src/engine/extract/call.rs` concurrently.
- Build/test concurrency and Rayon remain capped at two; named rails must finish
  within two minutes and the generated release probe within five minutes.
- Formatting runs once immediately before commit; formatter churn is not a
  review task.
