# Enum column-type defects, and the oneOf carrier

Two defects found by probing the compiler on 2026-08-08. Neither is covered by
any fixture. Both block using an enum as the `oneOf` carrier for schema emit.

## Contents

- [The two defects](#the-two-defects)
- [Defect 1: a nullary variant emits invalid SQL](#defect-1-a-nullary-variant-emits-invalid-sql)
- [Defect 2: an enum name is not a column type](#defect-2-an-enum-name-is-not-a-column-type)
- [Why storage keeps the tag rel and the flat shape stays at the wire](#why-storage-keeps-the-tag-rel-and-the-flat-shape-stays-at-the-wire)
- [Red fixtures, ready to paste with the fix](#red-fixtures-ready-to-paste-with-the-fix)
- [Reproduction](#reproduction)
- [What was measured alongside](#what-was-measured-alongside)

## The two defects

| id | one line | status |
| --- | --- | --- |
| `enum_nullary_variant_empty_pk` | a variant with no fields emits `PRIMARY KEY ()`; the program cannot boot | unbuilt |
| `enum_column_type_erased` | an enum name refuses as a column type although it monomorphizes into real tables | unbuilt |

## Defect 1: a nullary variant emits invalid SQL

Source:

```prolog
rel maybe_text(none() ; some(value: text)).
rel noted(id: int, value: text).
noted(id, value) <- maybe_text_some(id, value).
```

`compile_dl6` returns 0 and prints `wrote`. Emitted DDL:

```sql
CREATE TABLE "maybe_text_none" ("id" INTEGER NOT NULL, PRIMARY KEY ()) WITHOUT ROWID
```

Both consumers reject it:

```
sqlite3 3.43   Error: in prepare, near ")": syntax error
tsv2 runtime   BOOT FAILED: LibsqlError: SQLITE_ERROR: near ")": syntax error
```

The other two tables of the same enum are well formed, which is why the compiler
sees nothing wrong:

```sql
CREATE TABLE "maybe_text_some" ("id" INTEGER NOT NULL, "value" TEXT NOT NULL,
  PRIMARY KEY ("value")) WITHOUT ROWID
CREATE TABLE "maybe_text_tag" ("id" INTEGER NOT NULL, "tag" TEXT NOT NULL,
  "__refcount" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("id","tag")) WITHOUT ROWID
```

Every fixture in `conformance/fixtures/0_enum_variants.pl` gives every variant at
least one field, so the nullary arm has never executed.

Bare `none` without parens is a parse error at column 21, so `none()` is the only
spelling that reaches the emitter.

Two candidate fixes: key a fieldless variant on its `id` column, or refuse the
construct with a named message in `0_refusal_messages.pl`. The first keeps
`none()` usable, which is what makes an enum an Option.

Secondary observation on the same output: `maybe_text_some` keys on `"value"`
rather than `"id"`, so two instances carrying the same text collide. Worth a
separate look; it is not what breaks boot.

## Defect 2: an enum name is not a column type

Three probes, same shape, different verdicts:

| source | verdict |
| --- | --- |
| `rel span(start: int, end: int).` used as `at: span` | compiles |
| `rel grade(ripe(sugar: int) ; green(days: int)).` used as `g: grade` | `column_type_unknown` |
| `g: grade_tag`, a two-column rel that exists as a table | `column_type_unknown` |

The third line is the receipt that this is a defect. `grade_tag` is emitted with
`__refcount` and a full delta family, and it still reports as an unknown type.
Both declarations are spelled `rel`.

### Root cause is phase order

```
step 0  parse text        decls = [enum_decl(grade, (ripe(sugar:int) ; green(days:int))),
                                   col_type(picked/2, 2, grade)]
step 1  parse_dl.pl:822   ValueRelationNames = names in column position that
                          also have a relation_schema  ->  grade has none yet
step 2  parse_dl.pl:834   normalize_relation_value_decls mints type_decl per name
                          -> grade skipped                     <-- the loss
step 3  1_expansion.pl:69 enum_context computed, enum expanded
                          -> grade_ripe, grade_green, grade_tag now exist
step 4  0_type_plane.pl:62 type_definitions/2 scans for type_decl(_, _)
                          -> grade absent
step 5  0_type_plane.pl:126 column_storage(Types, grade, ref(grade)) fails
step 6                    refused: column_type_unknown
```

Steps 2 and 3 are ordered wrong for enums, and nothing re-runs step 2 afterward.

`0_enum_expand.pl:12-16` names this hazard in its own header and mints
`enum_context/2` against it. That predicate has two consumers,
`0_match_expand.pl:22` and `1_expansion.pl:69`. `0_type_plane.pl` mentions enums
zero times.

### Fix shape

Hand `enum_context` to the type plane and resolve an enum name to
`ref(EnumName_tag)`. The column stores one INTEGER id; the variant is recovered
by joining the tag rel the expansion already emits. No new table family, no
second checker, and the surrogate-key law is satisfied by construction.

Open design question for the user: should `g: grade` mean "any variant" through
the tag rel, or should a column be required to name one variant (`g: grade_ripe`)?
The first gives sum types in column position, which is the `oneOf` that OpenAPI
and JSON Schema emit need.

## Why storage keeps the tag rel and the flat shape stays at the wire

A flat `UNION ALL` view over the variant tables was considered as an alternative
carrier.

| axis | tag rel stays a table | flat view over variants |
| --- | --- | --- |
| exists today | yes, `__refcount` + 3 delta tables | no, `CREATE VIEW` occurs 0 times in the emitter and in every generated program |
| NULL | none, every variant table stays `NOT NULL` | needs NULL padding for non-shared columns |
| incremental maintenance | already delta-tracked | no delta table, no refcount, no departure frontier |
| DRed retraction | works per variant table | undefined |
| column reference | one INTEGER id | no stable id on a `UNION ALL` |
| width | each variant its own arity | union of all variants, mostly null |
| `oneOf` emit | variant list and per-variant fields both recoverable | which column belongs to which variant is lost |

The NULL row is the decisive one. Absence in this language is already spelled
without NULL, measured by execution:

```
tick 1  doc(1, {"name":"alpha"})   ->  has_name(1,"alpha")
tick 2  doc(2, {"other":"beta"})   ->  no row, key absent
tick 3  doc(3, {"name":null})      ->  no row, key present and null
```

`decode(payload, {name: name})` treats json `null` and a missing key
identically, so three-valued logic never reaches a join. A padded view would put
it back.

The flat one-row-per-enum shape is the right presentation, and
`0_type_plane.pl:11` already places it: "Canonical JSON exists only at the
wire/render boundary." `type_canonical_json/4` is that flattener.

## Red fixtures, ready to paste with the fix

`conformance/fixtures/*.pl` is auto-discovered with no expected-failure
mechanism, so these are staged here rather than committed red. They belong in
`conformance/fixtures/0_enum_variants.pl` in the same commit as the fix.

```prolog
fixture(enum_nullary_variant_boots_and_tags,
    prog(
        [enum_decl(maybe_text, (none() ; some(value:text)))],
        []),
    [],
    [
        [+maybe_text_none(1)],
        [+maybe_text_some(2, "hi")]
    ],
    [
        final(maybe_text_tag/2,
              [maybe_text_tag(1, none), maybe_text_tag(2, some)]),
        ticks(2)
    ]).

fixture(enum_name_is_a_column_type,
    prog(
        [
            enum_decl(grade, (ripe(sugar:int) ; green(days:int))),
            col_type(picked/2, 1, int),
            col_type(picked/2, 2, grade)
        ],
        []),
    [],
    [
        [+grade_ripe(10, 7), +picked(1, 10)]
    ],
    [
        final(grade_tag/2, [grade_tag(10, ripe)]),
        final(picked/2, [picked(1, 10)]),
        ticks(1)
    ]).
```

## Reproduction

```sh
cd v6
S=prolog/compile/scripts/compile_dl6.sh

printf 'rel maybe_text(none() ; some(value: text)).\n' > /tmp/n.dl6
bash $S /tmp/n.dl6 /tmp/n.ts           # rc 0, "wrote"
grep -o 'CREATE TABLE "maybe_text_none"[^`]*' /tmp/n.ts
sqlite3 :memory: 'CREATE TABLE "x" ("id" INTEGER NOT NULL, PRIMARY KEY ()) WITHOUT ROWID;'

printf 'rel grade(ripe(sugar: int) ; green(days: int)).\nrel picked(id: int, g: grade).\n' > /tmp/e.dl6
bash $S /tmp/e.dl6 /tmp/e.ts           # refused rule 'column_type_unknown'
```

## What was measured alongside

Two further observations from the same session, filed here rather than as ARCH
rows because neither blocks the enum work.

**Reading `__rel` compiles but never derives.** A rule body may read the catalog
(`compile.pl:252` states the read/write split), and
`rel_name(local_name) <- __rel(...)` compiles and runs. Tick 1 prints
`"deltas":{}` because catalog rows arrive through boot `INSERT` rather than as an
arrival, so no delta ever reaches the rule. `ARCH.pl` already carries this as
`catalog_g2_oracle_parity`.

The catalog itself is populated and correct. After boot, 20 rows:

```
text|int|float|bool|json     kind=primitive   ids 1-5
p4_read_catalog              kind=module
__rel                        kind=rel     arity=11
  rel_id .. h_rule           kind=column  type_id 2,2,2,1,1,2,2,2,1,1,1
rel_name                     kind=rel     arity=1
  name                       kind=column  type_id=1
```

`type_id` is already right for primitives: every `int` column got 2, every
`text` got 1.

**`__rel` carries an 11-column primary key** including three TEXT hash columns,
against the surrogate-key law in CLAUDE.md. Index entries carry the full width,
so the cost is per-row size. User-parked 2026-08-08 as low priority; `rel_id`
alone is the key.
