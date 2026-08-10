# DD dance proof, plain words

## TOC

- [1. Goal](#1-goal)
- [2. The dance](#2-the-dance)
- [3. The pieces on the floor](#3-the-pieces-on-the-floor)
- [4. The plan the compiler prints](#4-the-plan-the-compiler-prints)
- [5. The Rust shape](#5-the-rust-shape)
- [6. The pilot](#6-the-pilot)
- [7. The ladder](#7-the-ladder)
- [8. The proof](#8-the-proof)

## 1. Goal

Make a small Prolog program print a dataflow plan. Make a Rust program follow
that plan. Give both programs the same arrivals. Their lines of output must be
the same in the same order.

This is an in-memory proof. There is no database, no SQL, no background task,
and no imported differential-dataflow engine in the generated Rust.

## 2. The dance

Every tick does this:

```text
new rows arrive with + or - signs
        |
        v
put them in the right indexed buckets
        |
        v
use only the changed rows to make the next changed rows
        |
        v
repeat while there is new work
        |
        v
add equal rows together and remove rows whose weight is zero
        |
        v
print the additions and removals for this tick
        |
        v
carry rows that must fire rules next tick
```

The repeated middle section is the fixed point. A recursive rule starts with a
small work batch, makes another batch, and stops when the next batch is empty.

## 3. The pieces on the floor

| current piece | simple meaning | DD-shaped piece | Rust storage |
|---|---|---|---|
| `_sign` | plus or minus on a changed row | signed weight | `(row, i64)` batch entry |
| `frontier` | rows to process now | current work batch | `Vec<Row>` |
| `next_frontier` | rows found for the next round | next work batch | another `Vec<Row>` |
| `dred` | scratch area for removing and checking rows | retraction/rederive work | alternating vectors plus a map |
| `refcount` | how many reasons keep a row alive | total weight per row | `BTreeMap<Row, i64>` |
| `scope` | which groups need a new answer | reduce key set | `BTreeSet<GroupKey>` |
| `avg_accumulator` | saved sum and count for one group | reduce state | `BTreeMap<GroupKey, (f64, i64)>` |
| drain cap | maximum empty follow-up ticks | progress limit | `100` |
| tick | one turn of the clock | dataflow epoch | increasing `u64` |

Some parts need their own explicit operators: ordered input rows, keyed
replacement, log row stamps, retention, and exact JSON printing. They are part
of the runtime's behavior around the dataflow loop.

## 4. The plan the compiler prints

The plan names five things:

```text
relations       what row shapes exist
arrangements    how each row shape is indexed
operators       map, filter, join, reduce, and iterate
wires           which operator sends changed rows to which other operator
tick order      the fixed list of steps for one tick
```

For the small pilot, the picture is this:

```text
arrival source_row
        |
        | +alpha, +beta at tick 1
        v
source_row index  ------------------->  mirror index
                                           |
                                           v
                              print +mirror(alpha), +mirror(beta)

arrival source_row
        |
        | -alpha, -beta at tick 2
        v
source_row index  ------------------->  mirror index
                                           |
                                           v
                              print -mirror(alpha), -mirror(beta)
```

The compiler already knows relation shapes, rule order, rule bodies, recursive
seed and hop shapes, and the current arrival/departure paths. The plan printer
puts those facts into explicit fields before any backend writes its own code.

## 5. The Rust shape

One shared Rust file holds the small engine:

```text
signed batches
ordered maps used as indexes
weight addition and zero removal
fixed-point loop
tick counter and empty-tick cap
JSON line formatter
```

One generated Rust file holds the program:

```text
row types
index keys
operator descriptions
rule-specific functions
the fixture arrivals
```

The kernel is about 260 to 360 lines for the proof. The generated pilot is
about 90 to 140 lines. A general plan printer is about 180 to 260 lines, and a
pilot Rust emitter is about 300 to 450 lines.

## 6. The pilot

The pilot has one rule:

```text
mirror(item) comes from source_row(item)
```

Tick 1 adds `source_row(alpha)` and `source_row(beta)`. The output adds the two
matching `mirror` rows. Tick 2 removes both source rows. The output removes
the two matching `mirror` rows.

That is enough to prove that the output includes an addition batch and a
retraction batch in order.

## 7. The ladder

```text
         A. Prolog prints the plan
                      |
                      v
       snapshot the exact plan text
                      |
                      v
     B. Hand-written Rust follows it
                      |
                      v
       compare every output line
                      |
                      v
      C. Prolog emits that Rust shape
                      |
                      v
        compile it and compare again
```

| step | work size | result |
|---|---:|---|
| A | 180-260 Prolog lines | deterministic plan for the pilot |
| B | 350-500 Rust lines | plan-shaped hand translation and runtime |
| C | 300-450 Prolog lines | generated Rust for the same pilot |

## 8. The proof

The output has one compact JSON line for each tick:

```json
{"tick":1,"deltas":{"mirror":{"add":[["alpha"],["beta"]],"del":[]}}}
{"tick":2,"deltas":{"mirror":{"add":[],"del":[["alpha"],["beta"]]}}}
```

Rows and relation names are sorted the same way on both sides. Empty ticks
still get a line. The test saves the oracle lines and the Rust lines, then
uses a byte comparison. A final empty `mirror` table would only show the end;
these two lines show the whole movement.
