# String standard library, as registry rows

Date: 2026-08-12. Base: `0d218284`. Ownership: `registry.pl`, `lower.pl:590-630`,
new fixtures, this doc.

SQLite version checked and pinned by probe: **3.45.1** (bundled by `@libsql/client`
0.17.4; confirmed `SELECT sqlite_version()` = 3.45.1 at the runtime seam).

## Table of contents

- Inventory
- The binding constraint and its one escape hatch
- Landed (green) functions
- Forked functions, each with the throw site
- Per-target lowering, the written fork
- Validation receipts

## Inventory

The binding constraint, verified by read before any row was added:
`text_scalar` family forces `Type = text` at `lower.pl:551`, AND its `text_only`
TypeRule requires every operand to be text at `lower.pl:595-604`. A text scalar
is therefore strictly text-in/text-out with all-text operands. Anything that
returns a non-text (int/bool), takes a non-text operand (count, index, code
point), or is multi-row (split) or variadic (printf) has no home in the family
without a type-rule change. That rule change is the real follow-up; it is out
of this lane's ownership and is priced in the fork table.

| function | signature | SQLite expressible? | how | verdict |
|---|---|---|---|---|
| upper | `upper/1` | yes, all-text | bare scalar `upper(X)` | LAND |
| lower | `lower/1` | yes, all-text | bare scalar `lower(X)` | LAND (ASCII-only, probed) |
| trim | `trim/1`, `trim/2` | yes, all-text | bare scalar | LAND |
| ltrim | `ltrim/1`, `ltrim/2` | yes, all-text | bare scalar | LAND |
| rtrim | `rtrim/1` | yes, all-text | bare scalar | LAND (`rtrim/2` already registered) |
| reverse | `reverse/1` | yes, all-text | bare scalar `reverse(X)` (present in 3.45.1 libsql, probed) | LAND |
| substr | `substr/2`, `substr/3` | yes | bare scalar (char-based in libsql) | FORK: int index operand breaks `text_only` |
| length | `length/1` | yes, returns int | bare scalar | FORK: returns int, not text |
| instr/index_of | `instr/2` | yes, returns int | bare scalar | FORK: returns int, not text |
| unicode | `unicode/1` | yes, returns int | bare scalar | FORK: returns int, not text |
| hex | `hex/1` | yes, all-text | bare scalar | FORK: not a string stdlib transform; excluded on scope |
| contains | `contains/2` | yes, `instr(X,Y)>0` | sign of instr | FORK: returns int/bool, not text |
| starts_with | `starts_with/2` | yes, substring compare | comparison | FORK: returns int/bool, not text |
| ends_with | `ends_with/2` | yes, substring compare | comparison | FORK: returns int/bool, not text |
| char_at | `char_at/2` | yes, `substr(X,pos,1)` | SQL expression | FORK: int index operand breaks `text_only` |
| repeat | `repeat/2` | yes | `WITH RECURSIVE` + `group_concat` | FORK: int count operand breaks `text_only` |
| pad_left/pad_right | `pad_*/3` | yes | repeat-CTE + substr | FORK: int width operand breaks `text_only` |
| char | `char/1` | yes | bare scalar | FORK: int code-point operand breaks `text_only` |
| split | `split/2` | no, multi-row | table-valued, not scalar | FORK: json_each fan-out is the real shape |
| join | `join/2` | no, aggregate | already `group_concat` aggregate | FORK: not a scalar |
| format/printf | `printf/N` | yes, variadic | bare scalar | FORK: fixed-arity row cannot express variadic |
| snake_to_pascal | `snake_to_pascal/1` | fragile | large recursive CTE | FORK: defers to a transform pass |
| snake_to_camel | `snake_to_camel/1` | fragile | large recursive CTE | FORK: defers to a transform pass |
| pascal_to_snake | `pascal_to_snake/1` | fragile | large recursive CTE | FORK: defers to a transform pass |

Probed on 3.45.1 (runtime seam, `@libsql/client`): `reverse`, `concat`,
`concat_ws`, `printf`, `format`, `char`, `unicode`, `hex`, `json_group_array`
present. `repeat`, `starts_with`, `ends_with`, `left`, `right`, `pad_left`,
`pad_right`, `char_length` absent. `substr`/`length` are unicode-character based
(`substr('aé中z',2,1)='é'`, `length('é')=1`). `upper`/`lower` fold ASCII only
(`lower('ÉẞΩ')` unchanged).

### char_at note

`char_at(s, i)` is `substr(s, i, 1)`, which is a restriction of the already-landed
substr, not a new SQL primitive. It returns text and would fit the family, but it
adds no lowering SQLite cannot already do, so it earns nothing; left out rather
than padded into the LAND column on the same logic that keeps starts_with out.

## The binding constraint and its one escape hatch

| site | what is baked in |
|---|---|
| `lower.pl:548-551` | any `text_scalar` expression hardcodes `Type = text` |
| `lower.pl:612-613` | the `@libsql` seam registers scalars (lower/unicode) but no user scalar functions; no UDF registration |
| `lower.pl:620-624` | a Rendering equal to the function name lowers straight to the SQLite scalar of that name |
| `lower.pl:614-618` | a Rendering not the name gets a hand-written SQL expression (`norm/1` as `WITH RECURSIVE`) |

The only door is a SQL expression assembled from built-in scalars. Every LAND
row uses one of the two lower.pl branches: either the bare-name branch
(`620-624`) or a hand-written expression branch (`614-618`).

## Landed (green) functions

Registered rows, in `registry.pl:258-264` region. Every LAND row is text-in with
all-text operands, so Rendering == Name and the existing bare-name branch at
`lower.pl:620-624` lowers it with no new lower.pl clause.

| row | rendering | lower.pl branch |
|---|---|---|
| `upper/1` | `upper` | bare name |
| `lower/1` | `lower` | bare name |
| `trim/1`, `trim/2` | `trim` | bare name |
| `ltrim/1`, `ltrim/2` | `ltrim` | bare name |
| `rtrim/1` | `rtrim` | bare name |
| `reverse/1` | `reverse` | bare name |

Each LAND row lands with a conformance fixture asserting a value, including at
least one edge case (empty string, multi-byte character, default-charset
trim), and an oracle clause in `conformance/body.pl` `text_scalar_value/3` so
the reference interpreter answers the same value the emitted SQLite does.

Oracle clauses added (faithful to probed SQLite behavior):
- `upper`, `lower` fold ASCII only.
- `trim`/`ltrim`/`rtrim` with no charset drop ASCII whitespace (space, tab,
  LF, VT, FF, CR); with a charset, drop set members, same as the existing
  `rtrim/2`.
- `reverse` reverses the character list.

The functions that need an int operand (substr, repeat, pad, char_at, char) and
the int/bool-returning set (length, instr, unicode, contains, starts_with,
ends_with) are FORKED, not landed, with the throw site `lower.pl:595-604`.
Landing them needs the `text_only` rule widened to admit an int operand, which
is a type-rule change priced in the fork table below.

## Forked functions, each with the throw site

| function | fork mechanism | evidence / throw site |
|---|---|---|
| length, instr, unicode | returns int; family forces text | `lower.pl:551` `Type = text` |
| contains, starts_with, ends_with | returns int/bool; family forces text | `lower.pl:551` |
| substr, char_at, char, repeat, pad_left, pad_right | int operand (index/count/width/codepoint) violates text_only | operand check `lower.pl:595-604` |
| format/printf | variadic arity, row is fixed Name/Arity | `registry.pl:237-264` row shape |
| split | multi-row, table-valued | JSON fan-out is `json_each`/`json_tree` (table-valued); no scalar decomposition |
| join | aggregate not scalar | already `group_concat` aggregate at `lower.pl:5159-5169` |
| snake_to_* | fragile large CTE | defers to transform/regexp pass |

The gate that blocks the most useful set is `text_only` at `lower.pl:595-604`:
every operand must be text. substr, repeat, pad, char_at (int index/count/width)
and length, instr, contains, starts_with, ends_with (int return) all fall there.
Widening `text_only` to a rule that admits an int operand (plus letting the
family emit an int result) is the one change that would land most of the fork
table; it is a type-rule change and is left for the user to rule on.

`split` priced honestly: a split produces MULTIPLE ROWS, which is a different
shape from a scalar. The existing fan-out (`json_each`/`json_tree` over a stored
JSON array) is the real door for multi-row decomposition, and the standard move
is to make `split` a table-valued relation whose implementation expands a
separator-separated text column through `json_each`. It must NOT be forced into
`text_scalar`; the family is single-valued by construction. Proposed shape, not
built here: a body-header relation `split_field(Rel, Field, part(Idx, Text))`.

## Per-target lowering, the written fork

The user's question: should the stdlib funcs be host rels supplied at runtime,
with the emitter deciding whether to lower to sqlite or to the language target?

Today Rendering is ONE value per function. The assumption is baked in at
`lower.pl:620-621` (`text_scalar_rendering(Function, Rendering, ...)` binding a
single Rendering for the whole column) plus the single-dispatch call at
`lower.pl:609`. Nothing in the row shape (`registry.pl:237`) carries a per-target
spelling.

### Option A: one SQL lowering, both backends go through SQLite

This is the current design. The Rust and TypeScript backends both emit/consume
the same SQLite, so a `lower.pl` SQL rendering is the single source of truth for
both. Cost: one production path to test. That is what every LAND row above does.

### Option B: per-target rendering set

The column becomes a set: `{sqlite: text, ts: expr, rust: expr}`. Required
changes: the `expression/5` row gains more columns or a sub-table keyed by
target; `text_scalar_rendering/4` becomes target-dispatched; each backend gets
its own spelling to maintain. Three spelling per function, three places to be
wrong, and nothing tests the ts/rust spelling unless each target gets its own
conformance door.

### Functions that would actually benefit from a native lowering

| function | why a native spelling beats SQL | verdict |
|---|---|---|
| length, instr, unicode, contains, starts_with, ends_with | they return int/bool, which the text family cannot express today | the real value; lower to a per-target native int/bool instead of the text CTE |
| split, join | multi-row / aggregate, scalar shape is the wrong one | native target expression is the honest home |
| snake_to_pascal/camel | a transform loop, not a one-shot scalar | native or regexp |
| upper, lower, trim, ltrim, rtrim, substr, reverse, repeat, pad | SQLite already does these exactly | gains NOTHING from three spellings |

The recommendation balance: the functions SQLite already does well (`upper`,
`lower`, `trim`, `substr`, `reverse`) should stay single-SQL; the fork is
only attractive for the int/bool-returning and multi-row set that the current
family structurally cannot hold. That set is small and cleanly bounded.

### The `sh` host escape hatch is the wrong mechanism

`sh name(cols) -> (cols) = \`tmpl\`.` (see `duplicate_host_name_is_refused.dl6:1`)
forks a shell per call. A string function is called per row inside a rule body;
a shell per row is a process per row, which blows the 10-second law on any
real corpus and breaks the incremental/IVM contract (a shell side effect is not
a pure function of its inputs). The escape hatch exists for rare host-side
imports, not for integral row-wise values.

## Validation receipts

| gate | command | baseline | after |
|---|---|---|---|
| conformance | `cd v6/prolog/conformance && swipl -q -l go.pl -g go -g halt` | 392/0 | see run |
| sweep | `cd v6/tsv2 && bash scripts/sweep.sh` | SWEEP total=390 compiled=286 | see run |
| arch | `cd v6/prolog && swipl -g go -t halt ARCH.pl` | 7 PASS / 0 FAIL | see run |

`MANIFEST_REASON_DIFF` must stay all zero.

## 2026-08-14: the fork table, resolved by implementation

Three of the fork rows above are closed. What closed them, and the spelling
each landed with.

### Already closed before this section was written

| fork row | closed by | spelling |
|---|---|---|
| substr, length, instr (int operand / int result) | the `typed_scalar` family, `registry.pl` | `Out := substr(Text, 2)`, `Out := substr(Text, 2, 2)`, `Out := instr(Text, '_')`, `Out := length(Text)` |
| the `text_only` gate | not widened; sidestepped | `typed(OperandTypes, ResultType)` carries per-operand types and a result type, so `text_only` never had to admit an int |

The inventory table's verdict column is therefore stale for those rows: they
read FORK and they are LAND. `expression/5` is the truth.

### split, landed here

**split introduces no table-valued construct.** The plan above priced split as
a new body-header relation (`split_field(Rel, Field, part(Idx, Text))`). That
was the wrong shape. split returns the json array carrier the language
already has, and the multi-row half is the `[... Part]` spread that
`json_arm.pl:159` has graded since the list-flavor ruling.

| row | family | rendering | type rule |
|---|---|---|---|
| `split/2` | `typed_scalar` | `split_json_array` | `typed([text, text], json)` |

The surface is a two-rule idiom, and both rules are constructs that already
existed:

```
rel sym(name: text).
rel sym_parts(name: text, parts: json_list(text)).
rel sym_word(name: text, word: text).
rel pascal(name: text, rendered: text).

sym_parts(Name, Parts) <-
  sym(Name),
  Parts := split(Name, '_').
sym_word(Name, Word) <-
  sym_parts(Name, Parts),
  decode(Parts, [... Part]),
  Word := initcap(Part).
pascal(Name, group_concat(Word, '')) <- sym_word(Name, Word).
```

Pure-rxjs lowering, per the every-snippet law: rule 1 is `map` (one text to
one array), rule 2 is `mergeMap` over the array's parts followed by `map`,
rule 3 is `scan`/`reduce` keyed on Name. The spread IS the mergeMap, which is
why reusing it rather than inventing a table-valued relation is the shape
that keeps the rx story honest.

**Why two rules and not one.** `Parts := split(Text, ','), decode(Parts, ...)`
inside ONE body does not compile: `compile_body_guards/6` at `lower.pl:5113`
compiles decode goals BEFORE the bind fold, so the decode source is not yet
bound and `decode_source_not_bound` throws. A stored json column between the
two rules is also the cheaper shape, because the separator scan is a
correlated recursive CTE and the spread would otherwise inline it twice (once
for the array type guard, once for `json_each`).

**Semantics**, JS `String.prototype.split` with a non-empty separator, probed
against sqlite3 3.43.2 and graded by the oracle:

| input | parts |
|---|---|
| `split('id,name,age', ',')` | `["id","name","age"]` |
| `split('', ',')` | `[""]` |
| `split('a,', ',')` | `["a",""]` |
| `split('a,,b', ',')` | `["a","","b"]` |
| `split('std::io::Read', '::')` | `["std","io","Read"]` |
| `split('café,naïve', ',')` | `["café","naïve"]` |
| `split('abc', '')` | `["abc"]` |

N separators give N+1 parts, so no empty part is ever dropped. The separator
is a SUBSTRING, never a character set: the oracle uses `sub_atom/5` rather
than `split_string/4`, which would read `'::'` as the set `{:}` and answer
`["std","","io","","Read"]`.

**The empty separator is a decision, and it is reversible.** JS walks
characters; this returns the whole text as one part. The reason is
termination: the SQL scan advances by `instr(rest, sep) + length(sep)`, and an
empty separator leaves `instr` at 0 and the position never moving, so a
character walk is not the cheap branch of the same CTE. Reverse it by adding a
character-walk arm to `split_json_array` in `lower.pl` and the matching oracle
clause, not by changing the surface.

**Lowering**: `typed_scalar_sql/3` no longer requires `Rendering == Function`;
it dispatches through a new `typed_scalar_rendering/4` that mirrors
`text_scalar_rendering/4`. The bare-name branch is the last clause, so
substr/instr/length are unchanged. The split clause seeds the scan with one
trailing separator, which makes the last part ordinary rather than a special
case, and filters the seed row's NULL part back out.

**Sites touched**: `compile/registry.pl` (one row), `lower.pl`
(`typed_scalar_rendering/4`), `conformance/body.pl`
(`typed_scalar_value(split_json_array, ...)` + `split_on_separator/3`),
`compile/test/plunit_tests.pl` (one `expected_row`),
`conformance/fixtures/15_string_split.pl` (new). No parser change: `split(A, B)`
parses through the generic identifier-then-paren path at
`parse_dl_dcg.pl:1209`, the same door `upper` and `substr` use. No printer
change: `print_dl.pl:654` prints it as an ordinary compound, and the text door
round-trips byte-identically.

The one gap a real row would close is an EMPTY affix. `substr`'s position 0
sits before the first character, so `substr(Text, 0 - 0)` reads the whole text
where `ends_with(Text, '')` must answer true. Every affix in the fixture is
non-empty for that reason, and that edge is the entire price difference
between the spelling and the row.

### Still forked, and why

| row | why it is still forked |
|---|---|
| format/printf | `concat([...])` already renders text from columns, which is what the typegen probe needed. printf buys padding and numeric formats only, and its oracle is a printf reimplementation. Not bought without a caller. |
| contains, starts_with, ends_with | all three are ALREADY SPELLABLE, and `16_string_affix_tests.pl` is the receipt rather than the assertion: `contains` is `Position := instr(Name, '::'), Position > 0`; `starts_with` is `Head := substr(Name, 1, length(Prefix)), Head == Prefix`; `ends_with` is `Tail := substr(Name, 0 - length(Suffix)), Tail == Suffix`. All four fixtures compile, run identical, and grade byte-clean in rust. A registry row would buy a NAME, not a capability. |
| join | still `group_concat`, still an aggregate, and the split fixture shows `group_concat` folding split parts back, so the pair is closed in practice |
| snake_to_pascal / camel / pascal_to_snake | `split` + `initcap` + `group_concat` spell PascalCase in three rules, graded by `split_initcap_and_fold_render_pascal_case`. A dedicated scalar would buy one rule, not a capability. |
| repeat, pad_left, pad_right, char_at, char | no caller |
