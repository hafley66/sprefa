# Clock checker picks how each relation runs

Status: idea, user 2026-10-09 ("each expr had metadata on the ways it outputs/when, is it sync
inside curr frontier, is it stratified frontier (effect) etc. then use that info to flatten ivm
into linear things when makes sense and when actual time data happens we ivm"). Nothing built.
Earlier design: `plans/2026-09-03-dl7-native-rust-sqlite-runtime.md:994` (clock checking as a DL7
analysis over dependency rows with read plane, write plane, sign, grade, role).

## RxJS twin

| clock | RxJS twin | runs as |
|---|---|---|
| const | a literal | computed at compile time |
| once | `of(array)`: emits, completes | one pass: array map, filter, join |
| stratum | `expand` until nothing new, over `of` | a loop to a fixpoint, then done |
| effect | `mergeMap(request)`: answer comes later | one round boundary, batched requests |
| live | `fromEvent`: never completes | DD: kept state, deltas, backpressure |

A relation's clock is the max of its inputs' clocks, in the order above; recursion lifts `once`
to `stratum`.

## Example: the grammar emitter

| step | relation | clock |
|---|---|---|
| 1 | `fs.json "node-types.json"` | once |
| 2 | `kind`, `field`, `field_type` | once |
| 3 | `supertype` closure (recursive) | stratum |
| 4 | doc pieces, `fold doc.join` | once |
| 5 | `fs.file` | written once |

No `live` input, so no DD. With `fs.watch` at step 1, every step becomes `live` and runs in DD.

## Open for Chris

- Are these five clocks the set.
- Does `stratum` stay apart from `once`.
- Where the clock is stored: on the dependency rows (September plan) or on declarations.
