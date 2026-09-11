# Receipt 44: identifier validation cache candidate

## Disposition

Rejected without source changes. The parser validation predicate has repeated
ground inputs, but the per-read cache bookkeeping costs more wall time than the
validation scans on the measured fixtures. `v7/src/0_reader/0_parser.pl` and
`v7/test/0_reader.test.pl` are clean after rollback.

## Current call path and modes

`valid_identifier_codes/1` is a `+Codes` semidet predicate. Its only result is
success or failure. `Codes` is the already-created list of character codes;
the predicate has no path, source-position, origin, or diagnostic argument.

The direct parser callers are:

| Caller | Nearest-shadow | 2_partial |
| --- | ---: | ---: |
| `finish_token/9` | 1,181 | 1,231 |
| `finish_variable/10` | 1,290 | 1,322 |
| Total calls | 2,471 | 2,553 |
| Unique ground code lists | 275 | 292 |
| Repeated calls | 2,196 | 2,261 |
| Invalid results | 0 | 0 |

The predicate is also called from `valid_atom_codes/1` in the symbol path. The
measured compiler profiles above captured the parser callers through the full
reader/compiler path. Validation is pure with respect to the code list and the
fixed `ascii_alpha/1` and `identifier_rest_code/1` grammar predicates.

## Candidate

The candidate wrapped the body of `read_dl7/5` in
`setup_call_cleanup/3`, initialized a per-read assoc, and looked up each ground
code list before running the existing validator. A dynamic thread-local assoc
was measured first. A non-backtrackable per-thread assoc slot using
`nb_setval/2` and `nb_current/2` was also measured to remove repeated
`retractall/1` and `assertz/1` operations. Both versions were removed.

The cache stored only `valid` or `invalid`. Caller construction of symbols,
variables, source rows, origins, and diagnostics remained unchanged.

## Measurements

Direct clean compiler probes, each in a fresh SWI process:

| Fixture | Baseline | Assoc candidate | Delta |
| --- | --- | --- | --- |
| nearest-shadow | 1,899,633 inferences, 538.01 ms | 1,839,476, 484.72 ms | -60,157, -53.29 ms |
| 2_partial | 3,996,513 inferences, 770.80 ms | 3,935,798, 851.69 ms | -60,715, +80.89 ms |

Interleaved fresh-process nearest-shadow wall samples for the `nb_setval/2`
candidate were `474.46, 456.46, 458.78 ms`, median `458.78 ms`. Baseline
samples were `429.63, 456.19, 410.49 ms`, median `429.63 ms`.

Parser-only fresh-process `2_partial` samples were:

| Mode | Samples, ms | Median |
| --- | --- | ---: |
| Candidate cache | 2.45, 2.46, 2.17 | 2.45 |
| Baseline | 1.90, 1.63, 1.68 | 1.68 |

## Exact parity checks

The candidate produced the existing canonical hashes:

| Fixture | Rows | Diagnostics | SHA-256 |
| --- | ---: | --- | --- |
| nearest-shadow | 810 | `[]` | `f42d4b6a73ce4cdd97594ff9710377c499a8c78e2ff8987517793cbbca29a99a` |
| 2_partial | 910 | `[]` | `d47c0778343e976d0fc648324cbb4003c643543ee00b61e817c407ae4ce69519` |

Candidate focused tests passed: reader `14/14`, syntax expander `4/4`.
After rollback, reader `13/13` and syntax expander `4/4` passed. The nearest
shadow live gate passed with 810 rows and empty diagnostics. The existing
`2_partial` live gate still reports its pre-existing checkpoint mismatch,
`compiler_row_checkpoint(910,15562)`, after producing 910 rows, eight closure
rounds, and empty diagnostics.

## Files and semantics

No parser or test source changes remain. No kernel, type, binding, phase, source
origin, diagnostic, ordering, or multiplicity behavior was changed. Unrelated
dirty paths were preserved.
