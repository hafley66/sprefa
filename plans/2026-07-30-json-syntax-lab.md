# JSON syntax lab -- grammar, lowering, list types, cards

Lane `lane/json-syntax-lab`, base `6108cf85`. **Zero production edits.**
Everything executable is under `v6/prolog/labs/json_syntax/`; this doc is the
only other file.

    swipl -q -l v6/prolog/labs/json_syntax/0_receipts.pl -g go -g halt
    # JSON_SYNTAX_LAB 25 PASS (grammar 7, lowering 7, lists 7, cards 4)

Inputs: `plans/2026-07-30-json-query-language-recovery.md` (the archaeology),
`plans/2026-07-30-json-interop-lab.pl` (the current-world record), the locked
`single_rel_type_system`, `v6/prolog/conformance/body.pl`, and the three
directives ruled in `chat_log/20260730.1.fable-opus-storm-lab-assimilation.pl`.

**No syntax lands here.** The user sees exact spellings on cards in §5 before
any parser is touched.

---

## 1. Grammar draft

Eleven productions. v5 shipped nine; the two extra are the literal/pattern
split, which v5 did not need because v5 had no construction plane.

    ── LITERAL (constructing) ──
    jsonlit  = objlit | arrlit | scalar
    objlit   = "{" [ pairlit {"," pairlit} [","] ] "}"
    pairlit  = keylit ":" jsonlit
    keylit   = IDENT | STRING
    arrlit   = "[" [ jsonlit {"," jsonlit} [","] ] "]"
    scalar   = INT | FLOAT | STRING | "true" | "false"

    ── PATTERN (matching) ──
    jsonpat  = objpat | arrpat | hole | scalar        % scalar = equality filter
    objpat   = "{" [ pairpat {"," pairpat} [","] ] "}"        % OPEN
    pairpat  = keypat ":" jsonpat
    keypat   = IDENT | STRING | keyhole | "**" | GLOB | "re:" REGEX
    arrpat   = "[" "..." jsonpat "]" | "[" [jsonpat {"," jsonpat}] "]"

Two laws fall out, and neither is a preference:

**(a) The literal grammar IS the pattern grammar minus holes.** One `{`, two
roles. `1_grammar.pl` proves it by construction: `parse_literal/3` is
`parse_pattern/3` plus a hole-free check, not a second DCG (receipt R4).

**(b) Quoting is the literal marker on the VALUE plane; bareness is the literal
marker on the KEY plane.** This is forced by JSON5, not chosen: JSON5 permits
unquoted keys and forbids unquoted string values, so the value slot is free for
dl6 variables and the key slot is not. Consequence: **every key-axis production
is pattern-only, forever.** Constructing an object with a computed key is
`json_object(K, V)`, never a brace literal.

### Where each may appear

| form | head args / VALUES rows | rule body | host output columns | query results |
|---|---|---|---|---|
| object/array/scalar literal | yes | yes (equality filter) | yes | rendered |
| value hole | no (heads are total) | yes | no | -- |
| key hole `$k` / `**` / glob / `re:` | **never** | yes | **never** | -- |
| array spread `[... p]` | never | yes | never | -- |

Edge bodies stay behind the existing named refusal
(`edge_body_needs_json_destructure`) -- see card CARD-EDGE-BODY-JSON.

### The one place the dialects disagree

`1_grammar.pl` parses both the shipped v5 surface and the dl6 spelling law, to
one IR. They agree everywhere except a **bare run in value position**:

    { state: open }     v5  -> pat_eq(open)     (literal)
    { state: open }     dl6 -> pat_hole(open)   (variable)
    { state: 'open' }   dl6 -> pat_eq(open)
    { stars: 4 }        both -> pat_eq(4)       (numbers/bools never diverge)

Receipt R5. The migration is total because JSON5 forbids unquoted string values
anyway, so dl6's reading costs no JSON5 compatibility.

### Acceptance case (receipt R2)

The gh-cache flagship (`examples/gh-cache.dl:116-117`) parses **verbatim**, and
its dl6 transcription yields the byte-identical IR term:

    [... { number: $num, title: $title, state: $state, user: { login: $author } } ]
    [... {number: num, title: title, state: state, user: {login: author}}]
      both -> pat_arr_spread(pat_obj([kp(k_exact(number), pat_hole(num)),
                                      kp(k_exact(title),  pat_hole(title)),
                                      kp(k_exact(state),  pat_hole(state)),
                                      kp(k_exact(user),
                                         pat_obj([kp(k_exact(login), pat_hole(author))]))]))

27 verbatim archive examples parse (R1), spanning v1 (`archive-20260428`), v3,
v4 and v5, including `${N?}`, `$$`-free quoted `"$ref"` keys, `re:` keys, globs,
`**`, nested key fan-out and array spread.

### JSON5 subset the draft takes

Taken: unquoted keys, trailing commas, `#` comments (already the dl6 comment),
both quote characters. Excluded, with reasons: `null` (no such language value --
card `null_and_optional`), `NaN`/`Infinity` (the float ruling is finite-only),
hex/leading-plus/leading-dot numbers (the dl6 number lexer), escapes inside
identifiers.

### Keeping `{` extensible (directive: "it will later be abused beyond json")

Recommendation on a card: reserve `Tag{...}` now. The seam is **already free in
the prolog term form** -- swipl reads `point{x: 1}` as a native dict, a term
shape that cannot unify with `{}/1` (receipt R7). So the later non-json brace
form has a home that does not move the json spelling, and reserving it today
costs exactly one refusal clause.

---

## 2. Type + lowering draft

### Storage

`json` columns store **TEXT with a `json_valid` CHECK**. Not `jsonb`: the two
SQLite instances this project already runs disagree about whether `jsonb`
exists. System `sqlite3` is 3.43.2 and rejects it (receipt L6); the `@libsql`
driver bundles 3.45.1 and accepts it (measured out of band). A storage decision
cannot depend on a function only one of them has, and the rust flip adds a
third.

`column_storage(_, json, text)` already exists in `0_type_plane.pl:72`. The
directive does not change the storage kind; it changes what the compiler is
allowed to *do* with it.

### The coexistence rule

> **The brace pattern's lowering is a function of the SOURCE COLUMN'S DECLARED
> TYPE, never of the pattern.**

    rel resp(ep: text, body: json).            -- body: json  -> json1 plan
      decode(body, {number: num})   ==>  json_extract(body, '$.number')

    rel diag(where: place, message: text).     -- where: place -> dictionary join
      decode(where, {file: file})   ==>  '__dict_place'(where, file, _)

One surface, two lowerings, picked by the decl. The declared-struct path is
`lower.pl expand_decode_rules/4`, **unchanged**. `json` is the explicitly
dynamic escape, and it is the only place the key axis is even meaningful,
because a declared struct has no unknown keys (`decode_field_unknown` exists
precisely to say so).

### Tick-log contract: does not move

`canonical_json_text/2` stays the single canonicalizer. Receipt L7 measures why
it has to: `json()` minifies but **preserves key order**, and
`json_group_object` follows row order, so json1 will not canonicalize for us at
any point in the pipeline. An explicit `ORDER BY` is what buys canonical order
at the SQL boundary. The cross-target log contract is untouched by this design.

### Three lowering receipts (SQL emitted by `2_lowering.pl`, executed against
system sqlite3, rows asserted)

**L1 -- exact keys, flat + nested, ONE row, ZERO joins.**
`{ number: $n, user: { login: $a } }` (`src/datapath.rs:1502-1509`):

```sql
SELECT json_extract(b0."body", '$.number') AS "n",
       json_extract(b0."body", '$.user.login') AS "a"
  FROM resp b0
 WHERE json_extract(b0."body", '$.number') IS NOT NULL
   AND json_extract(b0."body", '$.user.login') IS NOT NULL;
```

Exact-key members cost no join: descent walks the path inside one
`json_extract`, which is v5's "leaves join, descents fan out"
(`v3 walker.rs:1-8`) restated in SQL. The `IS NOT NULL` conjuncts are what make
a missing key a silent non-match (`missing_key_yields_no_match`).

**L2 -- THE FLAGSHIP. Array-of-objects fan-out with sibling correlation.**
`[... { number: $num, title: $title, state: $state, user: { login: $author } } ]`
(`examples/gh-cache.dl:116-117`):

```sql
SELECT json_extract(e0.value, '$.number') AS "num",
       json_extract(e0.value, '$.title') AS "title",
       json_extract(e0.value, '$.state') AS "state",
       json_extract(e0.value, '$.user.login') AS "author"
  FROM resp b0, json_each(b0."body") e0
 WHERE e0.type = 'object'
   AND json_extract(e0.value, '$.number') IS NOT NULL
   AND json_extract(e0.value, '$.title') IS NOT NULL
   AND json_extract(e0.value, '$.state') IS NOT NULL
   AND json_extract(e0.value, '$.user.login') IS NOT NULL;
```

One join, one row per element, siblings correlated. The whole ghcacher
`pull_request` row, which the recovery doc graded **(c) blocked on storage**.

**L3 -- KEY CAPTURE, the highest-leverage (d) row.**
`{ $key: $value }` (`examples/type-from-json.dl:25`):

```sql
SELECT e0.key AS "key",
       e0.value AS "value"
  FROM sample b0, json_each(b0."body") e0
 WHERE e0.value IS NOT NULL;
```

**`json_each` already yields `(key, value)`.** The construct the recovery doc
graded "no v6 spelling, single most-used v5 form" needs **zero new SQL
machinery** once `body` is a json column. Its entire remaining cost is a
spelling.

**L4 (bonus) -- `**`, and the path v4 wanted and v5 dropped.**
`{ **: { image: $i } }` lowers to `json_tree`, and the *same join* already
carries `fullkey`:

```sql
SELECT json_extract(t0.value, '$.image') AS "i", t0.fullkey AS "path"
  FROM doc b0, json_tree(b0."body") t0
 WHERE t0.type = 'object'
   AND json_extract(t0.value, '$.image') IS NOT NULL;
-- rows: deep|$.a.b
```

v4's `$$${PATH?}` -- the one construct the recovery doc lists as
dropped-with-no-successor -- is a column that is already there.

### The emitted `type` guard is load-bearing

`json_each`/`json_tree` hand back SQL values, so `value` is JSON text for
containers and a bare scalar for leaves. Descending into a leaf is **not** a
silent non-match in SQLite: `json_extract` raises `malformed JSON` and kills the
whole statement. The emitted `e0.type = 'object'` guard is what preserves v5's
silent-non-match semantics. This bit the lab on first run and is the kind of
thing a paper design would have shipped.

### Cost table, in joins

| construct | joins | mechanism |
|---|---|---|
| exact key, any depth | 0 | `json_extract` path |
| array spread | 1 | `json_each` |
| key capture `$k` | 1 | `json_each` (`key`,`value`) |
| key wildcard `$_` | 1 | `json_each` |
| glob key | 1 | `json_each` + `GLOB` (**core SQLite**) |
| regex key `re:` | 1 | `json_each` + `REGEXP` (**not core**) |
| `**` descent | 1 | `json_tree` |
| `**` + path bind | 1 | `json_tree.fullkey` (same join) |

Statement counts stay flat per rule -- no per-arrival loop, no per-element
statement.

---

## 3. List types

### Verdict

> **json is the array carrier; `list(T)` is a typed view over it. Relational
> element storage is not a list, it is a rel.**

This is the recovery doc's implied `host_flattened` and the directive's json
column type saying the same thing twice: *arrays fan out into rows when you
query them, and are carried as canonical json text when you store them.* Five
generations never once stored an array (recovery doc §array_storage), and the OG
coordinate model had extrinsic ids available and still flattened at extraction.

### Grading, 3 options x 5 axes (`3_lists.pl`, every cell has a receipt)

| axis | cons cells | indexed rows | **json carrier** |
|---|---|---|---|
| content identity | ok -- chain hash works, tails shared, every element interned | poor -- whole-array identity needs a separate rule; the hash people reach for is the canonical text | **best** -- the canonical text *is* the id, and it is already the log contract (T1) |
| retraction / refCount | poor -- N rows + N refCount edges per list | ok -- one `DELETE WHERE array_id=?`, but the N rows exist | **best** -- one column in one row; zero cascade, and the cycle question cannot arise in text (T2) |
| aggregate heads | poor -- ordered chain in SQL is a recursive CTE producing N rows | ok -- one `INSERT..SELECT` with `row_number()`, but the head value is then an id needing interning | **best** -- `json_group_array` is a native aggregate: one statement, one value per group (T3) |
| tick-log contract | poor -- recursive CTE per value, or 1000 memo rows for a 1000-element list | ok -- one grouped read with explicit `ORDER BY` | **best** -- storage *is* the contract; render is identity, zero joins (T4) |
| 0/1/many | best -- `nil` is a real value | poor -- empty array is zero rows, indistinguishable from absent without a header | **best** -- `[]` is a value and is not absence (T5) |

Measured (T2): one 1000-element list occupies **1 row** as a carrier, **1001**
as indexed rows (1000 elements + 1 header), **1000** as cons cells. All three
render byte-identically to `[10,20,30]` (T4) -- the difference is entirely what
each pays to get there.

The carrier loses exactly one thing: **per-element sharing and independent
retraction**. That is the axis relational storage exists for, and the answer is
that such a thing is not a list -- it is a rel with an index column, declared as
such. Two planes, stated once.

### Generics

**`list(T)` as the only parametric type, T over a closed set of four scalar
types.** Measured checker delta (T6), and it is the whole delta:

1. one `column_storage/3` clause -- `list(T)` stores TEXT
2. one element guard -- `T` in `{int, text, bool, float}`
3. one named refusal `list_of_relation_refs(Rel)` -- ids in a list would enter
   the tick log, breaking the print-values-not-ids ruling
4. one named refusal `list_element_not_scalar(list(_))` -- nesting is what the
   `json` type is for

There is **no type variable, no unification, no instantiation**: T ranges over a
closed four-element set. That is why `list(T)` can be the only parametric type
without dragging generics into the checker.

**Sharp finding (T6):** SQLite can enforce *array-ness* as a column CHECK
(`json_valid(c) AND json_type(c)='array'`, verified) and **cannot** enforce the
element type -- CHECK constraints prohibit subqueries and `json_each` is a table
function. Element typing is a checker / emitted-guard obligation, never a
storage constraint.

---

## 4. Card reconciliation

### Answered by the directives (9)

| card | origin | by directive | answer |
|---|---|---|---|
| CARD-KEY-CAPTURE | recovery | json_syntax_native | holes ARE the directive; lowering is `json_each(key,value)`, zero new SQL (L3). Residue = one spelling. |
| CARD-ARRAY-FANOUT | recovery | json_as_rel_type | flagship executes as one `json_each` join (L2). Lifting the compiler refusal is a dispatch. |
| CARD-CONSTRUCTION | recovery | json_as_rel_type | "lowers to sqlite json1" *is* `json_group_array`/`json_group_object`; already ruled emittable. Dispatch. |
| CARD-RECURSIVE-KEY | recovery | json_syntax_native | `**` was live v1..v5; lowering is `json_tree`, and `fullkey` also supplies v4's dropped `$$${PATH?}` (L4). |
| **CARD-SUBTREE-CAPTURE** | recovery | json_as_rel_type | **the project's oldest open json question closes.** A value-position hole with no sub-pattern already meant "bind, do not descend"; what v5 lacked was a typed place to put the subtree. `human-goals.md:693` answered. |
| CARD-PATTERN-KEY (glob half) | recovery | json_syntax_native | glob key = one join + SQL `GLOB`, core SQLite (L5). |
| json_residency | interop | json_as_rel_type | **core_global.** A column type plus a base-grammar literal is as core as a construct gets. |
| array_storage | interop | list_types_and_generics | the four options collapse to the two-plane statement of §3. |
| recursive_identity | interop | json_as_rel_type | **split, not chosen:** `refuse_cycles` stands for ref columns; a json column is acyclic by construction because text cannot cycle. |

### Still open (13)

Four inherited, nine created by the directives themselves. Ordered by leverage
in §5.

Two carry no spelling to pick and are scheduling, not design:
`CARD-EDGE-BODY-JSON` (half answered -- the SLOT-TERM-STRUCT encoding half is
answered by json columns storing canonical JSON text; the frontier-staging half
is a runtime arc) and `schema_import_boundary` (blocked on the same session's
`openapi_codegen_spine` directive, which **inverts** the direction: if the spec
is generated from prolog facts, the import half may never be needed).

---

## 5. The card list for user sign-off

Every card below carries at least two exact spellings, gated by receipt C3.
Full text and cost notes: `v6/prolog/labs/json_syntax/4_cards.pl` (`spelling/4`).

**1. CARD-KEY-HOLE-SPELLING** -- unblocks 4 of the 5 (d) rows. In key position a
bare identifier is a literal label today; what marks a key as a variable?

    (a) sigil        decode(body, {$key: $value})
    (b) parens       decode(body, {(key): value})
    (c) brackets     decode(body, {[key]: value})
    (d) invert       decode(body, {key: value})   and a LITERAL key becomes {'name': value}

(a) has five generations of precedent and `$` appears nowhere else in dl6.
(d) is the only option fully consistent with the ruled dl6 law and costs a hard
migration of every brace in the corpus.

**2. CARD-PATTERN-GOAL-SPELLING** -- how a pattern attaches to its source.
**Note: directive `json_as_rel_type` takes the word `json` as a TYPE, so v5's
own op name `json(body, q:{...})` is no longer available.**

    (a) decode(body, {number: num, user: {login: author}})     -- shipped today, zero change
    (b) body = {number: num, user: {login: author}}            -- `=` is not a body operator yet
    (c) match(body, {number: num, user: {login: author}})      -- reuses a word that means something else

**3. CARD-LIST-SPELLING** -- checker delta measured at four clauses; only the
spelling is open.

    (a) rel repo(name: text, tags: list(text)).
    (b) rel repo(name: text, tags: text[]).
    (c) rel repo(name: text, tags: json).        -- do-nothing; nothing states it is an array of text

**4. CARD-JSON-NULL** (sharpened `null_and_optional`) -- there are now **two**
null questions, not one.

    (a) two-plane split   resp({name: 'cli', parent: null}).   % inside json: stored, round-trips
                          repo(null).                          % typed column: field_not_text refusal
    (b) reject at ingress  resp({..., parent: null}).          % => json_null_at_ingress(parent)
    (c) explicit variant   rel parent(some: text; none).

**5. CARD-BRACE-TAG** -- reserve `Tag{...}` now for the directive's stated
future abuse of `{`?

    (a) reserve   diag(point{x: 1, y: 2}).   % => refused: tagged_brace_reserved(point)
    (b) do not

**6. CARD-JSON5-SUBSET** and **7. CARD-STRING-QUOTE** -- the literal's exact
shape. Both already parse; these are printer/lexer scope decisions.

    json5 draft:  { # the repo name
                    name: 'cli', stars: 4, tags: ['go', 'rust'], }
    strict json:  { "name": "cli", "stars": 4, "tags": ["go", "rust"] }

**8. CARD-DESCENT-DEPTH** -- `**` has never had a cap in any generation
(archive `TASKS.md` T9 asked, never built).

    (a) unbounded      decode(body, {**: {image: i}})
    (b) capped         decode(body, {**(3): {image: i}})
    (c) bind the path  decode(body, {$$path: {image: i}})    -- free in the lowering (json_tree.fullkey)

**9. CARD-REGEX-KEY** -- `REGEXP` is *syntax* in core SQLite with no
implementation. The `sqlite3` CLI and `@libsql` each supply one (measured);
rusqlite by default does not, and `rust_flip_soon` means that matters.

    (a) ship it        decode(body, {re:^(dev-)?dependencies: {$name: $version}})
    (b) compose        decode(body, {$section: {$name: $version}}), section =~ '^(dev-)?dependencies'

**10. CARD-VALUE-PATTERN** -- recommend **not wanted**; v5 parsed it and matched
it literally, so it never shipped its semantics (`TASKS.md` T7 still open).

**11. CARD-FORMAT-DISPATCH** -- yaml/toml/jsonl through one grammar was v5's
behaviour; in v6 it is a host-decl question, not syntax.

**12. CARD-EDGE-BODY-JSON** and **13. schema_import_boundary** -- no spelling to
pick; scheduling only.

---

## 6. Findings worth carrying beyond this lane

1. **The two highest-graded (d) rows have exact json1 implementations.** Key
   capture is `json_each`'s existing `(key, value)` columns; `**` is
   `json_tree`. The recovery doc graded both "genuinely needs new surface
   syntax" -- correct about the *surface*, and the *lowering* turns out to be a
   solved problem the moment `body` is a json column. What is left to decide is
   only spelling.
2. **v4's `$$${PATH?}` comes back for free.** `json_tree.fullkey` is in the same
   join that implements `**`. The one construct the recovery doc lists as
   dropped with no successor costs nothing.
3. **`jsonb` is not portable across the two SQLite builds this project already
   runs** (3.43.2 CLI vs 3.45.1 libsql). json columns store TEXT.
4. **`REGEXP` is not core SQLite.** Both our current builds happen to ship an
   implementation; rusqlite does not. Prices card 9 honestly against the rust
   flip.
5. **json1 will not canonicalize.** `json()` preserves key order and
   `json_group_object` follows row order, so `canonical_json_text/2` stays the
   single canonicalizer and the cross-target log contract is untouched.
6. **The `type = 'object'` guard is not cosmetic.** Without it, descending into
   a scalar raises `malformed JSON` and kills the statement instead of failing
   silently. Any implementation of this design must emit it.
7. **SQLite cannot CHECK a list's element type** (no subqueries in CHECK). It
   can CHECK array-ness. Element typing is a checker obligation.
8. **The prolog term form already has the `{` extensibility seam** -- swipl
   reads `tag{...}` as a dict, unrelated to `{}/1`.
9. **`json` as a type word retires `json(...)` as an op name**, which is the v5
   spelling. Card 2 exists because of this collision.
10. **`decode` is not an rx/prolog/SQL word** (the language-design review's B8
    already flagged the class). Card 2 is the place to fix it if it is going to
    be fixed.

## 7. Lab inventory

| file | receipts | what it proves |
|---|---|---|
| `0_receipts.pl` | -- | one entry, `go/0` |
| `1_grammar.pl` | R1-R7 | prototype DCG; 27 verbatim archive examples; flagship dialect agreement; literal = pattern minus holes; `tag{}` seam |
| `2_lowering.pl` | L1-L7 | pattern -> json1 SQL, emitted text asserted AND executed; jsonb portability; canonicalization ownership |
| `3_lists.pl` | T0-T6 | 3x5 grading with measured row counts; four-clause checker delta; CHECK-constraint limits |
| `4_cards.pl` | C1-C4 | 14 origin cards classified; 29 exact spellings; every answer attributed to a ruled directive |

Lab protocol: this lab dies on landing. Durable output = the card list (§5) into
a ruling, the grammar (§1) into `parse_dl.pl`/`SYNTAX.md`, the lowering (§2)
into `lower.pl`, the list verdict (§3) into `0_type_plane.pl`. Until then
nothing here is production.
