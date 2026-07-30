# JSON flex lab — verdict

Contract: [2026-07-30-json-flex-lab-header.md](2026-07-30-json-flex-lab-header.md).
Base sha `a116e3e9`. Grade-then-harden on the json plane, both doors, byte-diffed.

**Headline.** The json plane's SEMANTICS held up better than its ENCODERS. Every
production (exact key, spread, key capture, `**` descent, empty object, typed
list) answered correctly on both doors under every value kind the lab threw at
it. What did not hold was the layer underneath: the oracle's tick log was not
JSON at all for any control character, a text column holding a json object read
back as SQL NULL, and the tick-log encoder decided whether a value was structure
by looking at its first character. Three defects, all fixed here with fail-first
receipts, none of which any of the 209 prior fixtures could see.

Corpus 209 -> 221. Sweep 145/143/0-wrong -> **157 total / 155 identical / 0
wrong**, both emitter modes, the 2 run_errors being the pre-existing pair. Zero
movement in any prior bucket.

---

## 1. Build-vs-buy: the external corpus (Q7)

Researched before any generator was written, per the standing law.

| candidate | licence | shape | verdict |
|---|---|---|---|
| **nst/JSONTestSuite** | MIT | 318 `test_parsing` files classified `y_` (must accept) / `n_` (must reject) / `i_` (implementation-defined), plus 22 `test_transform` files for number precision, duplicate keys and NFC/NFD keys | **BOUGHT.** The only candidate whose classification maps onto an accept/reject gate, which is precisely the question ("json is a large state machine"). Its `test_transform` half lands directly on three of the header's named slots. The reference corpus behind "Parsing JSON is a Minefield"; reporting against it is a number other implementations can be compared to. |
| JSON_checker (json.org, Crockford) | no licence file | 38 files, `pass1..3` / `fail1..33` | Rejected: a strict subset of JSONTestSuite's cases, no implementation-defined class (so no honest place to put SQLite's own latitude), and no licence. |
| google/json-test-suite | Apache-2.0 | one large generated document plus a fuzz corpus | Rejected: no per-case accept/reject classification, so it produces no tally. Useful as a fuzz seed, which is not this question. |
| JSON-Schema-Test-Suite | MIT | schema validation cases | Rejected: tests a validator, not a parser. Out of scope. |
| fast-check / jsverify property generators | MIT | random documents over the grammar | Rejected as a DEPENDENCY, adopted as a SHAPE. A generator grades round-trip, never the accept/reject boundary. The round-trip leg here is a fixed 136-value enumeration (`0_receipts.pl:corpus_value/1`) rather than a seeded random one, because a cross-target CONTRACT receipt has to be reproducible and diffable across runs and machines. |
| bespoke document generator | — | — | Refused. JSONTestSuite fits, is MIT, and is what everyone else reports against. |

The corpus is cloned at run time into the OS temp dir (or `JSON_TEST_SUITE`);
nothing is vendored.

### The tally, the way the suite intends it

```
y_ (must accept)   95 / 95   pass
n_ (must reject)  187 / 188  pass
i_ (impl-defined)  31 accepted / 4 rejected  (0 required either way)
crash-instead-of-refusal: 0
```

The arrival gate under test is the `json` column's own
`CHECK (json_valid("body"))` — the real thing a document meets, not a synthetic
parser call.

**The one `n_` failure is inherited, not ours: `n_multidigit_number_then_00.json`.**
SQLite's `json_valid` accepts it. Nothing in this repo can refuse it without
putting a second JSON parser in front of the first.

**Zero crashes.** Every one of the 188 `n_` documents and 35 `i_` documents that
was refused was refused by the CHECK constraint, named, with the row rejected.
No document produced a runtime error, a partial write, or an unraisable failure.

Every ACCEPTED document (127) then round-trips through the tick-log encoder:
**0 throws, 0 non-JSON output, 0 non-idempotent**.

---

## 2. What was broken, and is now fixed

All three were red on the real sweep before they were green. None was visible to
the 209-fixture corpus, and that invisibility is the finding.

### F1. The oracle's tick log was not JSON (control characters)

Both prolog encoders spelled the hex escape with an ABSOLUTE column stop:

```prolog
format(atom(HexAtom), '\\u~`0t~16r~4|', [Code])
```

`~4|` counts from the start of the atom and `\u` already occupies two of those
four columns, so every escape came out with TWO hex digits and the next source
character glued onto it:

```
value_json('a\fb', J)   ->  J = '"a\u0cb"'
JSON.parse('"a\u0cb"')  ->  SyntaxError: Bad Unicode escape in JSON
```

`\b`, `\f`, `\r` and every other control code took that arm. The tsv2 door is
`JSON.stringify`, which is correct AND uses the short escapes, so the two doors
disagreed on every one of these bytes as well.

Fixed to `JSON.stringify`'s exact set (ECMA-262 QuoteJSONString): `" \ \b \f \n
\r \t` by name, four lowercase hex digits otherwise.

**THERE WERE THREE COPIES.** `conformance/ticklog.pl`, `0_type_plane.pl`, and a
private one in `compile/sweep.pl`. Repairing the first two left the fixture RED,
because the SCHEDULE sweep.pl writes still said `"back\u08space"` and the sweep's
own reader rejected it. `escape_json_codes/2` is now exported from
`0_type_plane.pl` and imported by sweep.pl; ticklog.pl keeps its documented
mirror (it is a script, not a module — the same duplication `json_value_json/2`
already carries, for the same reason).

Fail-first receipt: `RUN_ERROR json_string_control_escapes_are_valid_json Bad
Unicode escape in JSON at position 45` -> identical.

### F2. A text column holding a json object read back as SQL NULL

`lower.pl:canonical_column_expr/3` renders a json1 tagged compound
(`{"fn": F, "args": [...]}`) back into `F(...)` term text, gated on:

```sql
CASE WHEN json_valid(c) AND json_type(c) = 'object' THEN json_extract(c,'$.fn') || '(' || ... END
```

That guard is true of EVERY json object, including one a program legitimately
stores in a text column. For those, `json_extract(c,'$.fn')` is NULL and the
whole concatenation is NULL — in a column `IRowValue` says can never be null.

```
RUN_ERROR json_top_level_scalar_document_is_a_value
  Cannot read properties of null (reading '0')
```

The guard now tests for the tagged SHAPE (`json_type(c,'$.fn') = 'text'` and
`json_type(c,'$.args') = 'array'`). `coalesce` was added in the same expression
for the nullary functor, which collapses the same way one arity down
(`json_each` over `[]` returns no rows, so `group_concat` answers NULL).

### F3. The tick-log encoder guessed structure from the first character

```ts
if (value[0] !== "{" && value[0] !== "[") return null;   // ticklog.ts, removed
```

Wrong in BOTH directions, measured:

| stored | column type | old (sniff) | oracle | new |
|---|---|---|---|---|
| `42` | json | `"42"` | `42` | `42` |
| `null` | json | `"null"` | — | `null` |
| `true` | json | `"true"` | — | `true` |
| `{"a":1}` | **text** | `{"a":1}` | `"{\"a\":1}"` | `"{\"a\":1}"` |

The sniff got **20 of the lab's 23 valid documents wrong**, and disagreed with
the oracle on **22 of the 136 corpus values**.

`json` stops collapsing to `text` at the driver seam
(`emit_ts.pl:boundary_column_type/2` — whose header asserted that widening
`IRowColumnType` "would buy nothing", a premise this lab refutes),
`IRowColumnType` gains a `json` member, and the encoder is type-directed.
`rowValueFromSql` needed no new arm; the served arrival boundary keeps json's
acceptance byte-identical to text's on purpose.

**Measured en route, and it is its own finding:** `ref` columns need the same
arm. A ref column's boundary value is the dictionary's memoized `__rendered`
text, and that text is **not key-sorted** — it comes out in DECLARED column
order (`{"start":3,"end":9}` for `span(start, end)`). The old sniff had been
silently re-canonicalizing it on the way out. Nineteen fixtures went red the
moment `ref` was left out of the new arm. See card C6.

### F4. `json_canon/2` answered "empty object" for an unbound value

```
json_canon(Free, C)  ->  C = obj([]),  and Free is now bound to '{}'
```

The `'{}'` clause unified an unbound input with the atom. Latent rather than
live (every shipped path reaches `json_canon` through `eval_expr/2`, which
throws `unbound_in_expression` first — measured), but a silent wrong answer is
exactly what a named refusal is for. Now `throw(json_value_unbound)`.

---

## 3. Per-question findings

### Q1 — values

Every value kind through storage, destructure and render, both doors. Full table
in the lab's own output; the items that carry a decision:

**Depth.** json1's nesting cap is exactly **1000 accepted / 1001 refused**,
bisected (`@libsql` 3.45.1 and the 3.43.2 CLI agree; it is
`SQLITE_MAX_JSON_DEPTH`). `JSON.parse` accepts far deeper. The oracle has no cap
of its own: depth 20,000 and width 100,000 both canonicalize and render.

**Numbers do not round-trip as text.** 11 of 23 documents have a canonical text
that differs from their stored text, and every one is a number, because json1's
`json()` preserves the source LEXEME while the canon preserves the VALUE:

```
1.0        -> 1              1e6    -> 1000000        -0 -> 0
1e-999     -> 0              1e999  -> null           9007199254740993 -> 9007199254740992
```

`1e999 -> null` is the sharp one: **the tick log asserts a value the document
does not contain.** `JSON.stringify(Infinity)` is `null`, and the encoder's
`Number.isFinite` guard only covers the numeric-column path, never the json
path. Card C1.

**Wide integers are a READ-side cliff at ±(2^53 − 1).** The write always
succeeds; the read throws:

```
INSERT 9007199254740992                       -> ok, row is in the table
SELECT "v" WHERE "v" = 9007199254740992       -> RangeError: Received integer which
                                                 cannot be safely represented as a
                                                 JavaScript number
```

A program can store an integer it can never read back, and the error names no
rel and no column. Same throw for a wide integer reached through `json_extract`
inside a document. Past i64, the failure mode changes rather than stops: `json()`
keeps the source text but `json_extract` hands back a REAL. Card C2, with a
fixture pinning the last working value on both signs.

**Strings.** After F1, every escape class agrees byte-for-byte across all three
encoders: `" \ / \b \f \n \r \t`, `\uXXXX` for the remaining control codes, BMP
and astral characters raw. Empty string, empty object and empty array all render
as themselves on both doors.

### Q2 — null in documents vs a language with no null

**The doors already agree, and that is the surprise.** The shipped presence read
`json_extract(...) IS NOT NULL` collapses present-null with absent — and the
oracle collapses them too, because `body.pl:braces_decode/2` guards both member
clauses with `Value \== none`. Both doors yield zero rows for both states. The
header listed this predicate as a mechanical fix; **fixing it unilaterally would
CREATE a divergence**, so it is not fixed here. It is a shared design decision
that nothing states, and it needs a ruling, not a patch.

Read paths classified (both builds):

| read | present value | present null | absent | |
|---|---|---|---|---|
| `json_extract(doc,'$.k')` | the value | NULL | NULL | COLLAPSE |
| `json_extract(...) IS NOT NULL` | 1 | 0 | 0 | COLLAPSE — **the shipped one** |
| `json_type(doc,'$.k')` | `text`/… | NULL | NULL | COLLAPSE |
| `json_type(...) IS NOT NULL` | 1 | 1 | 0 | preserve |
| `EXISTS (json_each … key = 'k')` | 1 | 1 | 0 | preserve |

**A LIVE DIVERGENCE the corpus has never covered.** The oracle's stand-in for
json null is the ATOM `none`, and the guard is spelled against that atom — so
the ordinary text value `"none"` is unreachable on the oracle while it binds
normally on the emitter:

```
oracle:   decode({status: none}, {status: V})       ->  0 solutions
emitter:  json_extract('{"status":"none"}','$.status') IS NOT NULL  ->  1 row, V = 'none'
```

And the render disagrees the same way: `ticklog.pl` prints `{k: none}` as
`{"k":"none"}` while the emitter prints a real json null as `{"k":null}`. The
codebase already solved this exact ambiguity once, for booleans, with
`bool_lit/1`; json null has no such term. Card C3.

**Two documents cannot be stored at all**, measured through the real emitted
arrival statement: a json `null` DOCUMENT vanishes (the column is `NOT NULL` and
`json_extract` of a JSON null is SQL NULL), and a json `true` DOCUMENT degrades
to the integer `1` (SQLite has no boolean). Card C4.

### Q3 — keys

**Duplicate keys diverge, silently.** The oracle THROWS `json_dup_key([a,a])`.
The emitter accepts the document at the CHECK, `json()` keeps both members,
`json_each` yields the key twice, and the tick-log canon then **last-wins**:

```
stored   {"a":1,"a":2}
json()   {"a":1,"a":2}
json_each keys  [a|a]
tick log {"a":2}
```

Card C5. JSONTestSuite's `test_transform/object_same_key_*.json` exist because
implementations disagree about exactly this.

**Empty-string key, `$`-shaped key and `**`-shaped key are ordinary data** on the
value plane, on both doors, and survive key capture. Fixtures promoted.

On the PATTERN plane the two markers behave differently, and only one is a
collision: `'$ref'` (a quoted atom) is an ordinary key — only the `$`/1 COMPOUND
is a hole — but `'**'` in key position is unconditional, so **a document key
literally spelled `**` can never be matched by an exact-key pattern**. Measured:
`decode({'**': deep, ...}, {'**': X})` binds X to the whole root object (descent
semantics), never to `deep`. Card C7.

**NFC and NFD stay distinct** everywhere: json1 does not normalize, neither
encoder normalizes, and both spellings survive as separate keys. Pinned by
fixture, with both keys written as `\x` escapes — the two spellings look
identical in every editor and tools in this repo's own authoring path silently
normalize one into the other (it turned the first draft into `json_dup_key` with
no visible cause).

**Key collation is a cross-target contract with a hole.** Prolog `keysort` on
atoms is code-POINT order; JS `Array.prototype.sort` on strings is UTF-16
code-UNIT order. They agree across the whole BMP and differ on the astral plane,
where surrogates sort below U+E000..U+FFFF:

```
input       U+FF3A  U+1D400  U+61
JS .sort()  U+61  U+1D400  U+FF3A
code point  U+61  U+FF3A  U+1D400
```

No shipped program can produce an astral key today, so this is a contract
decision rather than a bug. Card C8; the BMP half is pinned by fixture.

A key containing `"` is fine as data on both doors; `lower.pl` refuses it by name
(`json_key_contains_quote`) at PATTERN position, where it would have to be
concatenated into a path string.

### Q4 — canonicalization

**The three encoders agree.** Over the lab's 136-value corpus, after the fixes:

```
ticklog.pl (#1)      vs  0_type_plane.pl (#2)   0 disagreements
ticklog.pl (#1)      vs  ticklog.ts (#3)        0 disagreements
ticklog.pl (#1)      vs  sqlite json()          0 differ on this corpus
the REMOVED first-char sniff                    22 disagreements  (negative control)
non-idempotent canon (prolog side)              0
non-idempotent canon (127 JSONTestSuite docs)   0
```

The `json()` agreement is corpus-specific and stated as such: json1 minifies but
**does not sort keys**, so it agrees only where the source order already matches
the canon. It is a third reference, not a third canonicalizer.

The one residue: **8 corpus values lose precision**, all wide integers, all
through `JSON.parse`'s double conversion before this code sees them. Reported,
not asserted away. Card C2.

### Q5 — malformed

**The `json` column's CHECK is total, and it is the whole protection.** Across
188 `n_` files plus 7 hand-built corruption classes (truncated object,
truncated array, empty string, non-json text, trailing content, JSON5
single-quoted key, trailing comma), every malformed document was refused by the
CHECK constraint with the row rejected. Zero crashes.

On an UNGUARDED text column the same documents make `json_extract` **raise
`malformed JSON` and kill the statement**. So the guard-first-in-WHERE claim
holds, and the reason it holds is the column type: nothing else stands between a
malformed document and a dead statement.

**A named asymmetry, in the safe direction.** SQLite 3.45's `json_valid` defaults
to strict RFC-8259 while `json_extract` reads the JSON5 superset. Our gate is
therefore STRICTER than our reader: `{'a':1}` and `{"a":1,}` are refused at
arrival but would be read correctly if they got in. Worth knowing before anyone
"helpfully" relaxes the CHECK.

**`#` comments are a TEXT-DOOR superset only.** `json_valid('{"a":1 # note}')` is
0; json1 has no comment syntax at any point. Whatever `.dl6` source accepts
inside a braces literal never reaches a json document.

**Oracle-side "malformed"** is a different question, since fixtures carry terms
rather than text — and the oracle has NO json text parser at all, which is why
Q7 could only be graded through the emitter's gate. `json_canon/2` passes a bare
compound and a partial list through unchanged (rendered as term text in a JSON
string, documented behaviour) and F4 closed the unbound hole. It still accepts
NON-ATOM KEYS silently — `{1: v}`, `{f(x): 1}`, `{{a:1}: 2}` all canonicalize
into objects whose "key" is a term and render as a JSON string. Card C9.

### Q6 — scale

Count-test law, on the real emitted statement shape.

```
documents=10     statements-to-read=1   rows=20     (2 per document)
documents=100    statements-to-read=1   rows=200
documents=1000   statements-to-read=1   rows=2000
EXPLAIN QUERY PLAN
  SCAN t
  SCAN j0 VIRTUAL TABLE INDEX 1:
json_each openings in plan: 1
```

One statement, one `json_each` opening, rows exactly linear in documents. No
per-arrival loop, no per-element statement.

Sabotage receipt (run by hand, recorded in the lab file's header): replacing the
`json_each(t."body") j0` join with a correlated `(SELECT … FROM json_each(…))`
per output column turns the constant one opening into one per column and the
assertion goes red. Deleting the `j0.type='object'` guard leaves the PLAN
identical and the assertion still green — which is why that guard carries a
SEMANTIC receipt in Q5 rather than a plan receipt.

One large document, 1,100,041 bytes / 10,680 elements, end to end:

```
store 2.7ms   destructure to 10,680 rows 3.9ms   tick-log render 1.4ms (1,249,563 bytes out)
```

Oracle-side fan-out, which is the cardinality the emitted joins have to match:
exact key 1, key capture 2, `**` descent 4, spread 2. A `memberchk` on the oracle
or a correlated scalar subquery on the emitter answers 1 for all four.

---

## 4. Cards handed back

Priced, none taken. Ordered by how much a decision changes.

**C1. `slot_json_float_fate` — numbers do not round-trip through the canon.**
`1.0 -> 1`, `1e6 -> 1000000`, `-0 -> 0`, `1e-999 -> 0`, and `1e999 -> null`,
which is the log asserting a value the document does not have.
- A: accept value-canon, and add a REFUSAL for non-finite results at the json
  path (mirrors the existing `Number.isFinite` guard on the numeric path). Small,
  closes the `null` hole, leaves lexeme rewriting in place.
- B: preserve number lexemes, which means not routing through `JSON.parse` — a
  JSON tokenizer. See C2 for the shared dependency question.
- C: refuse float-valued json documents at the arrival gate.
- *Closes on*: whether any real program's json documents carry floats. ghcacher's
  GitHub payloads do.

**C2. `slot_json_bignum` — integers past 2^53.** Read-side cliff (RangeError
naming no rel/column) at ±(2^53−1); silent precision loss inside documents.
- A: `@libsql` `intMode: "bigint"`, which changes EVERY integer at the seam into
  a `bigint` and ripples through `IRowValue`, the diff key, and every emitted
  comparison. Correct and large.
- B: a lossless JSON parser for the canon path only (`lossless-json` is the named
  candidate, MIT, keeps numbers as `LosslessNumber`). One dependency, bounded
  blast radius, does nothing for the read-side RangeError.
- C: refuse integers outside the safe range at the arrival gate, naming rel and
  column. Cheapest, honest, and loses expressiveness the storage layer has.
- *Closes on*: a user call on the dependency (B) and on whether wide integers are
  in scope at all. Fixture
  `json_safe_integer_boundary_survives_both_doors` pins where the seam ends today.

**C3. `slot_null_pattern_spelling` — the oracle's json null is an ambiguous atom.**
`none` is both "json null" and the ordinary text `"none"`, and the guard
`Value \== none` cannot tell them apart, so the emitter binds a value the oracle
drops. The codebase solved exactly this for booleans with `bool_lit/1`.
- A: `null_lit` (or equivalent) as a distinguished term, mirroring `bool_lit`.
  Needs a text-door spelling too: on the value plane a bare `null` reads as a
  VARIABLE today, so `{k: null}` is currently inexpressible in `.dl6`.
- B: refuse the atom `none` by name in a json value position. Decidable, cheap,
  and admits no null at all.
- C: leave it, and document that a json null and the string `"none"` are the same
  value on the oracle.
- *Closes on*: the ruling. Everything in Q2 hangs off this one.

**C4. json `null` and json `true` DOCUMENTS cannot be stored.** A top-level null
vanishes at the arrival gate (`NOT NULL` column, `json_extract` of JSON null is
SQL NULL); a top-level `true` degrades to the integer `1`. Measured through the
real emitted arrival statement. Sub-case of C3 for null; the boolean half is
independent and cheaper (SQLite has no boolean, so it needs an explicit
`json_quote`/text path at the arrival seam).

**C5. `slot_dup_key_fate` — duplicate keys.** Oracle throws, emitter silently
last-wins.
- A: refuse at the arrival gate. A top-level check is one expression
  (`(SELECT count(*) FROM json_each(b)) <> (SELECT count(DISTINCT key) FROM json_each(b))`);
  a RECURSIVE check needs `json_tree` and `fullkey` and is real work.
- B: define last-wins on both doors and delete the oracle's `json_dup_key` throw.
- C: leave it, and accept that the oracle rejects documents the emitter accepts.
- *Closes on*: whether any real payload has duplicate keys. GitHub's does not;
  hand-written `.dl6` braces literals can, and today that IS refused by the
  oracle.

**C6. The dictionary's `__rendered` text is not canonical.** The struct-as-rows
header (`plans/2026-07-29-struct-as-rows-header.md`) states that canonical JSON
is written once at intern time. It is not: keys come out in declared column
order, and the tick-log encoder has been re-sorting them on every read. Nineteen
fixtures prove it (they go red the moment `ref` is dropped from the encoder's
json arm). Either the intern-time render starts sorting, or the header's claim
gets corrected and the read-side canonicalization becomes the stated contract.
Not free either way: the intern-time fix touches the memoized text every value
row carries.

**C7. `**` in key position is unconditional.** A document key literally spelled
`**` can never be matched by an exact-key pattern, because the descent clause
fires first and cuts. Either a quoting escape on the key plane, or a named
refusal when a pattern's key is the literal `**`, or leave it documented. `$` has
no such problem — only the `$`/1 compound is a hole, a quoted `'$ref'` is data.

**C8. `slot_key_collation` — astral key sort.** Prolog code-point order vs JS
code-unit order. Unreachable today (no shipped program can mint an astral key),
but it IS the cross-target log contract, so a target written in a third language
needs the answer stated. Cheapest: state code-point order as the contract and
give ticklog.ts a comparator instead of the default `.sort()`.

**C9. `json_canon/2` accepts non-atom keys.** `{1: v}`, `{f(x): 1}`,
`{{a:1}: 2}` all canonicalize and render their "key" as a JSON string. A key must
be text in JSON. A named refusal is decidable and one clause.

**C10. Ambiguity in the tagged-term encoding.** After F2 the guard tests for the
tagged shape, but a text value that genuinely IS `{"fn":"x","args":[]}` still
renders as `x()`. The encoding has no reserved marker; shape is all the read side
can consult. Only a marker (or a per-column "this holds terms" bit) closes it.

**C11. `json_dup_key` prints as swipl's `Unknown message`.** Design-review
finding B4 in the wild again, on a json refusal: no file, no line, no explanation.
Every named refusal in this arc inherits it.

---

## 5. Slots from the header

| slot | status |
|---|---|
| `slot_json_float_fate` | OPEN, priced 3 ways — card C1. The `1e999 -> null` hole is new information. |
| `slot_json_bignum` | OPEN, priced 3 ways — card C2. Boundary measured exactly: ±(2^53−1), read-side. |
| `slot_key_collation` | OPEN — card C8. BMP half pinned by fixture, astral half unreachable today. |
| `slot_dup_key_fate` | OPEN — card C5. The divergence is now measured on both doors. |
| `slot_null_pattern_spelling` | OPEN — card C3, and it is the highest-value one: it is the only card the rest of Q2 depends on. |

---

## 6. Fixtures promoted (12, corpus 209 -> 221)

All in `v6/prolog/conformance/fixtures/8_json_flex.pl`. Nine pin an untested
agreement; three were fail-first.

| fixture | what it holds |
|---|---|
| `json_string_control_escapes_are_valid_json` | **fail-first (F1).** \b \f \r \x01 \t \n as a column value. |
| `json_control_escapes_inside_a_document` | **fail-first (F1).** the same characters inside a json object. |
| `json_top_level_scalar_document_is_a_value` | **fail-first (F2 + F3).** json column carrying scalars, beside a text column carrying JSON-looking bytes. |
| `json_non_ascii_keys_sort_by_code_point` | BMP key sort, the cross-target contract. |
| `json_nfc_and_nfd_keys_stay_distinct` | no normalization anywhere; both keys written as `\x` escapes. |
| `json_empty_string_key_round_trips` | the one key spelling that is not an identifier. |
| `json_marker_shaped_keys_are_ordinary_data` | `$ref` and `**` as document keys. |
| `json_safe_integer_boundary_survives_both_doors` | ±(2^53−1), where the seam ends. |
| `json_empty_containers_nest` | `{}` and `[]` at every position. |
| `json_deep_exact_key_chain_binds` | an 8-deep accumulated `json_extract` path. |
| `json_absent_key_yields_no_row_under_arrivals` | the missing-key silence under a real tick (the existing fixture is empty-schedule and grades vacuously). |
| `json_spread_and_capture_and_descent_multiply` | the three fanning productions in one rule; cardinality is the product. |

---

## 7. Receipts

Coordinator-reproducible, in this order.

```
cd v6/prolog/conformance && swipl -q -l go.pl -g go -g halt        221 PASS, 0 fail
cd v6/tsv2 && bash scripts/sweep.sh                                 RUN 157/155 identical/0 wrong
SPREFA_TSV2_EMITTER_MODE=naive bash scripts/sweep.sh                RUN 157/155 identical/0 wrong
cd v6/prolog && bash compile/scripts/text_door_receipt.sh           157/157/0
cd v6/prolog/compile && swipl -q -l test/plunit_tests.pl -g run_tests -g halt
cd v6 && just green                                                 EXIT 0
```

Baseline at `a116e3e9`: conformance 209/0, sweep 145/143/0-wrong, TEXT_DOOR
145/145/0. The 2 sweep `run_error` rows (`log_retraction_rejected`,
`fork_join_error_arm_is_a_value`) are the pre-existing pair and are unchanged.

Lab receipts, which die with the lab:

```
cd v6/prolog/labs/json_flex && swipl -q -l 0_receipts.pl -g receipts -g halt
cd v6/prolog/labs/json_flex && node 1_sqlite_receipts.mjs     6 receipts pass, 0 fail
```

`JSON_TEST_SUITE` points at a checkout; unset, the script clones nst/JSONTestSuite
into the OS temp dir. Every database is `:memory:`. No daemon, no
`~/.local/state`, nothing vendored.

## 8. Staffing

- Work type: grade-then-harden lab, worktree `agent-af3083d144be9043f`
- Base sha: `a116e3e9`
- Lab files: `v6/prolog/labs/json_flex/{0_receipts.pl,1_sqlite_receipts.mjs,corpus.jsonl}` —
  DELETED on landing per the lab protocol. **Last copy: `6dde7f9a`.** Recover with
  `git show 6dde7f9a:v6/prolog/labs/json_flex/1_sqlite_receipts.mjs`
- Production edits: `conformance/ticklog.pl`, `conformance/body.pl`,
  `0_type_plane.pl`, `compile/sweep.pl`, `compile/lower.pl`, `compile/emit_ts.pl`,
  `compile/test/plunit_tests.pl`, `tsv2/runtime/{types,ticklog,tickLoop}.ts`,
  `tsv2/serve/{3_engine,4_http}.ts`, `tsv2/scripts/{sweep,golden-run}.ts`,
  `tsv2/tests/7_value-plane.test.ts`, and the new fixture file
