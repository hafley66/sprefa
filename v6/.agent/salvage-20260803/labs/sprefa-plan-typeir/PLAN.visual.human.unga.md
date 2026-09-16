# type IR, the human version

## problem

schema of the 9 store tables is hand-written 3 times. they drift. nothing catches it.

```
spine.rs (SQL)     spine.ts + types.ts (TS)     openapi (soon)
   |                    |                          |
   +---- same 9 tables, three hands, no cop ------+
```

## plan

write it ONCE, as prolog facts. print the rest.

```
                 3a_spine_schema_facts.pl
                 table(files, ...).
                 column(files, path, text, ...).
                          |
        +----------------+----------------+
        v                v                v
   TS interfaces    rust structs      openapi json
   (marked zone)    (marked zone)     (arrows later)
        |
   drift test: re-run emitter, diff the zone.
   hand edit = red CI.
```

## mvp (~190 lines, build first)

1. facts file
2. TS emitter into marked zones
3. the drift test

## later rungs, same facts

- rust emitter parity
- arrows (request -> response) print openapi
- `list(T)` generics, monomorphized
- every shape gets a fake SCIP id string so v5's code index can join
  "this type" to "this code"

## your 2 calls

1. NodeRow + EdgeRow live in types.ts, other 7 rows in spine.ts.
   emit two zones (default) or move them together first?
2. fake SCIP ids carry a version segment. pin it to `dev`?
