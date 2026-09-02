# extract: syntax mode and semantic mode, one TSI fact stream

Plain-words twin of `2026-09-02-extract-syntax-semantic-modes.PLAN.md`,
the plan for issue `@extract-semantic-fact-roundtrip`. Receipts and
file:line citations live in the twin.

## TOC

1. The one sentence
2. The board
3. Nine criteria, eight arcs
4. What each mode can say
5. How a guess and an answer live together
6. The reverse door
7. What v7 gets
8. Forks for you

## 1. The one sentence

Extract's cheap parse pass and its checker pass both write TSI relations
(`tsi.edge`, `tsi.parameter`, `ts.readonly`, `rust.impl`, ...) on the JSONL
wire with a run row, a witness row per method, and a coverage row per
relation, and the same wire can be read back, validated, and handed to v7.

## 2. The board

```d2
direction: right
src: source files
syntax: syntax mode {
  parse: tree-sitter / oxc / syn
  rows: tsi rows, targets by written name, coverage partial
}
semantic: semantic mode {
  checker: tsc / rust-analyzer
  rows: tsi rows, native operators, coverage complete
}
wire: JSONL wire {
  protocol: protocol row, version 1
  fact: fact rows, open relation set, registry-checked
  witness: witness rows, one per method
}
ingest: extract --ingest, decode validate canonicalize
foreign: foreign producer, any language
v7: v7 loader, accepted/1, product nodes and :/4 edges
src -> syntax.parse
src -> semantic.checker
syntax.parse -> syntax.rows
semantic.checker -> semantic.rows
syntax.rows -> wire.fact
semantic.rows -> wire.fact
foreign -> ingest
wire.fact -> ingest
ingest -> v7
```

## 3. Nine criteria, eight arcs

| issue criterion | arc | you see it when |
|---|---|---|
| protocol version | A1 | line 1 of a stream is `protocol version=1` |
| decode and validate | A3 | a 4-arg `tsi.edge` is rejected by name |
| reverse door | A3 | `extract --ingest foreign.jsonl` prints sorted canonical rows |
| syntax runs say partial | A1, A4 | `run mode=syntax`, `coverage partial` |
| semantic runs say complete only after a full walk | A5, A6 | `coverage complete` per relation the adapter enumerated |
| TS: generics, optional, readonly, callables, mapped, conditional | A5 | `User<number>` has `tsi.called` and one `tsi.argument` |
| Rust: generics, impls, assoc types, callables, lifetimes, ownership | A6 | `type Output = Vec<T>` has `rust.assoc` |
| TS and Rust fixtures share `tsi.*`, differ in `ts.*` / `rust.*` | A8 | the intersection test is equal |
| DL7 imports accepted rows, semantic replaces syntax | A7 | syntax edges leave `accepted/1` when a complete run lands |

The issue's `## Decisions` note (2026-09-02, origin/main `15e95de83`) is the
contract. A1 and A3 start first, in parallel.

## 4. What each mode can say

| construct | syntax mode | semantic mode |
|---|---|---|
| declared field, name and position | yes | yes |
| field target resolved to a type | written name only | resolved, or `tsi.primitive` |
| optional, readonly | yes | yes |
| generic parameter | yes | yes, with variance |
| generic argument in position | no | yes |
| mapped, conditional type | no | `ts.mapped`, `ts.conditional` |
| associated type | no | `rust.assoc` |
| lifetime, ownership | no | `rust.lifetime`, `rust.ownership` |
| callable inputs and output | written names | resolved |
| conformance | explicit `implements` / `impl` only | checker proof |
| coverage claim | partial | complete where enumerated |

## 5. How a guess and an answer live together

```text
step 0  syntax run 0     tsi.type(t1) origin(User@d1:10..14)   fact 3
step 1                   tsi.edge(e1, t1, name, t2, 1)          fact 7   witness(7, run 0, parse)
step 2                   coverage(run 0, tsi.edge, partial)
step 3  semantic run 1   same key for the edge                  fact 7   witness(7, run 1, checker_walk)
step 4                   tsi.called(t9, t1, a1), argument(a1, 0, number)   new facts, run 1 only
step 5                   coverage(run 1, tsi.edge, complete)
read    accepted(F) <- witness(F, R, _), run(R, semantic, ...).
        accepted(F) <- witness(F, S, _), run(S, syntax, ...), not semantic_complete(Scope, Rel).
steady state: fact 7 stays with two witnesses; a syntax-only edge for the same scope is gone
```

The key never holds an answer. A resolution is its own witnessed row
(`tsi.denotes`, `tsi.has_type`).

## 6. The reverse door

```text
step 0  decode        serde on FlatFact              bad row -> ingest_decode(line)
step 1  registry      relation, arity, arg kinds     unknown -> ingest_relation(line, name)
step 2  id closure    every {"id"} is declared       missing -> ingest_dangling(line, id)
step 3  coverage      complete needs rows            empty   -> ingest_coverage(run, rel)
step 4  canonicalize  renumber ids, sorted_lines
step 5  re-emit       protocol row first, method=foreign witness on every fact
fixed point: ingest(ingest(x)) == ingest(x)
```

## 7. What v7 gets

```d2
direction: right
stream: accepted TSI rows
loader: 6_extract_loader.pl, json_read_dict
product: tsi.product to a product node
edges: "tsi.edge to :(Owner, Label, Target, Position)"
called: tsi.called and argument to intern(Callee, Args)
prims: tsi.primitive to a prelude primitive by class
native: ts.* and rust.* rows as comptime relations, untouched
conforms: Conforms(Source, Contract, Proof) runs unchanged
stream -> loader
loader -> product
loader -> edges
loader -> called
loader -> prims
loader -> native
product -> conforms
edges -> conforms
```

One imported Rust struct:

```text
struct Point { x: u8, y: Option<u8> }

tsi.product(t1)            origin(t1, rust, d1:7..12)
tsi.edge(e1, t1, x, p1, 0) tsi.primitive(p1, unsigned_8)
tsi.edge(e2, t1, y, t3, 1) tsi.called(t3, option, a1)  tsi.argument(a1, 0, p1)
rust.ownership(e1, owned)  rust.ownership(e2, owned)

v7:  :(Point, x, u8, 0)   :(Point, y, intern(Option, [u8]), 1)
```

## 8. Forks for you

| fork | A | B | default |
|---|---|---|---|
| serialization | JSONL on the FlatFact wire | binary columnar later | A |
| cross-repo identity | interned id, SCIP symbol text kept as an arg | SCIP symbol string as the id | A |
| witness granularity | one method slug | derivation graph | A |
| operator normalization | native only, `ts.mapped` | shared `tsi.mapped` beside it | A |
| version | one `protocol` integer | per-namespace versions | A |
| computed TS types | structural identity by edge graph | skip | A |
| syntax-only fact reaching `Conforms` | marked partial | unmarked | A |
| `rust.assoc` as a new relation | keep | fold into `tsi.edge` | A |
| v7 reads JSONL or SQLite | JSONL now | SQLite after A7 | A |

The issue is still `untriaged`. Accepting it is your call:
`issuectl intake accept extract-semantic-fact-roundtrip` from a main checkout.
