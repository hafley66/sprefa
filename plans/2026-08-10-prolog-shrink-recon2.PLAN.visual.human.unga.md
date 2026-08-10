# Less Prolog, measured second pass

## TOC

1. Result
2. Mass map
3. What the measurements changed
4. Dispatch board
5. Skip board
6. File-by-file findings
7. Proof required

## Result

One implementation slice remains:

- Delete the second copy of the SQL-template-array renderer.
- Expected deletion: 4 Prolog lines.
- Staffing: luna small, one worktree.
- Proof: generated output stays byte-identical, then unit, round-trip, and
  conformance gates pass.

Everything else measured here is skipped. Most repeated families contain less
source than the shared walker, loader, adapters, or conditional branches needed
to represent them.

## Mass map

```text
Prolog core                                                   23,111
│
├── lower.pl                                                   5,688
│   ├── JSON pattern compiler                    6 shapes / ~83 code lines
│   ├── repeated SELECT branches                            12 lines
│   ├── duplicate catalog ID walks                          10 lines
│   └── primitive/list row walkers                           8 lines
│
├── emit_ts.pl                                                2,799
│   ├── edge resolver generated control flow                126 lines
│   ├── refcount/expand/dred objects                         68 lines
│   ├── ordered arm/occurrence serializers                   55 lines
│   ├── incremental statement entries                        48 lines
│   ├── aggregate SQL objects                                26 lines
│   ├── snapshot serializers                                 18 lines
│   └── duplicate SQL-array renderer                   8 lines -> save 4
│
├── parse_dl.pl                                               2,019
│   ├── 184 predicate names
│   ├── 226 source lines with whitespace/literal calls
│   └── A/B declaration loops                                 8 lines
│
├── analyze.pl                                                1,765
│   └── overlap with program checks                  3 predicate names
│
├── 0_program_check.pl                                          940
│   └── literal shareable overlap                              5 lines
│
├── print_dl.pl                                                  684
│   ├── type inverse                                           7 lines
│   └── modifier inverse                                      12 lines
│
└── registry.pl                                                  624
    └── single-row data clauses                              132 rows

Tests and conformance                                        19,813
Excluded from shrink
```

## What the measurements changed

The first recon projected large savings from visual regularity. Three completed
receipts replace that projection:

| completed work | measured result |
|---|---:|
| dead trees | 25,111 lines deleted |
| descriptor pilot | the profitable catalog slice was taken |
| two later descriptor families | 34 lines added net |
| first serializer family | 7 lines deleted net |

The two later descriptor families each needed a fact table and a recursive
walker. Their fixed cost was about 30 to 40 lines per family. A candidate now
needs clearly more than twice that repeated mass before dispatch.

## Dispatch board

| lane | source mass | new overhead | expected net | model |
|---|---:|---:|---:|---|
| delete duplicate SQL-template-array renderer | 8 | 0 | -4 lines | luna small |

The two predicates have the same three operations: convert SQL strings to JS
templates, join them with commas, and surround them with brackets. One name can
serve all current call sites.

## Skip board

| slice | family mass | expected net | reason |
|---|---:|---:|---|
| analyzer/checker rule helpers | 5 | -2 | export and import leave two lines |
| column-type declaration walkers | 8 | +2 or more | shared module costs more |
| rule constructors | 4 | +4 or more | shared module costs more |
| identity membership | 4 | +4 or more | shared module costs more |
| catalog rel-ID passes | 10 | -1 to +1 | combined traversal is break-even |
| primitive/list catalog walkers | 8 | -1 to +3 | accumulator shapes differ |
| snapshot entry renderers | 10 | -1 to +1 | selector and adapters consume it |
| optional-null serializer clauses | 6 | +2 to +6 | higher-order dispatch adds lines |
| A/B declaration loops | 8 | +2 to +6 | parse semantics differ |
| parser lexeme wrapper | 226 source lines | +2 to +4 | each call still needs one converted line |
| parser production table leaf | 7 | +23 to +33 | signature-family interpreter dominates |
| JSON operation table | 10 replaceable | +20 to +30 | eight threaded arguments remain |
| repeated SELECT-shape branches | 12 | 0 to +2 | adapters consume the helper |
| trigger normalization move | 3 | +1 or more | producer and consumers change |
| snapshot serializer family | 18 | 0 to -4 | below twice fixed overhead |
| incremental statement serializers | 48 | -3 to -13 | below twice fixed overhead |
| refcount/expand/dred serializers | 68 | +2 to -13 | three term shapes remain |
| aggregate SQL serializers | 26 | +4 to +14 | conditional fields remain |
| edge resolver block | 126 | +30 to +40 | generated control flow remains |
| ordered serializers | 55 | -10 to -20 | below twice fixed overhead |
| parser/printer type symmetry | 14 | +16 to +26 | needs two directions plus data |
| parser/printer modifier symmetry | 25 | +35 to +55 | needs two independent walkers |
| registry rows moved to JSON | 132 Prolog rows | repository adds 28 to 36 | displacement increases total lines |

Negative net values in the table mean deleted lines. Positive values mean the
refactor adds lines.

## File-by-file findings

### analyze.pl and 0_program_check.pl

Their predicate-name intersection contains three names: rule-is-edge,
rule-head, and rule-body. The larger invalid-program families already live in
the checker and are imported by the analyzer. The remaining literal duplicate
is five lines. Exporting and importing the helpers leaves two deleted lines, so
no lane is assigned.

### lower.pl

The JSON pattern compiler looks table-shaped from its six top-level pattern
forms. Its work is stateful: source position, alias index, variable bindings,
FROM fragments, WHERE fragments, and recursion all vary by operation. A table
can replace about ten dispatch lines while retaining a 30-to-40-line driver.

The repeated SELECT builders contain twelve lines. A common four-way helper and
four adapters consume twelve to fourteen lines.

The two catalog ID walks perform the same increment. One also accumulates a
name-to-ID map. Combining them saves between negative one and one line.

### emit_ts.pl

Serializer families were counted separately. The largest block is the edge
resolver, but most of its lines generate branches and loops. A descriptor does
not remove those branches.

The ordered serializer has 55 lines and could delete 10 to 20 after a walker.
That mass remains below twice the measured 30-to-40-line family overhead.

The duplicate SQL-array renderer is exact and already has all required
behavior. It needs deletion and call-site renaming only.

### parse_dl.pl and print_dl.pl

A lexeme wrapper changes spelling without reducing line count because explicit
state variables remain at every call.

A production table only covers the small word-to-value leaves. Statement,
declaration, expression, list, and brace productions return different values
and thread different state.

Type and modifier parsing have small inverse printers. A shared data table needs
both directional walkers, which exceed the existing clauses.

### registry.pl

There are 132 rows that could be written as plain data. A loader would rebuild
nested Prolog roles so existing callers continue querying the same terms.
Moving the rows reduces Prolog by about 96 to 104 lines while adding about 28 to
36 repository lines overall. No lane is assigned.

## Proof required

For the one dispatched deletion:

```text
capture generated-output status
            │
            v
       compiler sweep
            │
            v
generated-output status identical?
       │ yes                 │ no
       v                     v
     plunit                 stop
       │
       v
round-trip grades 1, 2, and 3
       │
       v
      pass
```

The generated-output comparison proves byte identity. Unit, round-trip, and
conformance runs cover the behavior gates. A manifest row count alone does not
prove either condition.
