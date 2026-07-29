# File identity and span spine

> Physical storage candidates in this document were measured by
> `plans/2026-07-29-file-span-storage-lab.md`. The selected physical row is
> `file_span(file_span_id, rev_file_id, start, end)`. There is no separate
> `blob_span` table. The identity and lifetime analysis below remains the input
> to that lab; its unmeasured Candidate A/B choice is superseded.

## Context

The current language and storage layers use `file` for multiple identities:

- `sprefa-extract::BlobHash` identifies content, while its local `Span` contains
  only byte coordinates (`v6/sprefa-extract/src/types.rs`).
- `sprefa-store.files` is a content table keyed by `content_hash`.
- `sprefa-store.revs_files` places content at `(rev_id, path_string_id)`.
- the July 19 table plan used `file` for `(repo_id, path_id)` and `blob` for
  content (`v6/plans/2026-07-19-v6-table-design.md`).
- the language currently interns declared structs through generic dictionary
  rows. A `file_span` represented as canonical JSON there would duplicate its
  parent and coordinates.

The extractor wire also repeats symbolic record, family, kind, path, and name
values in JSONL. `FlatFact` uses literal strings for `record`, `family`, and
`kind`, while most spans are emitted without a blob or file parent
(`v6/sprefa-extract/src/types.rs:1242-1347`).

The required query surface covers:

1. content and every byte span within that content;
2. a repository-relative logical file across revisions;
3. the content observed for that file at one revision;
4. every span located at a repo, revision, and path;
5. text, line, column, and child slicing without stored text or stored
   line/column values.

The storage target is one copy of each repo, revision, path, digest, name, enum
ordinal, and content coordinate, subject to measured table and index costs.

## Proposed type signatures

These names describe semantic identities. They do not choose declaration
syntax.

```text
RepoId        : scalar reference
RevId         : scalar reference
PathId        : scalar reference
FileId        : scalar reference
BlobId        : scalar reference
RevFileId     : scalar reference
BlobSpanId    : scalar reference

repo          : (RepoId, repo_name, remote?)
rev           : (RevId, RepoId, revision_kind, revision_key?, base_rev?)
path          : (PathId, normalized_path)
file          : (FileId, RepoId, PathId)
blob          : (BlobId, digest, byte_len, line_count)
rev_file      : (RevFileId, RevId, FileId, BlobId)
blob_span     : (BlobSpanId, BlobId, start, end)

file_span     : (RevFileId, BlobSpanId)
```

`file_span` is a query result or checked value assembled from `rev_file` and
`blob_span`, with the invariant:

```text
rev_file(RevFileId, _, _, BlobId)
blob_span(BlobSpanId, BlobId, Start, End)
0 <= Start <= End <= blob.byte_len
```

`FileId` names the logical `(repo,path)` file across time. `BlobId` names
bytes independently of repo, revision, and path. `RevFileId` names one file
occurrence at one revision. `BlobSpanId` names a range within immutable
content.

A span over `FileId` alone has no byte identity because the file may contain
different blobs at different revisions. Queries over a logical file therefore
return revision-qualified `file_span` values.

## Instance timelines and lifetimes

```text
RepoId
  lives while the corpus knows the repository

FileId = (RepoId, PathId)
  lives across revisions and path observations

RevId
  committed: immutable
  work: one mutable observation identity with a base committed revision

BlobId
  immutable and globally content-addressed

RevFileId = (RevId, FileId) -> BlobId
  committed: immutable
  work: replaced when the observed content changes

BlobSpanId = (BlobId, start, end)
  immutable and reusable across every placement of the same blob

file_span = (RevFileId, BlobSpanId)
  exists while that revision-to-file placement exists
```

An update to a worktree file changes the `RevFileId -> BlobId` placement. Its
old `BlobSpanId` rows remain content-correct and may be retained or collected
independently. No span is rewritten to point at new bytes.

## Storage model

### Dimensions

```sql
repo(
  repo_id INTEGER PRIMARY KEY,
  ...
)

path(
  path_id INTEGER PRIMARY KEY,
  normalized_path TEXT NOT NULL UNIQUE
)

file(
  file_id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  UNIQUE(repo_id, path_id)
)

rev(
  rev_id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  ...
)

blob(
  blob_id INTEGER PRIMARY KEY,
  digest BLOB NOT NULL UNIQUE,
  byte_len INTEGER NOT NULL,
  line_count INTEGER NOT NULL
)

rev_file(
  rev_file_id INTEGER PRIMARY KEY,
  rev_id INTEGER NOT NULL,
  file_id INTEGER NOT NULL,
  blob_id INTEGER NOT NULL,
  UNIQUE(rev_id, file_id)
)
```

`rev_file_id` exists because located facts need one compact parent reference.
Without it, every located fact repeats both `rev_id` and `file_id`.

### Content spans

Candidate A:

```sql
blob_span(
  blob_span_id INTEGER PRIMARY KEY,
  blob_id INTEGER NOT NULL,
  start INTEGER NOT NULL,
  end INTEGER NOT NULL,
  UNIQUE(blob_id, start, end)
)
```

Facts reference `blob_span_id`. Location is recovered through the fact's
`rev_file_id`, or through the active query's revision and file relation.

Candidate B embeds `(blob_id,start,end)` in each fact table and creates no
span dictionary. It removes one join and adds repeated coordinates.

Candidate C stores `(rev_file_id,start,end)` in each fact. It repeats
coordinates and prevents content-span reuse across identical blobs at
different paths or revisions.

Candidate A and B require a measured comparison before selection. The
measurement must include:

- distinct blob spans divided by total span references;
- references per distinct span across CST, type, call, dataflow, comments,
  diagnostics, and refactoring facts;
- `blob_span` table plus indexes in bytes;
- fact-table bytes under embedded coordinates;
- read statement counts and query plans for span-to-text and
  repo/rev/file-to-spans queries.

`file_span` has no mandatory table. It is the checked pair
`(rev_file_id,blob_span_id)`. A materialized junction or surrogate
`file_span_id` is permitted only after a caller needs to reference the same
located pair enough times to pay for its table and indexes.

## Query surface

The following relations can be supplied as ordinary queryable rels:

```text
files_in_repo(RepoId)                   -> FileId
revisions_of_file(FileId)               -> RevId
file_at(RevId, FileId)                  -> RevFileId, BlobId
spans_in_blob(BlobId)                   -> BlobSpanId, start, end
spans_in_file_at(RevFileId)             -> BlobSpanId, start, end
locate(RevFileId, BlobSpanId)           -> file_span
same_content(FileId, RevId, RevId)      -> bool
placements_of_blob(BlobId)              -> RevFileId
```

Pure span operations:

```text
slice(BlobSpanId, relative_start, relative_end) -> BlobSpanId
contains(BlobSpanId, BlobSpanId)                -> bool
overlaps(BlobSpanId, BlobSpanId)                -> bool
```

World-backed derivations:

```text
text(BlobSpanId) -> text
line(BlobSpanId) -> int
col(BlobSpanId)  -> int
```

`text`, `line`, and `col` read immutable bytes by `BlobId`. A per-blob newline
index is cached by the world host. These values remain query results and do
not become persisted columns.

## Userspace and kernel boundary

The language kernel needs:

- scalar reference column types;
- relations and joins;
- checked construction of compound values;
- host or bind calls with declared input and output types;
- dense dictionary support without exposing storage IDs as semantic values.

The kernel does not need Git, repository, file, path, or span syntax.

The standard file library owns:

- `repo`, `rev`, `path`, `file`, `blob`, `rev_file`, and `blob_span`
  declarations;
- the `file_span` checked value or view;
- query rules joining these relations;
- pure `slice`, `contains`, and `overlaps` operations;
- world-backed `text`, `line`, and `col` bindings.

Storage may recognize reference columns and execute these relations through
indexed tables. This is an execution path for declared rels rather than a
second authoring surface.

Generic struct dictionaries must not encode `file_span` as repeated canonical
JSON. The standard file library either keeps `file_span` as two scalar refs or
uses a storage-backed value codec whose physical representation is those two
refs.

## Extractor and JSONL boundary

One extraction batch has a header:

```text
batch(repo_id, rev_id, file_id, rev_file_id, blob_id, schema_version)
```

Every fact in that batch inherits the header. Paths, digests, repo, revision,
and file identity do not repeat per fact.

Compile-time families, record variants, and kind enums use stable integer
ordinals on the store-facing wire:

```text
[record_ordinal, family_ordinal, start, end, kind_ordinal, ...payload]
```

Literal enum strings are absent from fact rows. A versioned schema manifest
maps ordinals to names for validation and human rendering.

Names and other open vocabulary use a batch-local string dictionary:

```text
strings([text_0, text_1, ...])
fact(..., local_name_id, ...)
```

The storage adapter resolves each batch-local string ID to a durable dense
`string_id` in batched queries. Extractor-local IDs never become durable IDs.

Readable diagnostic JSONL is a renderer over the typed batch. It is not the
store ingestion contract.

## Sequence of reads, writes, and uniqueness

For one extracted file:

1. Normalize and intern the repo-relative path once.
2. Find or insert `file` by `(repo_id,path_id)`.
3. Find or insert `blob` by raw digest bytes.
4. Find or replace the work `rev_file` row by `(rev_id,file_id)`.
5. Resolve batch-local names to durable IDs in chunks.
6. For Candidate A, deduplicate all batch spans in memory, batch insert
   `(blob_id,start,end)`, and fetch their dense IDs in chunks.
7. Insert fact rows using `rev_file_id`, `blob_span_id`, enum ordinals, and
   durable vocabulary IDs.
8. Commit the whole extraction batch in one transaction.

Uniqueness conditions:

```text
path:       normalized_path
file:       repo_id + path_id
rev:        repo_id + committed revision key
work rev:   root_id + work kind
blob:       digest
rev_file:   rev_id + file_id
blob_span:  blob_id + start + end
```

No path, digest, enum spelling, line, column, source slice, or canonical
compound JSON is stored in a fact row.

## Decisions

Proposed:

1. `FileId` means logical `(repo,path)` identity.
2. `BlobId` means immutable content identity.
3. `RevFileId` means one revision-qualified file occurrence.
4. `BlobSpanId` means one immutable content range.
5. `file_span` is `(RevFileId,BlobSpanId)` with a shared-blob invariant.
6. `file_span` starts as a view/value and receives no table or surrogate.
7. paths and open names are dictionary values stored once.
8. enum families, record variants, and kinds are versioned integer ordinals.
9. extractor batches carry file identity once in a batch header.
10. text and line/column are derived through blob-backed bindings.
11. repository and span vocabulary lives in the standard library over
    ordinary relations and typed bindings.
12. the language kernel receives no new file-specific syntax.

Rejected:

- `FileId = content hash`: it cannot identify a repo-relative file across
  revisions.
- `file_span = (path,start,end)`: it repeats paths and leaves revision and
  content identity implicit.
- `file_span = (blob,start,end)` as the only located type: it cannot recover a
  unique repo, revision, and path.
- stored text, line, or column columns: each is derivable from immutable blob
  bytes.
- string enum values in store-facing JSONL: they repeat closed vocabulary per
  fact.
- generic canonical JSON for `file_span`: it repeats both reference values and
  adds parsing at the query boundary.

<!-- todo(decision): Confirm the proposed semantic names File, Blob, RevFile,
and FileSpan before any declaration spelling or migration lands. -->

<!-- todo(decision): Choose whether work revisions retain one stable rev_id while
rev_file rows change, or mint a new observation revision per accepted watcher
batch. -->

<!-- todo(decision): Choose the user-visible declaration spelling for standard
library value views separately from their rel declarations. No spelling is
introduced by this plan. -->

## Verification

Before implementation:

1. Add shared bench columns for database bytes divided by corpus bytes,
   database bytes divided by input facts, and total span references divided by
   distinct content spans.
2. Measure Candidate A and Candidate B on the callgraph, flow, comment, and
   crawl corpora.
3. Record SQLite `dbstat` bytes by table and index.
4. Record statement counts and `EXPLAIN QUERY PLAN` output for:
   - all spans in a repo at a revision;
   - all revisions and blobs of one logical file;
   - text and line/column for one span;
   - all placements of one content span;
   - ingest of one file with every extraction family enabled.

Semantic fixtures:

1. identical bytes at two paths produce one blob and two files;
2. one file with changed bytes across two revisions produces one file, two
   blobs, and two rev-file placements;
3. the same content range reused by three families resolves to the same
   `BlobSpanId` under Candidate A;
4. a blob span located through two placements returns two file spans;
5. text, line, and column remain identical after the working path changes;
6. slicing rejects out-of-bounds ranges and preserves the parent blob;
7. no fact-row schema contains path text, digest text, enum text, source text,
   line, column, or canonical struct JSON;
8. store-facing JSONL contains one batch identity and numeric closed
   vocabulary.

Existing gates remain at their measured baseline until migrations are
explicitly approved.

## Staffing

Planning and measurement only until the four decision comments close.

Measurement lane: one read/write worktree, no language grammar changes. It may
add amplification sensors, the Candidate A/B benchmark schema, fixtures, and a
result document. Base SHA must be recorded at dispatch. Suite budget is one
targeted store test run per implementation, two benchmark repetitions per
corpus, and one final existing storage suite.

Implementation splits after the measurement:

1. storage spine and batch adapter;
2. extractor typed-batch wire;
3. standard file relations and bindings;
4. program migrations and removal of sibling path/span columns;
5. line/column translators, grep text hosts, and concatenated coordinate IDs
   removed after parity receipts.
