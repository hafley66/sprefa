# File span kernel, host, and relation-reference boundary lab

## Context

This lab exercises the current DL6 parser, compiler, emitted SQLite runtime,
extractor output, store prototype, and host machinery. It continues:

- `plans/2026-07-29-rel-value-unification-lab.md`;
- `plans/2026-07-29-file-span-storage-lab.md`;
- the `type` keyword removal and queryable relation-reference prototype.

The semantic budget is existing `rel`, `key`, rules, aggregates, arithmetic,
enum/match lowering, and host demand/response relations. New surface spelling
requires a current-world case that cannot lower through those mechanisms.

The user constraints are:

- entities and relations form the model;
- a relation-domain column is a graph edge to a row of that relation;
- no dictionary relation, canonical JSON value, nullable variant payload, or
  repeated path/name/enum spelling in fact rows;
- files and file spans remain queryable through repo and revision placement;
- Git bytes may be fetched later and may also be materialized;
- storage growth, resident memory, statement count, and N+1 behavior require
  measurements;
- `ref` remains a candidate only for cases automatic typed edge lowering
  cannot express.

### Current-world receipts

`v6/prolog/labs/rel_value_unification/5_reference_relation_holes.pl`
checks the relation-edge prototype. Its nine checks establish one public
target relation, an integer edge endpoint in the parent, direct RHS target
queries, an indexed dereference join, no dictionary or stored JSON columns,
and no invented foreign-key policy. It also exposes two holes: `key(...)`
does not drive edge identity, and the old content-DAG test rejects keyed
entity cycles.

`v6/prolog/labs/rel_value_unification/7_kernel_host_ref_holes.pl` checks the
current compiler with a file-span program. Its nine checks establish:

- relative slicing uses existing arithmetic and guards;
- line ordinal uses the existing `count` aggregate;
- column anchoring uses the existing `max` aggregate;
- no current pure expression extracts a byte substring;
- `finding(span(Start, End))` currently writes a JSON compound term into an
  INTEGER reference column;
- scanning `span(Start, End)` cannot capture its opaque row identity;
- `ref(...)` has no registered semantics and is currently another JSON
  compound term;
- an interned string can already be expressed as an ordinary referenced
  relation.

The extractor census found 7,345,805 span references and 2,073,233 distinct
spans in 1,048 tracked source files. The selected physical span cell is:

```text
file_span(file_span_id, rev_file_id, start, end)
```

At the measured real multiplicity it used 32.24 database bytes per analysis
fact. Repeating path, digest, kind/name text, and coordinates used 315.31
bytes per fact.

The path benchmark over 3,019 current paths and 20 references per path found:

| cell | database bytes | bytes/reference | prefix lookup |
|---|---:|---:|---:|
| dedicated whole-path relation | 962,560 | 15.9417 | 0.0374/0.0381 ms |
| universal string relation plus path edge | 1,036,288 | 17.1628 | 0.1476/0.1472 ms |
| segment relation and junctions | 1,150,976 | 19.0622 | 0.0582/0.0590 ms |
| repeated path text | 3,477,504 | 57.5936 | 1.0328/1.0328 ms |

The extractor-name benchmark over 200 files found 39,642 name occurrences,
6,269 distinct names, and zero strings shared between the path and name
domains. Separate path/name relations and one universal string relation both
used 1,728,512 bytes and about 4.7 to 4.9 microseconds per exact name lookup
in two runs. This measurement provides no cross-domain deduplication benefit.

## Type signatures

These signatures describe checked relation shapes. They add no syntax.

```text
string       : (content: text) key(content)
repo         : (name: string) key(name)
path         : (text: string) key(text)
file         : (repo: repo, path: path) key(repo, path)

revision     : (repo: repo, identity: string) key(repo, identity)
committed    : (revision: revision, oid: string) key(revision)
worktree     : (revision: revision, root: string, base: revision)

blob         : (digest: string, byte_len: int) key(digest)
git_blob     : (blob: blob, repo: repo, oid: string) key(blob, repo)
stored_blob  : (blob: blob, bytes: bytes) key(blob)

rev_file     : (revision: revision, file: file, blob: blob)
               key(revision, file)
file_span    : (file: rev_file, start: int, end: int)
               key(file, start, end)

newline      : (blob: blob, offset: int) key(blob, offset)
span_text    : (span: file_span, text: string) key(span)
```

`committed` and `worktree` are total membership relations over revision
entities. `git_blob` and `stored_blob` are additive capabilities, so one blob
may have either or both. Absence is represented by absence of a row.

The `string` relation is an available language expression of interning.
Whether the store maps selected text domains through one resident interner is
a physical policy. The current benchmark does not require every text column
to acquire a visible `string` edge.

## Instance timelines and lifetimes

```text
string, path, repo, file
  corpus lifetime; dense physical IDs may be rebuilt

committed revision, blob
  immutable identity lifetime

worktree revision
  one observed snapshot lifetime

rev_file
  one file placement in one revision

file_span
  one byte range in one immutable placement

git_blob
  lifetime of a known retrieval capability

stored_blob
  lifetime of retained materialized bytes

newline
  durable or cached derivative of immutable blob bytes

span_text
  demand/response lifetime unless explicitly retained by a consumer
```

Removing `stored_blob` leaves blob, placement, span, and Git retrieval
identities unchanged. Reobserving a worktree creates or replaces revision
membership according to its existing key and clock policy.

## Storage, reads, writes, and uniqueness

Each relation has one ordinary table with a hidden dense row ID where another
relation references it. Declared `key(...)` columns determine semantic
identity and the UNIQUE index. An unkeyed set relation uses its full row.

One extraction batch performs:

1. Intern open strings in the existing resident interner.
2. Resolve repository, path, file, revision, and blob rows in batches.
3. Resolve `rev_file` rows by `(revision,file)`.
4. Deduplicate `(rev_file,start,end)` inside the batch.
5. Resolve or insert `file_span` rows with one multi-row statement per table.
6. Replace extractor coordinates in analysis facts with dense
   `file_span_id` endpoints.
7. Flush newly observed durable strings once at the batch boundary.

Queries use ordinary joins:

```sql
-- Span and placement by repository revision and path.
SELECT fs.file_span_id, fs.start, fs.end
FROM file_span fs
JOIN rev_file rf ON rf.rev_file_id = fs.rev_file_id
JOIN file f ON f.file_id = rf.file_id
WHERE rf.rev_id = ? AND f.path_id = ?;

-- Every placement of the same immutable content range.
SELECT rf.rev_id, rf.file_id, fs.file_span_id
FROM file_span fs
JOIN rev_file rf ON rf.rev_file_id = fs.rev_file_id
WHERE rf.blob_id = ? AND fs.start = ? AND fs.end = ?;
```

The measured direct `file_span` cell has covering SEARCH plans for forward
and reverse access. No `blob_span` row is materialized.

### Batch and memory policy

| operation | current/candidate shape | batch boundary | retained memory |
|---|---|---|---:|
| string interning | in-process `lasso::Rodeo` | durable new strings per extraction batch | interner corpus |
| relation ID resolution | SQLite lookup/insert | one multi-row operation per relation per batch | deduplicated batch keys |
| Git bytes | persistent `git cat-file --batch` per repo | requested blob IDs grouped by repo | response plus byte cache |
| stored bytes | SQLite keyed reads | requested blob IDs per tick | response plus byte cache |
| newline scan | once when blob bytes enter cache | one blob | measured 212,892 B for 300 indexes |
| blob cache | byte-bounded LRU | per fetched blob | measured ceiling 1,048,564 B |
| byte slice | direct cache operation | requested spans grouped by blob | response strings |

The existing TypeScript shell host spawns once per witness. Reusing that
executor for span text would make process count proportional to span demand.
The provider boundary therefore needs registered batched execution behind the
existing demand/response relation plan. This is an implementation requirement,
not a file-specific surface form.

## `ref` cases and proof status

| case | current result | automatic edge sufficient | explicit identity operation needed |
|---|---|---:|---:|
| parent constructs target from all keyed fields | compiler emits wrong JSON term | yes | no |
| parent matches target fields through typed column | indexed join prototype exists | yes | no |
| top-level RHS queries target membership | ordinary public relation query works | yes | no |
| scan target and retain opaque row identity without repeating fields | variable is unbound | no | candidate |
| pass one target identity through another multi-column relation | no current row-identity term | no | candidate |
| select a target by composite key and construct an edge | `key` is ignored by prototype | yes after key fix | no |
| missing referenced target | policy not implemented | typed construction must fail or produce no row | no |

The ordinary case can infer reference construction from the destination
column domain and the relation-shaped value. `ref(...)` currently adds no
behavior. Its remaining proof obligation is opaque row-identity capture and
transport. Multi-arity follows from the target relation key, so a separate
arity-specific reference concept is not established.

## Host and self-host boundary

DL6 rules can express:

- committed/worktree membership and their union;
- repo, revision, file, blob, and placement joins;
- file-span construction and range validation;
- relative span slicing through arithmetic;
- line ordinal from `newline` with `count`;
- column from the maximum preceding newline and subtraction;
- optionality through relation membership;
- capability selection between `git_blob` and `stored_blob`.

The external provider performs:

- filesystem and Git observation;
- persistent Git batch reads;
- optional byte materialization;
- byte acquisition for demanded spans;
- newline offset extraction while immutable bytes are resident.

The current expression evaluator cannot slice bytes. `span_text` therefore
fits an ordinary demand/response relation whose provider batches spans by
blob. A future generic byte-slice expression would move only the final slice
into the kernel; it would not change file, blob, span, or host identity.

## Decisions

### D1. Relation-domain columns

Use automatic typed graph edges. The destination column determines the target
relation. Construction resolves the target's declared key and stores its
dense row ID. Matching lowers to an indexed join. Direct RHS use remains an
ordinary relation query.

Cost:

- make existing `key(...)` drive row lookup and uniqueness;
- remove JSON compound emission in relation-domain columns;
- permit keyed entity cycles while retaining the content-cycle refusal;
- migrate the nine old nested-JSON oracle expectations.

No new syntax is selected.

### D2. `ref`

Keep `ref` unregistered while the opaque identity cases are labbed. Automatic
typed construction covers keyed field construction and dereference. A
candidate identity operation proceeds only if the real compiler cannot expose
row identity through existing variable or mode machinery without ambiguity.

Options for the remaining identity-capture question:

1. Automatic hidden identity binding in a typed destination context.
   No new spelling; cannot name an identity during a standalone RHS scan.
2. Existing-variable capture added to relation patterns.
   Names identity and fields together; changes pattern arity or parser rules.
3. `ref(RelationPattern)` as identity capture.
   Adds one expression form; key arity remains owned by the relation.
4. `ref(Identity, RelationPattern)` as identity plus destructuring.
   Adds one pattern form and an explicit identity variable.
5. No first-class identity value.
   Callers repeat keys and let typed construction resolve them.

Each option must lower through the same keyed row-resolution and join path.

### D3. Strings

Keep whole-path and name relations with dense IDs. Permit the store to use its
existing resident interner and batched durable string mirror internally.
Universal visible string edges remain optional because the measured corpus
had zero path/name overlap and no storage reduction.

Options if a uniform visible `string` relation is reconsidered:

1. Physical-only interner behind text columns.
   No language change; SQL tables keep domain-specific indexes.
2. Visible `string` edge for every open text domain.
   One namespace; adds joins and reference rows.
3. Visible `string` edges only for declared interned domains.
   Requires metadata or a declaration policy.
4. Separate path/name relations backed by one physical string pool.
   Preserves domain types; compiler/store mapping becomes indirect.

### D4. Span text and newlines

Use an ordinary typed demand/response relation with a registered batched
provider. Group requests by blob, consult the byte-bounded cache, fetch cold
Git blobs through one persistent batch process per repository, compute the
newline index once, and emit response rows.

Options for the byte-slice boundary:

1. Provider emits `span_text(span,text)`.
   No evaluator change; provider owns batching and slicing.
2. Provider emits blob bytes and kernel adds a generic byte-slice expression.
   Adds one general expression; moves response strings into the kernel.
3. Provider emits both `newline(blob,offset)` and `span_text`.
   DL6 owns line/column rules; provider owns byte work.
4. Persist `stored_blob` on first demand.
   Faster later reads; database grows by retained content bytes.

### D5. Provider binding

Use the existing host demand/response relation plan and link a typed,
non-shell executor by relation signature. Keep `bind` for continuous world
sources. Do not add `host rel`, file-specific calls, or arrow-return syntax
until a current fixture proves the existing plan lacks required clock or
cardinality information.

Options for link metadata:

1. Link configuration keyed by an existing relation signature.
   No DL6 spelling; deployment supplies provider, modes, and batching policy.
2. Existing host declaration generalized from shell to named executor.
   Reuses current declaration category; changes its payload.
3. Existing `bind` generalized for demand/response.
   Unifies providers; must preserve bind's continuous-source clock.
4. New surface declaration.
   Requires a demonstrated parser/runtime case unresolved by options 1 to 3.

## Verification

Run:

```sh
swipl -q -f v6/prolog/labs/rel_value_unification/5_reference_relation_holes.pl
swipl -q -f v6/prolog/labs/rel_value_unification/7_kernel_host_ref_holes.pl
python3 v6/sprefa-store/bench/file_span/3_paths.py \
  --root . --refs 20 \
  --output v6/sprefa-store/bench/file_span/path-results-4.json
python3 v6/sprefa-store/bench/file_span/4_strings.py \
  --root . \
  --output v6/sprefa-store/bench/file_span/string-results-2.json
swipl -q -f v6/prolog/ARCH.pl -g go -t halt
dl examples/gen-plans-index.dl
dl examples/gen-plans-index.dl --check
```

Implementation exit conditions:

- key-driven single and composite relation references pass actual emitted
  SQLite tests;
- target rows remain publicly queryable;
- edge construction stores an integer row ID and no JSON term;
- keyed entity cycles and content-key cycles have separate receipts;
- missing target and retraction behavior have tick-log receipts;
- provider statement/process count is bounded by batch and repository count;
- file-span queries retain indexed SEARCH plans;
- the two unrelated untracked flow fixtures remain untouched.

## Staffing

One implementation lane owns key-driven reference resolution and oracle
migration. One later runtime lane owns batched typed provider registration and
span-text receipts. Both remain review-gated because either lane can otherwise
create a new surface construct.
