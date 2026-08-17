# Mercury semi-naive reachability

## Build and run

```bash
./build.sh
./mercury-semi-naive --input <path>
```

`build.sh` runs `mmc -O5 --make` and names the executable for harness output.

## Representation

| item | representation |
|---|---|
| input | `io.read_file_as_string` plus an indexed ASCII scanner |
| edge index | CSR offsets and targets in `array(int)` |
| seen targets | one growable dense `array(uint64)` bitset per source |
| delta | a list of `(source, target)` pairs, replaced after each round |
| derived count | updated once when a bit changes from zero to one |
| checksum | bitset traversal after the fixpoint timer stops |

Each source bitset stores a contiguous range of 64-target words. It doubles
when a target falls outside that range. This keeps component-separated chain
inputs from allocating a full node-count bitmap for every source.

## Mercury array update probe

The direct nested update compiled with `mmc 22.01.8`, so no fallback was used.
The probe body was:

```mercury
array.init(2, bitmap.init(2), Seen0),
array.lookup(Seen0, 0, Bits0),
bitmap.set(1, Bits0, Bits),
array.set(0, Bits, Seen0, _Seen).
```

`array(bitmap)` would allocate node-count bits for every source. The entrant
uses the same destructive outer-array update with growable `array(uint64)`
values so the 100k chain case has bounded memory.

## Structural comparison

| concern | mercury-semi-naive | mono |
|---|---|---|
| edge lookup | CSR array slice | `FxHashMap<u32, Vec<u32>>` |
| seen membership | dense bit test | `FxHashSet<u32>` per source |
| mutation | unique-mode array threading | Rust mutable references |
| loop shape | recursive predicates over each delta | unrolled Rust loops |
| pair storage | Mercury list cells | Rust `Vec<(u32, u32)>` |
