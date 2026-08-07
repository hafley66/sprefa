# Interning across the five generations (sonnet archaeology, 2026-08-07)

Feeds the task #4 contract seed. Question answered: how did each generation
handle string keys, and when did the id+auto-join-view pattern die.

## TOC
- Per-generation table
- The v1 pattern, verbatim
- The v5 living equivalent
- The three deaths
- Seed implication

## Per-generation table

| gen | interned? | dictionary shape | auto-join view? | TEXT leaks into btree keys |
|---|---|---|---|---|
| v1 | yes, universally | `strings(id, value TEXT UNIQUE, norm, norm2)` + `refs(string_id, file_id, spans)`; fact tables carry ONLY `<col>_ref`/`<col>_str` INTEGER | yes: `CREATE VIEW "<rule>"` LEFT JOINs strings per column (`rule_tables.rs:110-136`) | none |
| v2 | yes (verbatim port) | same (`v2/src/store/_5_migrations.rs:14-90`) | yes (`_3_ddl.rs:63-105`) | none |
| v3 | yes (port + mutations table) | same (`v3/.../migrations.rs:1-90`) | yes | none |
| v4 | split | fact store: `sprf_strings` + `<rule>_facts(<col>_id)` (`sql.rs:563-598`); NEW runtime_graph subsystem: raw TEXT PKs, no dictionary (`runtime_graph.rs:1837-1855`) | fact store: TEMP VIEW per connection (downgrade); runtime_graph: "deferred until verified live" (`app.rs:276-278`), never landed | runtime_graph bookkeeping tables |
| v5 early | no, deliberately ("doctrine-blessed") | none | no | every `rel_<name>` with a text column |
| v5 current | yes, BY DEFAULT since ccbd3104 2026-07-12 | `_strings(id, content)` + dense `_sym_dict` in front (2026-07-21) | yes: `create_rel_view()` emits `rel_<name>_txt` decoding every interned column (`src/engine/declare.rs:117-178`) | df coordinate ids intentionally excepted (`src/lower.rs:78`) |
| v6 | no | only `__id` for rel-reference columns (`0_type_plane.pl`) | no | every emitted rel: composite TEXT PK WITHOUT ROWID (`lower.pl:928`) |

## The v1 pattern, verbatim (crates/schema/src/{migrations,rule_tables}.rs)

```sql
CREATE TABLE IF NOT EXISTS strings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    value TEXT NOT NULL UNIQUE,
    norm TEXT, norm2 TEXT
);
CREATE TABLE IF NOT EXISTS "<rule>_data" (
  id INTEGER PRIMARY KEY,
  "<col>_ref" INTEGER,   -- FK into refs (span)
  "<col>_str" INTEGER,   -- FK into strings (value)
  repo_id INTEGER, file_id INTEGER, rev TEXT
);
CREATE VIEW IF NOT EXISTS "<rule>" AS
SELECT t.id, s0.value AS "<col>", s0.norm AS "<col>_norm", ...
FROM "<rule>_data" t
LEFT JOIN strings s0 ON t."<col>_str" = s0.id;
```

## The v5 living equivalent (src/engine/declare.rs:117-178, HEAD today)

```sql
CREATE TABLE _strings (id INTEGER PRIMARY KEY, content TEXT NOT NULL);
CREATE VIEW rel_<name>_txt AS
SELECT COALESCE(
    (SELECT content FROM _strings WHERE _strings.id =
      (SELECT sym_hash FROM _sym_dict WHERE _sym_dict.id = rel_<name>."<col>")),
    (SELECT content FROM _strings WHERE _strings.id = rel_<name>."<col>")
  ) AS "<col>", ...
FROM rel_<name>;
```

## The three deaths

1. v4 runtime_graph: new subsystem built beside the interning fact store,
   skipped the dictionary, view "deferred until verified live", never landed.
2. v5 early: dropped by conscious doctrine; revived 2026-07-12 (ccbd3104
   "feat: make text columns interned by default").
3. v6 compiler: built fresh without the lesson = the second interning
   incident (skills + law 2026-08-07); fix queued as task #4.

## Seed implication

The pattern dies at every rebirth because it is a RETROFIT, never the
emitter's default path. The task #4 seed makes interning the DEFAULT lowering
for text key columns (waiver for cold rels), and the auto-join view is part
of the same emission, not a follow-up. v5's `create_rel_view()` is the
in-repo reference implementation; v1's `rule_tables.rs` is the purest form.
