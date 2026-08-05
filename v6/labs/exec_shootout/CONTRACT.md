# exec_shootout CONTRACT — three rust execution strategies, one harness, one number each

Goal: a throughput number per execution strategy for the same datalog
workload, at 10k / 100k / 1M edge scales, so the dynamic-souffle question
(interpret the IR, or monomorphize to rust, and what does each layer of
indirection cost) is answered by measurement.

## TOC
- The workload
- The three engines
- CLI + IO contract (every engine binary implements this)
- Graph families and scales
- Metrics and STANDINGS
- Correctness
- Ground rules

## The workload

One program, semi-naive evaluation REQUIRED (naive re-derivation
disqualifies the number):

```
reachable(x, y) <- edge(x, y).
reachable(x, z) <- reachable(x, y), edge(y, z).
```

Its rx lowering, for the record: `edge$` seeds `reachable$`; each delta
batch of reachable joins the edge index on `y` and feeds back until the
delta is empty (`expand` with an empty-batch stop).

## The three engines (one crate each, one binary each)

| dir | engine | the layer being measured |
|---|---|---|
| `interp/` | IR interpreter | rules live as DATA (structs describing atoms/joins); generic tuple storage; one engine loop reads the IR every batch. Zero per-program types |
| `rxgraph/` | rx operator graph | the program is a graph of boxed operator objects (map/filter/join/distinct) wired at startup; deltas flow through trait-object calls. The dynamic-dispatch indirection is the exhibit |
| `mono/` | compiler-emitted rust | lower `v6/prolog/labs/emit_rust_shootout/emit_rust.pl` with `swipl -g main -t halt`; it writes this crate's `src/main.rs`: concrete u32 types, concrete FxHashMap/Vec indices, the semi-naive loop unrolled for these two rules, seen-set sharded per source node. The readable "generated" source IS the exhibit; keep it one obvious file |

All three implement the same semi-naive algorithm; only the indirection
layer differs. No rayon, single thread (a `--threads` flag may exist,
reserved, default 1).

## CLI + IO contract

```
<engine-binary> --input <path>
```

Input file, plain text: first line `p <nodes> <edges>`, then one `u v` pair
per line, u32 node ids, whitespace separated.

stdout = exactly three JSONL events (machine surface, nothing else on
stdout):

```
{"event":"loaded","edges":M,"ms":<int>}
{"event":"fixpoint","derived":D,"ms":<int>}
{"event":"done","checksum":"<16-hex>","peak_rss_kb":<int>}
```

- `derived` = total reachable pairs (including the edge copies).
- `checksum` = XOR over all derived pairs of
  `fnv1a64(u.to_le_bytes() ++ v.to_le_bytes())`, printed lowercase hex,
  order-independent by construction.
- `peak_rss_kb` from `getrusage` (`ru_maxrss`, divide by 1024 on macOS).
- stderr is free-form logs; the harness captures it to a per-run file.
- Exit 0 on success; nonzero + one stderr line on any failure.

## Graph families and scales

Edges ladder: 10_000, 100_000, 1_000_000. Families:

| family | shape | why |
|---|---|---|
| `chain` | one path, but TRUNCATED closure: nodes = edges, closure capped by construction below | worst-case iteration depth |
| `layered` | DAG, L layers, edges only between adjacent layers, avg out-degree 4, seeded random | join-heavy, controllable closure |
| `grid` | 2D lattice, edges right and down | mixed depth and fan-out |

HARD BOUND: every (family, scale) case must produce between 1M and 20M
derived rows. The harness lane TUNES the family parameters (chain segment
length, layer count/width, grid aspect) to land inside that band, and
RECORDS the chosen parameters in STANDINGS.md. A case outside the band is
re-tuned, never shipped. Per-case timeout 120s; a timeout records DNF.

## Metrics and STANDINGS

`harness/` generates inputs (seeded, deterministic, committed generator,
NOT committed input files), runs every engine binary it is given
(`--engines <bin>[,<bin>...]`), 3 runs per case, best-of-3, and writes
`STANDINGS.md`:

- derived rows/sec for the fixpoint phase (THE number)
- load ms, fixpoint ms, total wall, peak RSS
- per engine: release binary size and cold `cargo build --release` seconds
- the tuned family parameters per case

The harness ships its own tiny naive reference engine (`harness/src/ref`)
used only for checksum truth on the 10k cases and for self-test; its speed
is never reported in standings.

## Correctness

For each case, every engine must agree with every other on (`derived`,
`checksum`); the reference engine anchors truth at 10k. Any disagreement
fails the whole run loudly; no standings are written from a run with a
mismatch.

## Ground rules

- Rust stable, edition 2021, one crate per dir, `cargo` only.
- Allowed deps: `fxhash` or `rustc-hash`, `libc` (for getrusage). Nothing
  else without a STOP-and-report.
- Comments state only constraints the code cannot show, max 2 consecutive
  lines. No em dashes anywhere. Banned words in prose and identifiers:
  provenance, substrate, load-bearing, regime.
- Descriptive identifiers, never single letters (loop indices included).
- Each lane owns ONLY its dir under `v6/labs/exec_shootout/`; the harness
  lane additionally owns `STANDINGS.md` at the shootout root.
- Labs die on landing: results distill into STANDINGS.md and the plan doc;
  the code survives only until the numbers are banked.
