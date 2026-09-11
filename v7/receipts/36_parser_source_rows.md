# Parser source-row difference lists

## Acceptance status

Accepted by the user. Exact output is preserved and parser inferences fall
deterministically. The observed 1.06 ms difference between fresh-process wall
medians is recorded as measurement noise rather than a rejection condition.
No checker files were touched by this change.

## Change

`read_dl7/5` keeps its exported signature and output term shapes. Parser
source-row assembly now uses private open-tail lists instead of copying row
lists through `append/3` while recursive forms and top-level forms unwind.

Changed parser predicates:

```prolog
read_dl7/5                 % unchanged public signature
read_top_forms/5           % compatibility wrapper remains
read_top_forms_dl/5        % new internal open-tail result
close_top_rows/2           % new single final tail close
continue_top_forms_dl/3    % new open-tail continuation
read_form_items_dl/8       % renamed internal row-producing path
continue_form_items_dl/5   % new open-tail continuation
finish_form/5
finish_query/7
finish_string/7
finish_symbol/9
finish_variable/10
finish_token/10
```

Successful internal term results carry `Rows0-Rows` as adjacent arguments.
The empty-form and end-of-input paths create an open tail with
`Rows0 == Rows`; only `close_top_rows/2` unifies the final tail with `[]`.
Error results retain the existing diagnostic-only shape and discard partial
rows through `reader_result/4`.

For a form, `finish_form/5` emits `[Source|ItemRows0]-ItemRows`. This preserves
the current source order: the enclosing form row precedes its child rows.
Continuation unification joins the current row tail to the next item or top
level form head, preserving authored order and row multiplicity. Node IDs,
origins, byte offsets, line/column positions, forms, variables, and
diagnostics remain unchanged.

## Exact output comparison

A fresh-process canonical comparison used the six prelude files, the one
macrotime file, `7_nearest_shadow.dl7`, `2_partial.dl7`, `0_minimal.dl7`, and
reader cases for malformed symbols, malformed forms, valid and unterminated
queries, and nested forms. The before and after canonical `read_dl7/5` output
manifests were byte-identical (`cmp` exit `0`, 16 cases).

Compiler canonical SHA-256 probes also remained unchanged:

| fixture | rows | hash |
| --- | ---: | --- |
| `7_nearest_shadow.dl7` | 810 | `d23315e1c3148b13ff8697f0b0b2a51a94cba7c1762ae4081cc1bdc4bddf5186` |
| `2_partial.dl7` | 910 | `8dd2d7dd2571fc18a571e58898cc6e48bbb7c1de2badfb59449dbf8457999748` |

## Measurements

Direct fresh-process prelude parser profile:

| measurement | before | after | delta |
| --- | ---: | ---: | ---: |
| inferences | 344,089 | 328,876 | -15,213 (-4.42%) |
| median wall, 9 runs | 29.80 ms | 30.27 ms | +0.47 ms (+1.58%) |
| bytes | 30,829 | 30,829 | 0 |
| forms | 231 | 231 | 0 |
| source rows | 3,929 | 3,929 | 0 |

Nearest-shadow direct parser inference changed from 3,439 to 3,335. Synthetic
one-form inputs with `N` nested sibling forms measured:

| N | bytes | before inferences | after inferences | delta |
| ---: | ---: | ---: | ---: | ---: |
| 100 | 1,195 | 22,079 | 20,576 | -1,503 |
| 200 | 2,495 | 43,979 | 40,976 | -3,003 |
| 400 | 5,095 | 87,779 | 81,776 | -6,003 |

The parser inference reduction is monotonic on these inputs. The prelude wall
measurements remain recorded below without treating millisecond-scale
fresh-process variation as an acceptance boundary.

The follow-up interleaved measurement used ten serial pairs against the same
30,829-byte prelude text. Each pair started a fresh SWI process and measured
only `read_dl7/5` after loading the parser and text:

```text
pair  before inf  before ms  after inf  after ms
1      344089      29.40     328876       30.98
2      344089      28.30     328876       29.66
3      344089      28.72     328876       29.71
4      344089      29.49     328876       30.33
5      344089      28.26     328876       29.59
6      344089      28.87     328876       30.13
7      344089      28.87     328876       30.05
8      344089      29.74     328876       30.33
9      344089      29.79     328876       29.43
10     344089      29.19     328876       31.26
```

Interleaved medians are `29.03 ms` before and `30.09 ms` after, a `+1.06 ms`
(`+3.65%`) difference. Every pair returned 3,929 rows and empty diagnostics.
The user classified this millisecond-scale wall difference as noise and
accepted the deterministic inference reduction plus exact-output parity. No
further source rewrite was made after this measurement.

The nearest-shadow live performance gate passed with 810 rows and empty
diagnostics. The `2_partial` live process produced 910 rows, eight closure
rounds, and empty diagnostics, then exited on the existing
`compiler_row_checkpoint(910,15562)` failure. The focused performance test's
17 cases passed.

## Tests

- `v7/test/0_reader.test.pl`: 13/13 passed, including the added enclosing-row
  order and multiplicity test.
- `v7/test/1a_syntax_expander.test.pl`: 4/4 passed.
- `v7/test/20_compiler_performance.test.pl`: 17/17 passed.
- Nearest-shadow live gate: passed, 810 rows, empty diagnostics.
- `2_partial` live gate: existing row-checkpoint exit 1, with 910 rows,
  eight rounds, empty diagnostics.

No compiler-kernel, checker, demand-cone, or runtime semantics changed. No CI
files were changed.
