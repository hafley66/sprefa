# Option versus null lab

## Standing

Lab base: `d3cb5eeaa0fcc9d9f3963ebe28c294843cfd14bc`, worktree
`sprefa-lane-optionlab`, branch `lane/option-vs-null`, verified with
`git rev-parse HEAD`.

No production edits. No syntax lands. Two files: this document and
[2026-07-30-option-versus-null-receipts.mjs](2026-07-30-option-versus-null-receipts.mjs),
83 assertions, every one executed against both SQLite builds the project
ships against (system `sqlite3` 3.43.2 and `@libsql/client` 0.17.4 carrying
SQLite 3.45.1).

**This lab deliberately does not close the question.** The user's instruction
was to widen it and price every branch. Section 10 is a card list with the
branches left open and the evidence that would close each. Section 11 says
what to measure next. There is no recommendation.

### The position this lab was given

The user's own words, and they moved during the conversation:

> earlier: "we should just add json/sql nulls and get it over with in a way
> that is coherent"
>
> now: "i would prefer no nulls but then how does souffle handle json lmfao"
>
> and the candidate floated: "we could use Option i wouldnt mind but that is a
> rel that will explode in values lmfao bc of how rel to table works i guess.
> hmmmmm. we can also json union the result of that json thing? so that its
> Some(json) ; None ; Undefined"

That last idea is the starting point and it is graded first, in section 1.

### Relationship to the two sibling lanes

`lane/nullplan` is writing the Design D implementation plan. At the time this
lab ran, that worktree was clean at `22c0c9f7` with no plan file written, so
there was nothing to read and nothing to disagree with directly. This lab
therefore disagrees in writing with the document nullplan is implementing,
[2026-07-30-null-coherence-lab.md](2026-07-30-null-coherence-lab.md), on three
specific points. They are collected in section 6 with receipts. Two of the
three are new facts that lab did not have; one is a hypothesis of this lab's
own that the measurement refuted, and it is recorded as refuted.

`lane/json-wiring` owns `parse_dl.pl`, `print_dl.pl`, `lower.pl`,
`registry.pl`, `analyze.pl`. This lab read all five and wrote none.

---

## 1. The three-variant json read, built and graded

The json1 trap has exactly three states and the user's enum has exactly three
variants. They line up:

| source state | `json_extract` | `json_type` | user's variant |
|---|---|---|---|
| key present with a value | the value | `text`/`integer`/`real`/`array`/`object`/`true`/`false` | `Some(v)` |
| key present, value is json null | SQL NULL | text `null` | `None` |
| key absent | SQL NULL | SQL NULL | `Undefined` |

`json_extract` collapses the last two. `json_type/2` does not. So the whole
classifier is one expression over `json_type` and it never consults
`json_extract` for the tag at all:

```sql
CASE
  WHEN json_type(document, '$.commit') IS NULL   THEN 'undefined'
  WHEN json_type(document, '$.commit') = 'null'  THEN 'none'
  ELSE 'some'
END
```

**Receipt V1** runs this over seven documents on both engines and gets the same
seven rows. The four `some` cases cover a string, the integer `0`, the empty
string, an empty array, and a nested object, which is the set that a
truthiness-based or `IS NOT NULL`-based classifier gets wrong:

```
subject 1 {"commit":"c1"}                 some       json_type=text     -> '"c1"'
subject 2 {"commit":null}                 none       json_type=null     -> 'null'
subject 3 {}                              undefined  json_type=NULL     -> NULL
subject 4 {"commit":0}                    some       json_type=integer  -> '0'
subject 5 {"commit":""}                   some       json_type=text     -> '""'
subject 6 {"commit":[]}                   some       json_type=array    -> '[]'
subject 7 {"commit":{"nested":null}}      some       json_type=object   -> '{"nested":null}'
```

**Receipt V5** is the negative control and it is the reason this matters. Over
those same seven documents, the presence predicate the current json lowering
uses, `json_extract(...) IS NOT NULL`, says six documents have the key; the
`json_type` predicate says six too, but they are not the same six. Counted
directly: `extract_presence_says_present` is 5 and `json_type_says_present` is
6. One document is silently lost by the shipped predicate.

**Receipt V2** projects the same read into the exact table shape
`v6/prolog/0_enum_expand.pl` generates, three variant tables plus the derived
tag view, and the tag rows come out right on both engines. Every column in
every one of those tables is `NOT NULL`. No null is written anywhere. The
three-variant read is total.

**Receipt V4** closes it under nesting. A json null nested inside a `Some`
payload stays inside the payload as json text and is never confused with the
`None` variant. Only the top path needs the three-way test; everything below it
stays inside the json value. That is what makes this a bounded construction
rather than a recursive one.

### The state the three variants cannot express

**Receipt V3.** An Option has four observable states, not three. The fourth is
row absence: a subject nobody probed. It is not `Undefined`, which means "we
looked and the key was not there". It is "we never looked".

```
id 1  ->  some
id 2  ->  none
id 3  ->  undefined
id 4  ->  no row at all
```

No variant can express the fourth, because the fourth is the absence of the
variant relation's row, and that is exactly the state the classical model
already uses for everything. Any Option encoding in a relational engine sits
on top of a state the engine already has and cannot remove. Section 8 says why
this is not a defect of the design so much as a fact about the medium.

### Does the read work today, end to end?

Partly. **Receipt C5**: the user's three-variant declaration compiles.

```dl
rel document(id: int, body: text).
rel json_read(some(id: int, payload: text) ; none(id: int) ; undefined(id: int)).

json_read_some(Id, Id, Payload) <+ document(Id, Payload).
```

**Receipt C2** is the wall. The same enum fed by a *level* rule, which is the
shape an outer join has, is refused:

```
unsupported_construct(keyed_level_head(repo_latest_some/3))
```

`0_enum_expand.pl:expand_variant/4` attaches `keyed(VariantRef, KeyPositions)`
to every generated variant relation, and `keyed_level_head` is a live refusal
(ruled 2026-07-29). So generated variant relations can only ever be fed by
arrivals or by edge rules. **An Option produced by a derived rule does not
compile today.** That is the single most important mechanical fact in this
lab, and it is a fact about the enum expansion's key choice, not about
Options.

---

## 2. Candidate A: Option as three variant relations

The user's spelling, as it would actually be typed:

```dl
rel repo(name: text).
rel latest_commit(repo: text, commit: text).
rel repo_latest(some(repo: text, commit: text) ; none(repo: text)).

repo_latest_some(Id, Repo, Commit) <- repo(Repo), latest_commit(Repo, Commit), Id := 1.
repo_latest_none(Id, Repo)         <- repo(Repo), not(latest_commit(Repo, _)), Id := 2.
```

Refused today, receipt C2. Written with `<+` it compiles, receipt C3.

Emitted tables, copied from the compiler output:

```sql
CREATE TABLE "repo_latest_some" ("id" INTEGER NOT NULL, "repo" TEXT NOT NULL, "commit" TEXT NOT NULL, PRIMARY KEY ("repo", "commit")) WITHOUT ROWID;
CREATE TABLE "repo_latest_none" ("id" INTEGER NOT NULL, "repo" TEXT NOT NULL, PRIMARY KEY ("repo")) WITHOUT ROWID;
CREATE TABLE "repo_latest_tag"  ("id" INTEGER NOT NULL, "tag" TEXT NOT NULL, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("id", "tag")) WITHOUT ROWID;
```

### Pure rxjs lowering

Standing repo law: every snippet carries its rx lowering, and a construct
whose rx lowering cannot be written is a design defect. This one can:

```ts
type RepoRow = readonly [repo: string];
type LatestRow = readonly [repo: string, commit: string];

const partitioned$ = combineLatest([repoRows$, latestRows$]).pipe(
  map(([repoRows, latestRows]: readonly [readonly RepoRow[], readonly LatestRow[]]) => {
    const commitByRepo = new Map(latestRows);
    return {
      some: repoRows.flatMap(([repo]) => {
        const commit = commitByRepo.get(repo);
        return commit === undefined ? [] : [[repo, commit] as const];
      }),
      none: repoRows.flatMap(([repo]) => (commitByRepo.has(repo) ? [] : [[repo] as const])),
    };
  }),
);

// The tag view is a plain projection, not a fourth source.
const tag$ = partitioned$.pipe(
  map(({ some, none }) => [
    ...some.map(([repo]) => [repo, "some"] as const),
    ...none.map(([repo]) => [repo, "none"] as const),
  ]),
);
```

Direct. No operator does anything unusual; the partition is a `map` over two
arrays, which is the plain-array rule from the standing style laws.

### Hazards this lab found in the shipped encoding

**Receipt E1.** `content_key_positions/2` in `0_enum_expand.pl` puts the
*content* columns in the primary key and leaves the `id` discriminator out.
So two different `Some` payloads for the same subject both survive:

```
id 1, commit-a
id 1, commit-b     -> options_for_this_subject = 2
```

The Option is not a function of the subject. Nothing in the storage layer says
it should be.

**Receipt E2.** `Some` and `None` coexist for the same subject with no refusal
at the storage layer either. The tag view happily carries two tags for one id.
Exhaustiveness is checked at the `match` site (`match_nonexhaustive`), never at
the write site. So the encoding permits a value that is simultaneously present
and absent, and only a program that happens to `match` on it will notice.

**Receipt E3.** A `Some` to `None` transition is four writes across three
tables: delete the some row, delete its tag row, insert the none row, insert
its tag row. Under the incremental emitter that is four delta rows through four
frontier tables per transition.

### Tier-0 classification

- World-fed Option, three variant relations plus derived tag: **(a)**, sugar
  over an existing lowering. It ships. Receipt C5 compiles it.
- Derived Option, the outer-join shape: **(b)**, a new lowering of existing
  semantics. Nothing semantic is missing; `keyed_level_head` and
  `content_key_positions/2` between them refuse it. Relaxing either is a
  lowering change, not a semantics change.

Not tier 0 either way.

---

## 3. Candidate B: Design D, `T?` with total equality

The null-coherence lab's recommendation, written as that lab writes it:

```dl
rel repo(name: text).
rel latest_commit(repo: text, commit: text).
rel repo_latest(repo: text, commit: text?).

repo_latest(Repo, Commit) <- repo(Repo), latest_commit(Repo, Commit).
repo_latest(Repo, null)   <- repo(Repo), not(latest_commit(Repo, _)).

rel missing_latest(repo: text).
missing_latest(Repo) <- repo_latest(Repo, Commit), Commit == null.
```

Semantics: `null == null` is true, `null == value` is false, and ordered
comparison, arithmetic and aggregate input all require an explicit
`present(Value)` narrowing first. SQL equality lowers to `IS NOT DISTINCT
FROM`; SQL 3VL stays an implementation detail below the language.

### Pure rxjs lowering

```ts
type Nullable<Value> = Value | null;
type RepoLatest = readonly [repo: string, commit: Nullable<string>];

const repoLatest$ = combineLatest([repoRows$, latestRows$]).pipe(
  map(([repoRows, latestRows]): readonly RepoLatest[] => {
    const commitByRepo = new Map(latestRows);
    return repoRows.map(([repo]) => [repo, commitByRepo.get(repo) ?? null]);
  }),
);

const missingLatest$ = repoLatest$.pipe(
  map((rows) => rows.filter(([, commit]) => commit === null)),
);
```

Shorter than candidate A's, which is the honest ergonomic point in its favour.
One relation, one consumer, one arm.

### What this lab measured that the null-coherence lab did not

**Receipt C4 and C7.** The spelling does not parse today, which that lab
already said. What it did not say is what the author sees:

```
ERROR: ... Unknown message: dl_parse_error(statement,[114,101,108,32,...])
```

Not a named refusal. swipl's Unknown-message fallback, carrying the source
rendered as a list of character codes. This is language-design-review finding
B4 in the wild, and it is the first thing anyone typing Design D would hit.

**Receipt D1, and this is the important one.** The null-coherence lab's card 5
refuses nullable columns in *declared keys*. That refusal is too narrow. Every
unkeyed set relation's primary key is its whole row, and the emitted DDL is
`WITHOUT ROWID`, which makes every primary key column implicitly `NOT NULL`.
So:

```sql
CREATE TABLE "repo_latest" ("repo" TEXT NOT NULL, "commit" TEXT, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("repo","commit")) WITHOUT ROWID;
INSERT INTO "repo_latest" ("repo","commit") VALUES ('beta', NULL);
-- Runtime error: NOT NULL constraint failed: repo_latest.commit
```

Both engines. And a derived relation is unkeyed *by construction*, because
`keyed_level_head` refuses a key declaration on a level-rule head. So the
flagship Design D program above, the one-relation outer join, cannot use the
default set-relation table family at all.

**Receipt D2.** The only current family that tolerates the null is
`__id INTEGER PRIMARY KEY` plus `UNIQUE (columns)`. UNIQUE treats two nulls as
distinct, and the emitter's write verb is `INSERT OR IGNORE`
(`lower.pl`, arrival and level insert paths). Result, both engines:

```
INSERT OR IGNORE ('beta', NULL)  x2  -> 2 rows
INSERT OR IGNORE ('alpha','a1')  x2  -> 1 row
```

A set relation now holds the same row twice. The B-plane set invariant that
`TICK-MODEL.md` requires is broken by the storage family, silently, on the
first duplicate arrival.

**Receipt D3.** The negation trap the null-coherence lab named, re-derived on
the emitter's actual `NOT EXISTS` shape rather than on a bare scalar. With a
null-bearing row present on both sides, the emitted form reports it absent
(count 1); the `IS NOT DISTINCT FROM` form reports it present (count 0).

**Receipt D5.** The same defect at the key-lookup site, which is worse than the
negation one because it is not a logic subtlety, it is a lost row:

```
INSERT ('repo-10', NULL)
SELECT ... WHERE "repo" = 'repo-10' AND "commit" = NULL          -> 0 rows
SELECT ... WHERE "repo" IS NOT DISTINCT FROM ... AND ... NULL    -> 1 row
```

The emitted key lookup cannot find a row it just wrote.

### The hypothesis this lab had and the measurement refuted

Going in, this lab expected `IS NOT DISTINCT FROM` to lose the index and
degrade every rewritten lookup to a table scan, which would have been a
decisive performance argument against Design D.

**Receipt D4, D4b, D4c: it does not.** On a 20,000-row table with a UNIQUE
index, on both builds:

```
"repo" = ? AND "commit" = ?
  -> SEARCH keyed_rel USING COVERING INDEX sqlite_autoindex_keyed_rel_1 (repo=? AND commit=?)
"repo" IS NOT DISTINCT FROM ? AND "commit" IS NOT DISTINCT FROM ?
  -> SEARCH keyed_rel USING COVERING INDEX sqlite_autoindex_keyed_rel_1 (repo=? AND commit=?)
"repo" IS NOT DISTINCT FROM ? AND "commit" IS NOT DISTINCT FROM NULL
  -> SEARCH keyed_rel USING COVERING INDEX sqlite_autoindex_keyed_rel_1 (repo=? AND commit=?)
```

Identical plans, including when the compared literal is itself NULL. The
null-safe rewrite costs correctness work in the compiler and nothing at the
planner. The strongest performance argument against Design D does not exist.
Recorded as refuted rather than quietly dropped.

**Receipt N2.** `GROUP BY` and `DISTINCT` are already null-safe, so the
boundary-diff plane needs no repair at all. Only rule-level equality does.
`multisetDiff`'s `JSON.stringify` row key already encodes JS null stably.

**Receipt N3.** The asymmetry SQLite's own documentation calls arbitrary,
measured in one database on one column: UNIQUE keeps three nulls, DISTINCT
collapses them to one.

### Where the null-safe rewrite has to travel

`v6/prolog/compile/lower.pl` is 2,688 lines and emits column equality from five
named families: `where_text/2` (three clauses, `:308-312`),
`key_join_equalities/5` (`:874`), `eq_placeholder/2` (`:1405`),
`qualified_equalities/4` (`:2078`), `delta_reference_identity/8` (`:2300`).
A type-directed rewrite has to reach all five and be *selective*, because
rewriting a total column to `IS NOT DISTINCT FROM` is correct but rewriting
everything loses nothing at the planner (D4) and loses the ability to tell a
reader which columns can be null.

### Tier-0 classification

**(c), tier 0.** A relation column gains a value that is in no current column
domain, and the single-relation outer result is a row shape no current fixed
arity total relation can express. This lab agrees with the null-coherence lab's
classification and adds that its structural cost is larger than that lab
priced, by receipts D1 and D2.

---

## 4. Candidate C: row absence, the status quo

```dl
rel repo(name: text).
rel latest_commit(repo: text, commit: text).
rel repo_with_latest(repo: text, commit: text).
rel repo_without_latest(repo: text).

repo_with_latest(Repo, Commit) <- repo(Repo), latest_commit(Repo, Commit).
repo_without_latest(Repo)      <- repo(Repo), not(latest_commit(Repo, _)).
```

Compiles today, receipt C1. Emits 4 persistent tables, 14 scratch tables, 12
indexes, 289 lines.

### Pure rxjs lowering

Identical in shape to candidate A's, minus the tag projection. It is the same
partition; candidate A is this plus a derived tag view, which is why candidate
A classifies as (a) rather than (b) in the world-fed case.

### Cost

Every downstream consumer has two input relations and two rule arms. The tick
log carries changes under two relation names. That is the whole complaint, and
it is real: it is a per-consumer cost paid forever, in exchange for a storage
model with no new semantics anywhere.

### Tier-0 classification

None. It ships.

---

## 5. Candidate D: optionality at the use site, never in storage

Found in prior art, and it is the one that changes the shape of the argument.

Datomic has no null for ordinary attributes. Absence is the datom not
existing. But a query clause on a missing attribute yields no binding, so the
whole tuple drops out, which is an inner-join effect nobody wants. Datomic's
answer is not to add a null to storage; it is `get-else`, an operator at the
*use site* that supplies the default so the tuple survives.

Exact signature, from
[docs.datomic.com/query/query-data-reference.html](https://docs.datomic.com/query/query-data-reference.html):

```
[(get-else src-var ent attr default) ?val-or-default]
```

and the doc's own worked example:

```clojure
(d/q '[:find ?artist-name ?year :in $ [?artist-name ...]
       :where [?artist :artist/name ?artist-name]
              [(get-else $ ?artist :artist/startYear "N/A") ?year]]
     db ["Crosby, Stills & Nash" "Crosby & Nash"])
=> #{["Crosby, Stills & Nash" 1968] ["Crosby & Nash" "N/A"]}
```

Transposed into this project's surface, as a body operator over the existing
two-relation storage:

```dl
rel repo(name: text).
rel latest_commit(repo: text, commit: text).
rel repo_latest(repo: text, commit: text).

repo_latest(Repo, Commit) <-
  repo(Repo),
  get_else(latest_commit(Repo, Commit), 'absent').
```

One relation out. One consumer arm. No column is ever nullable. No extra
relation is stored. The optionality is spelled once, by the consumer that
cares, and the default is a value that consumer chose.

### Pure rxjs lowering

```ts
const repoLatest$ = combineLatest([repoRows$, latestRows$]).pipe(
  map(([repoRows, latestRows]) => {
    const commitByRepo = new Map(latestRows);
    return repoRows.map(([repo]) => [repo, commitByRepo.get(repo) ?? "absent"] as const);
  }),
);
```

The `??` is the whole construct. This is the shortest rx lowering of the four.

### Graded

**Receipt G1.** Over the same two-relation storage at 10,000 subjects with 10
percent absent, the lowering `LEFT JOIN` plus `coalesce` produces exactly
10,000 rows, 1,000 of them defaulted. Zero extra relations, zero extra stored
rows.

**Receipt G2.** The point read stays an index SEARCH on both sides of the LEFT
JOIN, both engines:

```
SEARCH r USING PRIMARY KEY (name=?)
SEARCH l USING PRIMARY KEY (repo=?) LEFT-JOIN
```

**Receipt G3, the hole, and it is a real one.** The default has to *be* a value
of the column's type, so it is stolen from the value domain. On `text` a
program can usually spare a string. On `int` there is no safe choice:

```
subject a, score -1   -> truth = real        (a genuinely scored -1)
subject b, score  7   -> truth = real
subject c, score -1   -> truth = defaulted
```

Two rows carry `-1` and the reader cannot tell them apart. That is exactly the
hole SQL NULL fills, and it is why `get-else` is a mitigation and not a
replacement.

Note also what Datomic does *not* claim: it is not null-free everywhere.
[docs.datomic.com/schema/schema-reference.html](https://docs.datomic.com/schema/schema-reference.html),
"Composite Tuples": "`nil` is a legal value for any slot in a tuple. This
facilitates using tuples in range searches, where `nil` sorts lowest." A
system that avoided null in its attribute model still admitted it in its
composite key model, for ordering. That carve-out is worth reading twice
before assuming any design stays null-free under pressure.

### Tier-0 classification

**(b)**, a new lowering of existing semantics. It adds no value to any column
domain, touches no key, no diff, no tick log, and no oracle ground-term
identity. It lowers to `LEFT JOIN` plus `coalesce`, both of which the emitter
already writes. It is the only candidate in this lab that gets the one-row
ergonomic win without a tier-0 construct.

It also does not solve the general problem, per G3.

---

## 6. Where this lab disagrees with the null-coherence lab, in writing

Three points, in descending order of how much they should change a decision.

**1. The nullable-key refusal is too narrow, and the gap is structural.**
That lab's card 5 recommends refusing nullable columns in declared keys.
Receipt D1 shows the default full-row primary key on every unkeyed set
relation has the same problem, and receipt D2 shows the only fallback family
loses set identity. Combined with `keyed_level_head`, which makes derived
relations unkeyable, the consequence is that the flagship Design D program in
that lab's own section 7.2 cannot use the default table family. That is not a
refinement of its 12-seam estimate; it is a thirteenth seam of a different
kind, a storage-family decision that has to be made before any of the other
twelve can be implemented.

**2. That lab's prior-art section reads Souffle's `nil` as more general than it
is.** It says "every record type has a ground `nil` value", which is correct,
and cites it as prior art for "a typed empty value". Direct verification adds
the constraint that changes the reading: `nil` is *only* legal in a
record-typed column. Souffle's own test
`tests/semantic/record_null/record_null.dl` fails to compile with
`Error: Nil constant used as a non-record` when `nil` appears in a `symbol`
column. So Souffle's precedent is not "a typed empty value for any column"; it
is "a distinguished terminator for the one type former that needs a base
case". Section 7 has the full reading.

**3. The performance argument this lab expected to make against Design D does
not exist.** Receipts D4b and D4c show `IS NOT DISTINCT FROM` planning
identically to `=` against a UNIQUE index on both builds, including against a
NULL literal. If a reader of that lab was holding out for a planner-level
objection, there is not one. Recorded here because it cuts against this lab's
own initial hypothesis and in favour of the design it was scrutinizing.

---

## 7. Prior art

### Souffle, which is the question the user actually asked

**No general null.** `nil` exists and is scoped to record types only.
[souffle-lang.github.io/types](https://souffle-lang.github.io/types), Record
Types section: "Every record type has the `nil` value." Verified constraint
from the compiler's own test suite, `souffle-lang/souffle`
`tests/semantic/record_null/record_null.dl` and its expected error file
`record_null.err`: `Error: Nil constant used as a non-record in file
record_null.dl at line 13`. A `number`, `symbol`, `unsigned` or `float` column
has no bottom value at all, and writing `nil` there is a compile-time type
error.

**ADTs.** Declaration form, from the same page:

```
.type <new-adt> = <branch-id> { <name_1>: <type_1>, ... } | ...
```

Zero-field branches are legal (`.type Nat = S {x : Nat} | Zero {}`).
Consumption has no `match` or `case` keyword: the only mechanism is ordinary
unification against the `$branch(args...)` constructor pattern in body
position, for example `X(term) :- X($I(_,term)).` from
`tests/semantic/adt_access/adt_access.dl`.

**Is Option idiomatic in Souffle? No, and there is a named gap.** Open issue
[souffle-lang/souffle#2315](https://github.com/souffle-lang/souffle/issues/2315),
"Way to check if an ADT value is *not* constructed with a certain branch
constructor". The reporter's attempt `X != $Baz(_)` does not work. Bernhard
Scholz's reply: "the positive description (what it can be) can be expressed in
Souffle. However, this comes at the cost of enumerating all permitted
branches... The more interesting question is what I cannot be." A `match (X) of
$Baz(_) => true | _ => false` extension is proposed in the thread and is
unshipped. **Negative matching on an ADT branch is a known, open gap in
Souffle.** That is directly relevant here: candidate A's "this subject has no
`Some`" query is exactly the shape Souffle cannot write without enumerating
branches.

**Records at runtime.** `src/include/souffle/RecordTable.h`: "Data container
implementing a map between records and their references. Records are separated
by arity". `pack(tuple, arity) -> RamDomain` and `unpack(ref, arity)`. The
implementation in `datastructure/RecordTableImpl.h` is a flyweight
`findOrInsert`. It is an interning table producing dense integers, and it
**never frees**: no `free`, `evict`, `gc` or `erase` on either `RecordTable` or
`RecordMap`; the only cleanup is destroying the per-arity maps at program exit.
Consistent with a batch engine that has no retraction. This project does have
retraction, which is why the types-as-rels lab's support-GC finding was needed
and Souffle's precedent stops being transferable at that exact point.

**JSON.** No json type. No json functor: the full intrinsic list at
[souffle-lang.github.io/arguments](https://souffle-lang.github.io/arguments)
is `cat, ord, strlen, substr, to_number, to_string, to_unsigned, to_float,
autoinc`, the bit and logical operators, `max`, `min`. Nothing json-shaped.
The documented I/O types at
[souffle-lang.github.io/directives](https://souffle-lang.github.io/directives)
are `file`, `stdin`, `stdout`, `sqlite`; the word "json" does not appear on
that page at all.

But `IO=jsonfile` exists and works, undocumented, in
`tests/semantic/jsonfile/jsonfile.dl`. Its documentation gap is itself tracked
as [souffle-lang/souffle#1675](https://github.com/souffle-lang/souffle/issues/1675).
And here is the citation that answers the user's joke exactly. Souffle's own
JSON output fixture, `tests/semantic/jsonfile/C.json`:

```json
[{"x": null},
{"x": {"head": 1, "tail": null}},
{"x": {"head": 2, "tail": {"head": 3, "tail": null}}}]
```

**Record `nil` round-trips to the JSON literal `null`.** Souffle's one typed
empty value is rendered as json null by its own writer.

So: how does Souffle handle json? It declares the shape up front as a `.type`
and a `.decl`, serializes to and from that fixed shape, and renders its one
typed empty (`nil`, legal only on record columns) as json `null`. It never
parses arbitrary json into datalog. A key that might not be there is not a
thing Souffle can encounter, because the shape is fixed before any json is
read. **The front end flattens it before it reaches datalog, and that is
itself the answer.**

**Any public regret over no-null?** NOT FOUND. The CAV 2016 tool paper
(Jordan, Scholz, Subotić, "Souffle: On Synthesis of Program Analyzers",
[psubotic.github.io/papers/cav16.pdf](https://psubotic.github.io/papers/cav16.pdf))
was read in full and does not discuss the type system at all. The nearest
thing to a maintainer admission of friction is Scholz's comment in issue #2315
about branch enumeration being inconvenient, scoped to ADTs, not to nulls.

One correction worth carrying: the paper often cited as "CC 2016" with the
title "On Synthesis of Program Analyzers" is CAV 2016. The CC 2016 paper by
the same group is "On Fast Large-Scale Program Analysis in Datalog".

Also relevant to how Souffle deals with foreign nulls at the boundary:
[souffle-lang/souffle#2113](https://github.com/souffle-lang/souffle/pull/2113),
merged, "Treat Null as n/a In SQLite Input": SQL NULL in a fact source is
coerced to an empty string. A sentinel, applied at ingest, exactly candidate
D's mechanism and exactly candidate D's hole.

### Datomic

Covered in section 5. The additional piece is Rich Hickey's argument, which is
the sharpest thing in the prior art for this specific question.

**"Maybe Not"**, Clojure/conj 2018, 29 November 2018.
[youtube.com/watch?v=YR5WdGrpoug](https://www.youtube.com/watch?v=YR5WdGrpoug);
transcript at `github.com/matthiasn/talk-transcripts`,
`Hickey_Rich/MaybeNot.md`. Verified against the raw transcript:

> "There is no such thing as a maybe thing. If names are strings, names are
> always strings. You either know the name, or you do not know the name. That
> is an orthogonal idea from 'what is a name?' A name is a string. Knowing a
> name is a different idea. If type systems make you jam those two things
> together, they are wrong, because they are separate ideas."

and, on where optionality does not belong:

> "It is a mistake to put optionality in aggregate definitions. There is no
> usage context... Making an aggregate definition that you are going to use all
> over the place -- it may be an argument sometimes. It may be a return
> sometimes. It may be arguments to five different functions that do different
> jobs. It is the wrong place for optionality."

He names Clojure spec, Haskell, Kotlin and Scala as making the same mistake, so
this is not a Clojure partisan point. **This is a direct argument against
Design D**, because `rel repo_latest(repo: text, commit: text?)` is exactly
optionality declared in an aggregate definition with no usage context, and it
is a direct argument *for* candidate D, which puts optionality at the use site.

It is not decisive. Hickey's argument is about schemas that are reused across
many call sites; a relation in this project has a narrower reuse profile than
a Clojure spec. But it is the strongest stated case anyone has made for the
position the user drifted toward, and it deserves to be weighed rather than
cited.

### Flix

Flix has no null. [doc.flix.dev/arrays.html](https://doc.flix.dev/arrays.html):
"Flix does not have a `null` value, but one can be indirectly introduced by
reading from improperly initialized arrays which can lead to
`NullPointerException`s." That is a JVM interop leak, not a language feature.

`Option[t]` is an ordinary enum, `main/src/library/Option.flix:19-29`:

```
enum Option[t: Type] with Eq, Order, ToString { case None, case Some(t) }
```

**And you can put it in a Datalog relation.** The constraint on a relation
column is an `Order` instance, enforced by `Fixpoint3/Boxable.flix`:
`pub def box(x: a): Boxed with Order[a]`. `Option` derives `Order`, so it
qualifies. Proven live in the test suite, `main/test/flix/`:

```
// Test.Exp.Fixpoint.Constraint.flix:76-79
def testOptionConstraint01(): #{A(Option[Int32]), R(Option[Int32])} = #{
    A(None). A(Some(21)). A(Some(42)).
    R(x) :- A(x).
}
```

and a full recursive transitive closure over `Option`-typed columns, mixing
`None` and `Some` as edge endpoints, at `Test.Exp.Fixpoint.Solve.flix:104-114`,
with the same pattern repeated for `Result[String, String]` immediately after.

So Flix is the existence proof for candidate A as a *first-class column type*
rather than as N variant relations: a sum type in a datalog column, joined and
recursed over, with no null anywhere. The cost it pays is one this project does
not currently pay: every column type carries an `Order` instance, and the
storage is a boxed value, not a SQL column.

**Public regret about Option in Datalog being awkward: NOT FOUND.** Searched;
nothing surfaced. Absence of a complaint is not proof of absence.

### Datafun

Arntzenius and Krishnaswami, "Datafun: a functional Datalog", ICFP 2016,
[cl.cam.ac.uk/~nk480/datafun.pdf](https://www.cl.cam.ac.uk/~nk480/datafun.pdf).

Sum types are in the core grammar (`A, B ::= 2 | N | str | {A} | A + B | A × B`),
so `Option[A]` is expressible as `1 + A`. **The words "Option" and "Maybe" do
not appear in the paper**, so this is general machinery, not a named idiom.

The finding that matters for an incremental engine is the *order*. Section 2,
verbatim:

> "Sum types are ordered disjointly: in_i a ≤ in_i b iff a ≤ b, but in1 a and
> in2 b are never comparable."

The injection boundary is unconditionally incomparable. `None` and `Some(x)`
can never be ordered either way, whatever is inside.

Datafun draws a consequence from this that is worth carrying. Section 4.3:

> "While TRUE and FALSE are straightforward, there are two rules for boolean
> elimination, IF and IF+. This is because in Datafun, 1 plus 1 does not equal
> 2: booleans are not a sum of units. At the type 1 + 1, in1⟨⟩ and in2⟨⟩ are
> incomparable. But in Datafun, true > false."

Datafun refused to encode booleans as `1+1` *because* the disjoint sum order
makes the encoding useless for a monotone `if`. It gave booleans their own
primitive totally-ordered type instead.

Read against this project: the `Some` to `None` transition is not a monotone
step under any sum order, so it can only ever be a retract-then-assert pair.
That is exactly what the tick model already does (`-old` then `+new`), so
candidate A is compatible with the tick model *because* its transitions are
already modeled as sign decomposition rather than as in-place update. This is a
point in candidate A's favour that nobody had stated. It is also a warning
about candidate B: a nullable *column* invites the reader to think of a null to
value change as an update to a cell, and the engine has no such operation.

Datafun's sum elimination splits into `CASE` (subject discrete, branches
unrestricted) and `CASE+` (subject monotone, branches must be monotone in the
bound variable), the same discipline any `Option` match would need. Anything
richer than disjoint union, for example an order where `None ≤ Some(x)`, is
explicitly future work in the paper.

No statement about missing values, partiality or absence as a data-modeling
concern: NOT FOUND. The paper's only "partial" discussion is about partial
*functions* and general recursion.

### SQL's own regret

Hugh Darwen, "How To Handle Missing Information Without Using NULL"
(presentation, Warwick University, 9 May 2003, updated 27 September 2006,
[dcs.warwick.ac.uk/~hugh/TTM/Missing-info-without-nulls.pdf](https://www.dcs.warwick.ac.uk/~hugh/TTM/Missing-info-without-nulls.pdf)),
title page, verified by direct extraction:

> "'Databases, Types, and The Relational Model: The Third Manifesto', by C.J.
> Date and Hugh Darwen (3rd edition, Addison-Wesley, 2005), contains a
> categorical proscription against support for anything like SQL's NULL, in its
> blueprint for relational database language design."

A slide in the same deck is titled "SQL's 'Nulls' Are A Disaster" and cites
Date's *Relational Database Writings* volumes and *An Introduction to Database
Systems*, 8th edition, as the primary sources.

The mechanics, from PostgreSQL's docs rather than the paywalled standard.
[§9.2 Comparison Functions and Operators](https://www.postgresql.org/docs/current/functions-comparison.html):

> "Ordinary comparison operators yield null (signifying 'unknown'), not true or
> false, when either input is null. For example, `7 = NULL` yields null, as
> does `7 <> NULL`."

[§9.24.3 NOT IN](https://www.postgresql.org/docs/current/functions-subquery.html):

> "Note that if the left-hand expression yields null, or if there are no equal
> right-hand values and at least one right-hand row yields null, the result of
> the `NOT IN` construct will be null, not true."

That second one is the same defect as receipt D3, in the vendor's own words.

A correction to the null-coherence lab's citation: it points at
[sqlite.org/nulls.html](https://sqlite.org/nulls.html) for the "arbitrary and
puzzling" line, which is right, but that page is about `DISTINCT`/`UNION`/
`UNIQUE` treatment across engines, not about the identity or `NOT IN`
behaviour. For those, PostgreSQL's §9.2 and §9.24.3 above are the correct
sources.

### One terminology trap, stated because a reader will hit it

"Labeled nulls" in the datalog literature are **not** SQL nulls. They are
existential Skolem placeholders invented by the chase algorithm to fill target
positions no source value determines. Fagin, Kolaitis, Miller, Popa, "Data
Exchange: Semantics and Query Answering", *Theoretical Computer Science* 336
(2005), 89-124, section 2.1: "we assume an infinite set Var of values, which we
call labeled nulls, such that Var ∩ Const = ∅". A value-generation device
inside otherwise ordinary null-free instances. Anyone searching "datalog null"
for prior art will land on this literature and it is about a different problem.

**Has anyone who added SQL-style nulls to a datalog-family system written about
the consequences? NOT FOUND.** None of Datomic, Flix or Datafun did it. No
post-mortem was located. That gap is named here rather than filled.

---

## 8. The explosion, measured

The user's stated worry: "that is a rel that will explode in values lmfao bc of
how rel to table works". Nobody had put a number on it. Here is the number.

Shape: N subjects, 90 percent have the optional value, 10 percent do not. Every
schema is the DDL the compiler actually emits for that relation shape, taken
from the receipt C compiler output, not hand-drawn. Both engines agree on every
row count, table count and plan.

### At 100,000 subjects

| candidate | rows | tables | bytes | vs baseline |
|---|---:|---:|---:|---:|
| nullable column, key declared | 100,000 | 1 | 2,949,120 | 0.96x |
| two relations, status quo | 100,000 | 2 | 3,059,712 | 1.00x |
| Option, three variant relations | 200,000 | 3 | 4,800,512 | 1.57x |
| nullable column, derived relation | 100,000 | 1 | 6,545,408 | **2.14x** |

### At 10,000 subjects, same ordering

| candidate | rows | tables | bytes |
|---|---:|---:|---:|
| nullable column, key declared | 10,000 | 1 | 286,720 |
| two relations, status quo | 10,000 | 2 | 299,008 |
| Option, three variant relations | 20,000 | 3 | 471,040 |
| nullable column, derived relation | 10,000 | 1 | 614,400 |

### The result inverts the worry

The user's fear was that Option explodes. It does: rows double, because every
subject carries one variant row plus one tag row. On disk that is 1.57x, which
is a real cost and a modest one.

The thing nobody was worried about is worse. **A nullable column on a derived
relation is the largest of the four at 2.14x, with flat row counts.** The
reason is receipt D1: it is the only candidate forced off the single-btree
`WITHOUT ROWID` family and onto a rowid table plus a `UNIQUE` autoindex that
duplicates every column. Two btrees instead of one.

And the smallest of all four is a nullable column *when a key is declared*, at
0.96x, because then the nullable column sits outside the primary key and
`WITHOUT ROWID` survives. That is the best case and it is unavailable exactly
where the argument for nullability is strongest, because `keyed_level_head`
makes derived relations unkeyable.

So the storage answer is conditional on a refusal that has nothing to do with
nulls, and any storage claim about Design D that does not state which of the
two families it means is meaningless. That is the most useful number in this
section.

### Plans, since the repo law asks for EXPLAIN not reasoning

The optionality-**blind** read, the one written by code that does not know the
field is optional. All four reach it by index SEARCH at 100k, both engines:

```
two-rel          SEARCH repo_with_latest USING PRIMARY KEY (repo=?)
option-variants  SEARCH repo_latest_some USING PRIMARY KEY (repo=?)
nullable derived SEARCH repo_latest USING COVERING INDEX sqlite_autoindex_repo_latest_1 (repo=? AND commit>?)
nullable keyed   SEARCH repo_latest USING COVERING INDEX sqlite_autoindex_repo_latest_1 (repo=? AND commit>?)
```

Note SQLite rewriting `commit IS NOT NULL` into the range constraint
`commit>?` and still using the index. No candidate loses the blind read.

The optionality-**aware** read, full enumeration:

```
two-rel          COMPOUND QUERY | LEFT-MOST SUBQUERY | SCAN repo_with_latest | UNION ALL | SCAN repo_without_latest
option-variants  COMPOUND QUERY | LEFT-MOST SUBQUERY | SCAN repo_latest_some | UNION ALL | SCAN repo_latest_none
nullable         SCAN repo_latest USING COVERING INDEX sqlite_autoindex_repo_latest_1
```

One scan instead of two, for the nullable forms. That is the read-side win and
it is one B-tree traversal, not an asymptotic difference.

Candidate D adds nothing to storage at all; its read is
`SCAN r` plus `SEARCH l USING PRIMARY KEY (repo=?) LEFT-JOIN`, receipt G.

### The compile-time explosion nobody measured either

**Receipt C6b.** The incremental emitter mints scratch tables and indexes *per
relation*, whether or not the relation is ever populated. Measured across the
two compiled programs: each extra relation costs 1 scratch table and 1.5
indexes on average. Two-rel emits 14 scratch tables and 12 indexes; the
three-variant program emits 16 and 15. So the variant explosion is not only
rows; it is also a fixed per-relation compile-time cost paid at every program
load.

---

## 9. Tier-0 classification, all candidates

Applying the test from
[2026-07-30-rel-as-stream-lab.md](2026-07-30-rel-as-stream-lab.md) section 3:
(a) sugar over an existing lowering, (b) a new lowering of existing semantics,
(c) tier 0, a genuinely new semantic no lowering over current constructs can
express.

| # | construct | class | reasoning |
|---|---|---|---|
| 1 | three-variant json read, `json_type` classifier | **none** | ships. Receipt V1 executes it on both engines with no new anything |
| 2 | `Some`/`None`/`Undefined` as three variant relations, world-fed | **(a)** | enum expansion plus derived tag view, both shipped. Receipt C5 compiles it |
| 3 | the same, produced by a derived rule | **(b)** | refused by `keyed_level_head` plus `content_key_positions/2`. Both are lowering choices. No semantics is missing. Receipt C2 |
| 4 | Option as a first-class *column type* (Flix's shape) | **(c)** | a column domain gains a tagged value. Distinct from row 2, which spends relations instead |
| 5 | row absence, two relations and a join | **none** | ships. Receipt C1 |
| 6 | use-site defaulting, `get_else` | **(b)** | lowers to `LEFT JOIN` plus `coalesce`, both already emitted. Adds nothing to any column domain. Receipts G1, G2 |
| 7 | `T?` nullable column with total equality, Design D | **(c)** | a column domain gains a value in no current domain, and the one-row outer result is a row shape no total fixed-arity relation can express |
| 8 | SQL 3VL in rule comparisons | **(c)** | a third truth value enters rule solving |
| 9 | nullable columns in a *keyed* relation only | **(c)**, cheaper | still tier 0 in the value domain, but receipts D1, X show it keeps the `WITHOUT ROWID` family and stays the smallest of four. A narrower tier-0 than row 7 |

**Tier 0 is not empty here**, unlike the rel-as-stream lab. Rows 4, 7, 8 and 9
are all genuinely new semantics. Rows 1, 2, 3, 5 and 6 are not, and between
them they cover the user's json question completely.

---

## 10. The question under the question

**Does json force the issue?**

Stated plainly: **no, not for the read.** Measured, not argued.

The forcing-function story goes: json values are stored as TEXT (they are;
jsonb is not portable across the two builds this project runs, measured in the
prior lab), they are read through decode into typed columns, so every decode of
a possibly-absent key needs a destination, and a destination that can be
"nothing" is a null.

Receipt V1 breaks the chain at the third step. A decode of a possibly-absent
key does not need *a* destination; it needs *three*, and `json_type/2` already
tells you which one, totally, in one expression, identically on both builds.
Three total destinations is the enum machinery this project already ships.
Receipts V2 and C5 show the rows landing in it.

So the json argument for nulls evaporates. What does not evaporate:

1. **Ergonomics at the consumer.** Three relations means three arms. That cost
   is real, it is per-consumer, and it is paid forever. It is not a json
   argument; it is the same argument the two-relation status quo has always
   faced, and json neither strengthens nor weakens it.

2. **The derived case is refused.** Receipt C2. Today the user's idea only
   works arrival-fed or edge-fed. A json read is usually derived. This is the
   one place the json question genuinely bites, and it bites a lowering
   choice, not a semantics gap.

3. **`int` columns have no spare sentinel.** Receipt G3. Neither the variant
   encoding nor Design D has this problem; only candidate D does. Worth naming
   because candidate D is otherwise the cheapest.

4. **`Undefined` may be a distinction nobody wants.** Nothing in this lab shows
   any program needing to tell "key absent" from "key present and null". The
   three-variant read *can*; whether the fixture corpus ever *does* is an
   unmeasured question and it is card 6.

---

## 11. Numbered cards, branches left OPEN

Each card names at least two bounded options and the evidence that would close
it. **None is marked recommended.** That is deliberate; the brief was to hand
back a decision, not to take one.

**Card 1. Does the three-variant json read ship as the decode contract?**
- A: `json_type`-driven three-way classification, `Some`/`None`/`Undefined` as
  three variant relations. Ships today world-fed (V1, V2, C5).
- B: two-way, presence plus value, collapsing `None` and `Undefined`. Cheaper,
  and it is what the current lowering already does wrong (V5).
- C: keep `json_extract(...) IS NOT NULL` and accept the collapse.
- *Closes on*: a grep of the fixture corpus and of ghcacher for any program that
  distinguishes an absent key from a json null. If zero, B gets much cheaper.
  **OPEN.**

**Card 2. `keyed_level_head` on generated variant relations.**
- A: leave it. Options are arrival-fed or edge-fed only, and the outer-join
  shape stays two relations.
- B: exempt generated variant relations, since their key is synthesized by the
  expansion and not declared by the author.
- C: change `content_key_positions/2` so variant relations key on the `id`
  discriminator instead of on content, which would also close E1.
- *Closes on*: writing the derived-Option fixture and grading oracle against
  emitter under each of the three. C changes an existing shipped encoding and
  needs a regrade of the two enum fixtures. **OPEN. This is the highest-value
  card in the lab**, because it is the only thing standing between the user's
  own idea and a working derived Option, and it is class (b), not tier 0.

**Card 3. E1 and E2, the unenforced Option invariants.**
- A: accept. Exhaustiveness is a `match`-site concern and storage stays dumb.
- B: refuse at load, a checked "at most one variant per discriminator".
- C: key variant relations on the discriminator, which makes A impossible by
  construction and subsumes card 2 option C.
- *Closes on*: whether any shipped program can produce two variants for one
  discriminator. Receipt E2 shows the storage permits it; nothing shows a
  program does it. **OPEN.**

**Card 4. If Design D is taken, which table family?**
- A: `__id INTEGER PRIMARY KEY` plus `UNIQUE`, which is 2.14x storage (X) and
  loses set identity on null-bearing rows (D2).
- B: keep `WITHOUT ROWID` and refuse nullable columns outside declared keys,
  which makes derived relations unable to carry one at all (D1 plus
  `keyed_level_head`).
- C: a nulls-not-distinct uniqueness mechanism, which SQLite's default UNIQUE
  cannot implement and which would need a generated expression index or a
  sentinel-in-the-index encoding.
- *Closes on*: measuring C. It is the only unmeasured option and it is the only
  one that keeps both set identity and the derived case. **OPEN, and this lab
  considers it a blocking gap in the Design D plan, not a detail.**

**Card 5. Scope of the null-safe equality rewrite.**
- A: type-directed, only columns typed `T?` get `IS NOT DISTINCT FROM`.
- B: blanket rewrite of all five equality families in `lower.pl`, since D4
  shows the planner does not care.
- C: null-safe only in the identity-critical paths (negation, key lookup,
  support, boundary diff) and plain `=` in rule comparisons.
- *Closes on*: D4 already removes the performance objection to B; what remains
  is whether a blanket rewrite loses the reader's ability to see which columns
  can be null in the emitted SQL, which is a readability call on a stated
  project value (predictable emitted SQL). **OPEN.**

**Card 6. Is `Undefined` a distinction any real program wants?**
- A: yes, keep three variants.
- B: no, `None` and `Undefined` merge and the enum has two variants, which is
  ordinary `Option`.
- C: make it per-decode, an author writes two variants or three.
- *Closes on*: the corpus grep in card 1, plus a reading of ghcacher's real
  GitHub responses for a field that is present-and-null versus absent. GitHub's
  API does emit both. **OPEN.**

**Card 7. Optionality in the aggregate or at the use site?**
- A: in the relation declaration, Design D's `T?` or candidate A's variants.
- B: at the use site, `get_else`, which Hickey argues for directly and Datomic
  ships.
- C: both, with the declaration total and the use site defaulting.
- *Closes on*: whether the `int`-sentinel hole (G3) can be closed at the use
  site. It probably cannot without a null, which would make B a partial answer.
  Worth deciding anyway because B is class (b) and A row 7 is class (c).
  **OPEN.**

**Card 8. The refusal message for the first Design D keystroke.**
- A: fix it whether or not Design D lands, since C7 shows the parser emits a
  raw character-code dump through swipl's Unknown-message fallback.
- B: leave it, since it is a parse error for a spelling that does not exist.
- *Closes on*: nothing; this is a small decision. Recorded because it is design
  review finding B4 caught in the wild and because it is the exact first
  experience of anyone trying the recommended design. **OPEN, small.**

**Card 9. Does a sum type belong in a column at all, or only in relations?**
- A: relations, the shipped enum encoding, N tables plus a tag view.
- B: a column type, Flix's shape, `Option[Int32]` as a boxed ordered value.
- C: both, with the column form as sugar that expands to the relation form.
- *Closes on*: whether the storage plane can hold a tagged value without
  reintroducing the compound-inline punt the C2 typed-columns ruling made.
  `compound_storage = struct_as_rows` already ruled the general case toward
  rows, which points at A, but that ruling was about structs and did not
  consider sums. **OPEN, and it may already be answered by a ruling nobody has
  read against this question.**

---

## 12. What this lab would want to measure next

Not a recommendation. Measurements, in the order they would most change a
decision.

1. **Card 4 option C, nulls-not-distinct uniqueness in SQLite.** Two candidate
   encodings, a generated expression index over
   `coalesce(column, sentinel_blob)` and a partial-index pair. Measure both for
   correctness under `INSERT OR IGNORE`, plan quality, and behaviour on both
   builds. This is the only unmeasured branch that could make Design D cheap,
   and everything in the Design D plan is downstream of it.

2. **The corpus grep for cards 1 and 6.** How many programs in the 139-fixture
   corpus, plus ghcacher, plus the v5 examples, would distinguish absent from
   null if they could. If the answer is zero the whole three-versus-two variant
   question collapses and card 1 option B gets much cheaper.

3. **Derived Option under each of card 2's three options.** Write the fixture,
   grade oracle against emitter, count the statements per tick for a
   `Some`-to-`None` transition. Receipt E3 says four writes across three tables
   by hand; the emitted number under the incremental path is unmeasured and it
   is the number that decides whether the variant encoding is affordable at
   ingest scale.

4. **The `Undefined` question against real GitHub payloads.** ghcacher already
   fetches real responses. Count fields that arrive present-and-null versus
   absent across a real sample. This is cheap and it is evidence rather than
   speculation for card 6.

5. **Whether Flix's column-typed sum has a storage shape here at all.** Card 9.
   The `compound_storage = struct_as_rows` ruling ruled structs toward rows; a
   sum is not a struct and the ruling's reasoning may or may not carry. Reading
   the ruling against this question is a one-sitting job and it might close
   card 9 with no measurement at all.

6. **The `Some`-to-`None` transition through the real incremental emitter,
   under memory soak.** Every candidate makes that transition differently:
   candidate A moves a row between tables, Design D rewrites a column in place,
   candidate D writes nothing. Under the existing memory-soak harness, at
   sustained churn, those are three different pressure profiles and none has a
   number.

---

## Verification

```
git rev-parse HEAD
d3cb5eeaa0fcc9d9f3963ebe28c294843cfd14bc

SPREFA_CONFIG=/nonexistent/option-versus-null.toml \
DL_NO_DAEMON=1 \
node plans/2026-07-30-option-versus-null-receipts.mjs

sqlite3 CLI     = 3.43.2
@libsql SQLite  = 3.45.1
PASS 83 assertions, both SQLite builds
```

Every database is created fresh under the operating system temporary directory
and removed on exit. `~/.local/state` is never read or written. No daemon is
started. The compiler is invoked read-only, on scratch input files written to
the temporary directory, through the repository's own
`v6/prolog/compile/scripts/compile_dl6.sh`.

A worktree cut for a lab has no `node_modules`, so the receipts resolve
`@libsql/client` from the primary checkout's install of the same locked
version, or from `DL_LIBSQL_FROM` if set. Resolution is read-only.

## Staffing

- Work type: research lab, analysis and receipts only
- Worktree: `sprefa-lane-optionlab`, branch `lane/option-vs-null`
- Base SHA: `d3cb5eeaa0fcc9d9f3963ebe28c294843cfd14bc`
- Production implementation: unstaffed; every card is open
- Executed suite: 83 assertions across 6 parts, both SQLite builds
- Git actions: one commit on the lab branch. No merge, no push
