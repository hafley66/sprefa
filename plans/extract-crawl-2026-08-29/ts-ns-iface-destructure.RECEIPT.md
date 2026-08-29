# ts namespace / interface / destructured receivers (lane `fix-extract-ts-ns-iface`)

Three `ts.GAPS.md` oracle-only classes, measured against the tsc oracle
(`plans/extract-bench-2026-08-29/ts5.oracle.call.tsv`, 59,356 rows).

## Contents

- [Numbers](#numbers)
- [What each leg does](#what-each-leg-does)
- [Per-leg attribution](#per-leg-attribution)
- [The receiver classes, re-measured](#the-receiver-classes-re-measured)
- [Implementer fan-out: not built, and why](#implementer-fan-out-not-built-and-why)
- [How to reproduce](#how-to-reproduce)

## Numbers

ONE process, `~/projects/TypeScript-5.9`, `find src -name '*.ts' ! -name '*.d.ts'`
(600 files), `--resolve --project-root`, rc=0 every run.

| stage | ours | ours ∩ oracle | recall (∩/ours) | precision (∩/oracle) | drops | wall | peak RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| baseline (#566) | 59,311 | 41,547 | 70.05% | 70.00% | 7,638 | n/a | n/a |
| after closure mirror (#575) | 65,974 | 46,380 | 70.30% | 78.14% | 7,638 | 3.14s | 384 MB |
| after init receivers (#575) | 66,714 | 46,958 | 70.39% | 79.11% | 6,865 | 3.31s | 383 MB |
| after A (imported receiver seats) | 68,975 | 49,060 | 71.13% | 82.65% | n/a | n/a | n/a |
| after B (interface receivers) | 68,721 | 48,422 | 70.46% | 81.58% | n/a | n/a | n/a |
| after C (destructured receivers) | 69,225 | 48,829 | 70.54% | 82.26% | n/a | n/a | n/a |
| all three | 70,799 | 50,383 | 71.16% | 84.88% | 6,800 | 2.03s | 401 MB |

The three A/B/C rows are LEAVE-ONE-OUT: each is the full build with that one
leg disabled, so its distance from the last row is that leg's own contribution.
The legs compete for the same sites, so they do not add up (see below).

Oracle-only rows 12,398 -> 8,973. Ours-only 19,756 -> 20,416.

The three legs add 4,085 rows of which 3,425 (84%) are rows the oracle also
has. Drops fall 6,865 -> 6,800.

Wall and RSS are three back-to-back measurements of the final binary, one
process each: 2.04s / 2.01s / 2.03s and 407 / 400 / 395 MB (3.00s / 385 MB on a
cold page cache). The #575 row's 3.31s / 383 MB was measured on a differently
loaded machine; re-running the #575 binary here gives 4.42s / 408 MB, so this
lane costs no wall and no RSS.

Gate: `cargo test --features cli --no-fail-fast`, 564 passed / 0 failed, rc=0.

An earlier whole-gate run on this tree came back 563/1, the one failure being
`57_rust_module_plane.rs:373`
`barrel_resolve_wall_grows_linearly_with_file_count`
(`wall(400)=0.259s vs wall(200)=0.100s exceeds 2.5x`), a RUST wall-ratio test
this lane does not touch. Re-run three times isolated: ok, ok, ok. It passed in
the final whole-gate run too.

## What each leg does

| leg | site | rule |
|---|---|---|
| A, imported receiver seats | `ts.rs` `receiver_seat` + `imported_member_target`, `ts_resolve.rs` `export_seat` | `TsModuleIndex::bind` joins an export's identifier span to the def node CONTAINING it, and two corpus shapes seat no def node: `export namespace Debug {}` (its members are defs, the namespace is not) and `export const factory: NodeFactory = createNodeFactory(...)` (a const initialized by a call mints no CallF def, `ts.rs` `var_call_defs`). Both returned `Ok(None)` and fell to the name match, which answers only while the member name is corpus-unique. `export_seat` returns the (file, identifier span) without the def-node join; the member then binds inside the namespace, or on the const's DECLARED type anchored in the const's own file. A dotted receiver (`ts.factory`) walks the plane one segment at a time |
| B, interface receivers | `ts_receivers.rs` `seat`, `canonical_decl`, the `TSPropertySignature` arm, `field_on` | three defects. (1) DECLARATION MERGING: `interface Program` is written twice in `types.ts` and is ONE type; `decl_span` kept the first block and `members` keyed on each block's own span, while the module plane names whichever block exported the name LAST, so one block's members were unreachable. (2) PROPERTY SIGNATURES never reached `fields` (class `PropertyDefinition`s only), so `state.program.getTypeChecker()` had no field type to hop through. (3) the field lookup did not walk `extends`, and `TransformationContext.factory` lives on its base `CoreTransformationContext` |
| C, destructured receivers | `ts_receivers.rs` `seed_destructured`, `TypeBinding::Field` | `const { factory } = context` binds each property to its base's type one hop out, reusing the `RecvSpec::Field` shape the `base.field.recv()` leg already had. A destructured receiver is a GUESS and yields to the name match when its hop fails; a directly declared receiver still OWNS its site |

### The ownership policy C had to change

`ts.rs` resolve arm, before this lane: any traced receiver owns its site, and a
member it cannot find is a drop, never a name-match fallback. Applied to
destructured receivers that cost 101 oracle rows on the corpus, all of them
targeting `src/compiler/types.ts`: the destructured name entered scope, which
gated leg A's seat off, and the field hop then failed on the `extends` chain.
Fixing `field_on` recovered them; the fallback stays because a one-hop guess is
weaker evidence than a declared annotation.

## Per-leg attribution

Leave-one-out against the full build (70,799 ours / 50,383 overlap):

| leg | overlap without it | its own contribution |
|---|---:|---:|
| A, imported receiver seats | 49,060 | +1,323 |
| B, interface receivers | 48,422 | +1,961 |
| C, destructured receivers | 48,829 | +1,554 |
| sum of the three | | 4,838 |
| all three together | 50,383 | +3,425 |

The sum exceeds the joint total by 1,413 because the legs answer overlapping
sites: `factory.createX()` is reachable through A (the imported const's
declared type) and, where the file also destructures it off `context`, through
B and C. Whichever leg is left enabled takes the site.

## The receiver classes, re-measured

`ts.GAPS.md`'s oracle-only table was sampled at the #566 binary and its
closure-naming and init-receiver rows were closed by #575. Re-classifying the
12,398 oracle-only rows AT THE #575 BINARY by the receiver spelled at the site
(most common receiver of that callee in the caller's file):

| receiver | rows | leg |
|---|---:|---|
| `<no member site>` (bare call, closure naming) | 4,249 | out of scope here |
| `factory` | 2,823 | A, and C where destructured |
| `program` / `checker` / `host` / `sys` / `resolver` / `typeChecker` | ~1,500 | B |
| `Debug` | 183 | A |
| `context.factory` | 159 | B (field hop) |
| `ts.factory` / `ts` | 251 | A |
| `this.<field>` | ~320 | B (field hop) |
| `tracing` | 41 | out of scope: `tracing?.pop()` is a ChainExpression and `callee_name` (`ts.rs`) mints no site for it |

`src/compiler/types.ts` alone held 4,858 of the 12,398, all of them interface
signatures, which is what put legs A and B on the same target file.

## Implementer fan-out: not built, and why

The brief's task B asked for one edge per implementer of the receiver's
interface, cap 64, kind `Implements`. It is NOT in this PR, and the brief's own
receipt for it does not hold: `grep Implements v6/sprefa-extract/src/lang/ts.rs`
returns nothing, only the go arm emits that kind (`types.rs:446`).

The oracle cannot contain an implementer edge. `plans/extract-bench-2026-08-29/oracle_ts.mjs:162-163`
takes `checker.getResolvedSignature(node).declaration` n/a exactly ONE declaration
per call site, the signature the type checker resolved. For an interface-typed
receiver that is the interface's method signature, which is why the 4,858
oracle-only rows point at `types.ts`. Fan-out edges would therefore be
ours-only by construction: zero new overlap, and the `∩/ours` ratio falls by
however many they are.

Two places in the tree already record that call: `ts.rs` `interface_member_defs`
("a member call on an interface-typed receiver binds the SIGNATURE (the
oracle's coordinate), never an implementer") and `ts.GAPS.md:95` ("implementer
fan-out only where the oracle itself fans out (rare in ts: the oracle binds the
interface signature)").

What task B's gap needed instead was the receiver side, and that is what leg B
built: +1,961 oracle-confirmed rows against the 1,722 the brief projected.

`tests/fixtures/ts5_findings/iface_receiver/impls.ts` carries both implementer
shapes the brief named (a class with `implements`, and an object literal
returned by a factory typed `Session`), and
`72_ts_iface_receiver.rs::an_implementer_is_never_the_dispatch_target` pins that
no dispatch site reaches them. A fan-out leg, if the user wants the call-graph
edges regardless of the oracle, lands on that fixture and is measured as its own
stage row.

## How to reproduce

```bash
cd v6/sprefa-extract && cargo build --release --features cli
cd ~/projects/TypeScript-5.9
/usr/bin/time -l timeout 60 <repo>/v6/sprefa-extract/target/release/extract \
  --resolve --project-root ~/projects/TypeScript-5.9 \
  $(find src -name '*.ts' ! -name '*.d.ts') > /tmp/ts.jsonl
python3 plans/extract-bench-2026-08-29/normalize.py resolved /tmp/ts.jsonl \
  ~/projects/TypeScript-5.9 /tmp/ts.call.tsv /tmp/ts.type.tsv
python3 plans/extract-bench-2026-08-29/bench.py /tmp/ts.call.tsv \
  plans/extract-bench-2026-08-29/ts5.oracle.call.tsv
```

The normalized tsvs are 7 MB each and are not committed (the 1 MB rule).
