# File span storage lab

## Result in one screen

Store each repeated thing once:

```text
repo 7
  path 91 = "src/parser.rs"
    file 44 = repo 7 + path 91

revision 12
  rev_file 308 = revision 12 + file 44 + blob 900

blob 900 = content digest and size
  source: Git object abc123
  source: optional stored bytes

file_span 5001 = rev_file 308 + bytes 120..148

call fact -> file_span 5001
type fact -> file_span 5001
dataflow fact -> file_span 5001
```

The path occurs in one row. The digest occurs in one row. The byte range
occurs in one row. Every analysis fact carries one small integer,
`file_span_id`.

To read the source text for span 5001:

```text
5001 -> rev_file 308 -> blob 900 -> stored bytes or Git -> bytes[120..148]
```

To find every location of the same content range:

```text
blob 900 + bytes 120..148 -> matching rev_file and file_span rows
```

Git-backed and stored content are separate facts:

```text
git_blob(blob 900, repo 7, abc123)
stored_blob(blob 900, bytes)
```

A blob may have either row or both rows. No nullable “maybe Git, maybe stored”
columns are required.

Measured result using the real extractor's span reuse:

| representation | database bytes per analysis fact |
|---|---:|
| one `file_span` row, facts reference its ID | **32.24** |
| separate content-span and located-span rows | 38.30 |
| facts reference a content-span row | 43.89 |
| every fact repeats blob/start/end | 51.95 |
| every fact repeats path/digest/enum/name text | 315.31 |

The language-facing model remains relational. Revision variants become
separate relations and union rules. Content reads use the existing
host-request/response machinery with a typed executor. Two generic language
questions remain for user review: typed relation references and typed-host
declaration spelling.

## Context

This lab selects a null-free relational representation for repositories,
revisions, logical files, content, and spans. It follows
`plans/2026-07-29-file-identity-span-spine.md` and replaces that plan's
unmeasured physical span candidates.

The constraints supplied for the lab:

- repo/revision/files and their spans remain queryable in every direction;
- paths and open names are stored once;
- closed enum spellings do not repeat as text in fact rows;
- Git content may be read later, optionally copied into storage, or both;
- non-Git content has a total representation without nullable Git columns;
- no canonical-JSON dictionary representation for file or span references;
- database size and resident memory remain measured;
- file semantics should use ordinary rels, enum/match lowering, and the
  uniform host boundary where those mechanisms suffice.

Current implementation facts:

- enum variants already expand to ordinary relations and tag rules in
  `v6/prolog/0_enum_expand.pl`;
- match arms already expand to ordinary rules in
  `v6/prolog/0_match_expand.pl`;
- `sh_decl`/`probe` already lowers a host request to demand and response rels
  in `v6/prolog/1_host_expand.pl`;
- `bind_decl` is the continuous world-source path;
- the served host plan is data-driven through `IHostPlan`, but its implemented
  executor is currently `live_sh`;
- declared structs use canonical semantic and rendered JSON dictionaries,
  which is unsuitable for high-cardinality file spans.

The executable lab is under `v6/sprefa-store/bench/file_span/`:

| file | measurement |
|---|---|
| `0_bench.py` | schema size, ingest, RSS, filters, reverse lookup, query plans |
| `1_content.py` | persistent Git batch reads, stored content, bounded caches |
| `2_census.py` | real extractor span-reference multiplicity |
| `3_paths.py` | whole-path dictionary, segment dictionary, repeated path text |

Raw results are the JSON files beside those scripts.

## Type signatures

Names below describe semantics and checked IR. They do not select surface
syntax.

```text
RepoRef      : ref(repo)
RevRef       : ref(revision)
PathRef      : ref(path)
FileRef      : ref(file)
BlobRef      : ref(blob)
RevFileRef   : ref(rev_file)
FileSpanRef  : ref(file_span)

repo          : (RepoRef, repo_name)
path          : (PathRef, normalized_path)
file          : (FileRef, RepoRef, PathRef)

committed_rev : (RevRef, RepoRef, git_oid)
work_rev      : (RevRef, RepoRef, root_ref, base: RevRef)
revision      : union(committed_rev, work_rev)

blob          : (BlobRef, digest, byte_len, line_count)
git_blob      : (BlobRef, RepoRef, git_oid)
stored_blob   : (BlobRef, bytes)

rev_file      : (RevFileRef, RevRef, FileRef, BlobRef)
file_span     : (FileSpanRef, RevFileRef, start, end)
```

`committed_rev` and `work_rev` are exclusive variants with total columns.
Their common `revision` relation is a generated union. No variant payload
column is nullable.

`git_blob` and `stored_blob` are additive capability relations. The same
`BlobRef` may occur in both. A Git-backed blob can be copied into
`stored_blob` later without changing its identity. Durable non-Git content
has a `stored_blob` row. This is set membership rather than an exclusive sum.

`file_span` directly owns its revision-qualified file parent and byte
coordinates. Content identity is reached through:

```text
file_span -> rev_file -> blob
```

Content-equivalent spans across revisions or paths are queried by
`(blob,start,end)` through that join. There is no `blob_span` table in the
selected representation.

## Instance timelines and lifetimes

```text
RepoRef
  corpus lifetime

PathRef
  dictionary lifetime; one normalized whole path string

FileRef = (RepoRef,PathRef)
  logical file lifetime across revisions

RevRef
  committed_rev: immutable
  work_rev: observation lifetime, with a total base RevRef

BlobRef
  immutable content lifetime, independent of source capability

git_blob(BlobRef,...)
  present while Git is a known retrieval source

stored_blob(BlobRef,...)
  present while bytes are retained in the content store

RevFileRef = (RevRef,FileRef)
  one file occurrence at one revision

FileSpanRef = (RevFileRef,start,end)
  one located range within that immutable occurrence
```

Adding or removing a content-source capability row does not rewrite
`BlobRef`, `RevFileRef`, `FileSpanRef`, or any fact referencing them.

## Storage, reads, writes, and uniqueness

Selected physical tables:

```sql
repo(
  repo_id INTEGER PRIMARY KEY,
  name_id INTEGER NOT NULL
)

path(
  path_id INTEGER PRIMARY KEY,
  normalized_path TEXT NOT NULL UNIQUE
)

file(
  file_id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  UNIQUE(repo_id,path_id)
)

committed_rev(
  rev_id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  git_oid BLOB NOT NULL,
  UNIQUE(repo_id,git_oid)
)

work_rev(
  rev_id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  root_id INTEGER NOT NULL,
  base_rev_id INTEGER NOT NULL,
  UNIQUE(root_id)
)

blob(
  blob_id INTEGER PRIMARY KEY,
  digest BLOB NOT NULL UNIQUE,
  byte_len INTEGER NOT NULL,
  line_count INTEGER NOT NULL
)

git_blob(
  blob_id INTEGER NOT NULL,
  repo_id INTEGER NOT NULL,
  git_oid BLOB NOT NULL,
  PRIMARY KEY(blob_id,repo_id)
) WITHOUT ROWID

stored_blob(
  blob_id INTEGER PRIMARY KEY,
  content BLOB NOT NULL
)

rev_file(
  rev_file_id INTEGER PRIMARY KEY,
  rev_id INTEGER NOT NULL,
  file_id INTEGER NOT NULL,
  blob_id INTEGER NOT NULL,
  UNIQUE(rev_id,file_id)
)

file_span(
  file_span_id INTEGER PRIMARY KEY,
  rev_file_id INTEGER NOT NULL,
  start INTEGER NOT NULL,
  end INTEGER NOT NULL,
  UNIQUE(rev_file_id,start,end)
)
```

Fact tables reference `file_span_id`. Closed families and kinds use stable
integer ordinals. Open names use dense dictionary IDs.

Write sequence for one file observation:

1. normalize the whole repo-relative path and resolve `path_id`;
2. resolve `file_id` by `(repo_id,path_id)`;
3. resolve `blob_id` by raw digest bytes;
4. assert one or more content capability rows;
5. resolve `rev_file_id` by `(rev_id,file_id)`;
6. deduplicate `(start,end)` coordinates within the extraction batch;
7. batch insert `file_span(rev_file_id,start,end)`;
8. batch resolve `file_span_id` values;
9. insert family facts with numeric closed vocabulary and dictionary-backed
   open names;
10. commit the observation as one transaction.

Read paths:

```text
repo + revision + path prefix
  path -> file -> rev_file -> file_span -> facts

content-equivalent span placements
  rev_file(blob_id) -> file_span(start,end)

span bytes
  file_span -> rev_file -> blob -> git_blob or stored_blob

logical file history
  file -> rev_file -> committed_rev/work_rev -> blob
```

The measured query plans for the selected cell use covering index searches on
path, file, rev-file, file-span, facts, and reverse blob lookup.

## Measurements

### Real extractor multiplicity

`2_census.py` ran the release extractor over 1,048 tracked source files:

| measure | value |
|---|---:|
| distinct spans | 2,073,233 |
| span references | 7,345,805 |
| references per span | 3.543 |
| spans with at least 2 references | 2,053,038 |
| spans with at least 3 references | 1,275,701 |
| records | 4,931,005 |
| failed files | 0 |

99.0% of distinct spans have at least two references. 61.5% have at least
three.

### Synthetic reuse thresholds

All schema cells used the same deterministic repo/revision/file/blob data.
Each row is stable across two repeats.

| references per located span | selected smallest | bytes/fact | next |
|---:|---|---:|---:|
| 1 | embedded `(blob,start,end)` | 69.19 | content-span ref 70.76 |
| 2 | direct `file_span(rev_file,start,end)` | 44.80 | content-span ref 51.00 |
| 3 | direct `file_span(rev_file,start,end)` | 34.99 | two-level located ref 42.10 |

The direct `file_span` entity becomes the smallest representation at two
references. The extractor census measured at least two references for 99.0%
of distinct spans.

### Real multiplicity replay

`0_bench.py --census ...` replayed the real multiplicity distribution, capped
at 32 references, over 405,696 fact rows:

| representation | bytes/fact | ingest ms, two runs | peak RSS MB |
|---|---:|---:|---:|
| selected `file_span(rev_file,start,end)` | **32.24** | **517.6 / 530.5** | 77.9 / 78.5 |
| two-level `file_span(rev_file,blob_span)` | 38.30 | 585.1 / 610.9 | **71.4 / 71.5** |
| content-span ref | 43.89 | 587.0 / 588.5 | 72.5 / 72.2 |
| embedded coordinates per fact | 51.95 | 605.6 / 621.4 | 87.1 / 86.7 |
| repeated text baseline | 315.31 | 1,980.0 / 2,089.1 | 98.2 / 98.7 |

The selected direct located-span table is:

- 15.8% smaller than the two-level located/content-span model;
- 26.5% smaller than content-span references;
- 37.9% smaller than embedded coordinates;
- 89.8% smaller than repeated text.

Filter and reverse-placement query plans contain searches only:

```text
SEARCH path USING covering normalized_path index
SEARCH file USING covering (repo_id,path_id)
SEARCH rev_file USING covering (rev_id,file_id)
SEARCH file_span USING covering (rev_file_id,start,end)
SEARCH fact USING primary key (file_span_id,...)

SEARCH rev_file USING covering blob_id index
SEARCH file_span USING covering (rev_file_id,start,end)
```

### Content retrieval

`1_content.py` measured 300 tracked blobs containing 8,498,015 bytes. Each
source was read three times:

| source | run 1 | run 2 | storage |
|---|---:|---:|---:|
| persistent `git cat-file --batch` | 58.88 ms | 58.25 ms | Git object database |
| SQLite `stored_blob` rowid searches | 12.85 ms | 12.88 ms | 8,667,136 bytes |

Git batch retrieval averaged about 19.5 ms per 300-blob pass. SQLite-stored
content averaged about 4.3 ms. The two capability relations permit either or
both without nullable source columns.

A 1 MiB content cache peaked at 1,048,564 bytes and retained 153 of the 300
blobs. Newline indexes for all 300 blobs used 212,892 bytes. Content caching
can therefore remain byte-bounded while newline indexes remain a separate
small cache.

### Paths

`3_paths.py` measured all 2,996 tracked paths with 20 fact references each:

| representation | database bytes | bytes/reference | prefix filter ms |
|---|---:|---:|---:|
| whole normalized path dictionary | **954,368** | **15.93** | **0.0385 / 0.0387** |
| segment dictionary plus junction | 1,146,880 | 19.14 | 0.0573 / 0.0588 |
| repeated path text | 3,448,832 | 57.56 | 1.022 / 1.023 |

Whole-path interning is 16.8% smaller than segment normalization and uses one
covering range search for prefix filtering. Segment normalization adds rows
and a join. Repeated text is 3.61 times the whole-path database size for this
cell.

## Host and kernel boundary

### Ordinary relational portion

Repo, revision variants, paths, files, blobs, source capabilities, rev-files,
and file-spans are ordinary relations. Enum expansion supplies the exclusive
revision variants. Ordinary union rules supply common revision queries.
Match reads variant relations.

The generated enum tag relation currently stores its tag as text. Physical
closed tags should use declaration-order integer ordinals. Match lowering can
read variant relations directly, so a tag row need only exist when a program
queries the tag projection.

### Generic kernel requirement

The type plane needs a relation-reference column type:

```text
ref(Relation)
```

Its physical value is one dense integer. Its static type prevents a
`FileSpanRef` from being passed where `BlobRef` or `FileRef` is expected. This
is generic relational semantics and does not add file-specific syntax to the
kernel.

The existing generic struct dictionary is not used for relation references.
Reference values render through the referenced relation only at a requested
boundary.

### Host bindings

Continuous discovery belongs on `bind_decl`:

```text
watch(...)       -> arrivals and retractions
enumerate(...)   -> repo/revision/file observations
```

Demanded content operations belong on the existing host-plan demand/response
path with a typed non-shell executor:

```text
span_text(FileSpanRef)     -> text
span_position(FileSpanRef) -> start_line,start_col,end_line,end_col
span_slice(FileSpanRef,relative_start,relative_end) -> FileSpanRef
```

The executor:

1. joins `file_span -> rev_file -> blob`;
2. selects `stored_blob` when present or `git_blob` otherwise;
3. reads one blob through a persistent batch source;
4. uses byte-bounded content and newline-index caches;
5. returns ordinary response rows that participate in subscription lifetime
   and retraction.

`span_slice` validates bounds and interns the child
`(rev_file_id,start,end)` through the same batched store adapter. No stored
text or line/column columns are introduced.

The host plan already carries an `execution` field. Adding a registered typed
executor reuses its demand, witness, response, cache, and EDB arrival
machinery. File operations therefore require a blessed storage model and
executor registration, not file-specific evaluation syntax.

## Decisions

Selected by the lab:

1. Logical file identity is `(repo_id,path_id)`.
2. Paths are interned as complete normalized strings.
3. Revisions use total per-variant relations with ordinary union rules.
4. Blobs have one content identity and additive source-capability relations.
5. `file_span` is physically `(file_span_id,rev_file_id,start,end)`.
6. Facts reference one dense `file_span_id`.
7. There is no separate `blob_span` table.
8. Content-equivalent spans are queried through `rev_file.blob_id`.
9. Closed enum tags use numeric ordinals when materialized.
10. Open names use dense dictionary IDs.
11. File and span IDs use a generic relation-reference type rather than
    generic canonical-JSON structs.
12. Text and positions use typed executors on the existing host-plan
    demand/response path.
13. Git and stored content remain independently assertable capabilities.
14. Content and newline caches have explicit byte budgets.

Rejected by measurement:

- repeated path, digest, enum, and name text in fact rows;
- path-segment normalization for the measured filtering and storage workload;
- embedded span coordinates in every fact;
- a content-span dictionary plus a second located-span junction;
- nullable `git_oid` and `stored_content` columns on one blob row;
- persisted source slices, line numbers, or columns;
- canonical JSON as the physical file-span value;
- a file-specific expression or function-call surface.

## User decision cards

### Card 1: generic relation references

| choice | result |
|---|---|
| add a generic relation-reference type | dense integer storage with static `FileRef`/`BlobRef`/`FileSpanRef` separation |
| keep every reference as `int` | same physical bytes, no static separation |
| wrap references in declared structs | canonical JSON dictionary amplification, rejected by this lab's constraint |

Recommendation: generic relation-reference type. Surface spelling remains a
separate card before grammar work.

### Card 2: typed host declaration surface

| choice | result |
|---|---|
| general host declaration with registered executor | reuses current host-plan lowering for shell and typed executors |
| keep `sh` authoring and inject built-ins internally | no new surface now, but user programs cannot declare typed executors |
| blessed file operations only | smallest immediate surface, file-specific registry |

Recommendation: keep the internal plan generic now, register the file
executors, and withhold authoring spelling until the host declaration card is
presented.

<!-- todo(decision): Choose the surface spelling for generic relation-reference
columns after accepting or rejecting the generic ref(Relation) semantics. -->

<!-- todo(decision): Choose the authoring spelling for registered typed host
executors; implementation may reuse IHostPlan internally without exposing new
syntax first. -->

## Verification

Lab verification completed:

- every schema cell ran twice in a fresh process;
- real extractor census completed over 1,048 files with 0 failures;
- content cells ran twice;
- path cells ran twice;
- selected filter and reverse queries use covering index searches;
- all payload tables in selected revision and content-source variants have
  total columns;
- result JSON files retain raw counts, timings, RSS, dbstat, and query plans.

Implementation verification must add:

1. schema anti-join checks for orphan relation references;
2. bounds checks for every file span;
3. committed/work union fixtures;
4. Git-only, stored-only, and Git-plus-stored blob fixtures;
5. identical content at two paths and two revisions;
6. file-span filters by repo, revision, path, blob, and coordinate;
7. byte-bounded cache saturation;
8. enum ordinal stability across schema versions;
9. no raw path, digest, enum tag, source text, line, column, or canonical
   file-span JSON in fact rows;
10. database bytes/fact and RSS rails using the lab result format.

## Staffing

This lab was run locally on main without subagents. No source behavior,
grammar, or shipping schema was changed.

Implementation should split after the two user cards:

1. storage schema and batched relation-reference resolution;
2. generic reference typing in the type plane and checked IR;
3. extractor batch envelope and numeric closed vocabulary;
4. typed host executor registration for content and positions;
5. program migrations removing sibling path/span columns and concatenated
   coordinate identities.

Each implementation lane receives exact file ownership and a base SHA.
Grammar work remains blocked until its decision card is accepted.
