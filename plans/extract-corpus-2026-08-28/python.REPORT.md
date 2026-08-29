# sprefa-extract corpus battery: python

Corpus: CPython 3.14 stdlib, `/opt/homebrew/opt/python@3.14/.../lib/python3.14/`.
Binary: `v6/sprefa-extract/target/release/extract`, base `8e946ada9` (PR #524).

## Contents

1. [Corpus shape](#1-corpus-shape)
2. [Step 1: per-file default](#2-step-1-per-file-default)
3. [Step 2: per-file by family](#3-step-2-per-file-by-family)
4. [Step 3: --resolve per package](#4-step-3---resolve-per-package)
5. [Step 4: --family diet_scip](#5-step-4---family-diet_scip)
6. [Step 5: --family scip](#6-step-5---family-scip)
7. [Perf and RSS](#7-perf-and-rss)
8. [Parse errors](#8-parse-errors)
9. [Construct probes](#9-construct-probes)
10. [Findings](#10-findings)
11. [Fixes landed](#11-fixes-landed)
12. [What stays untested and why](#12-what-stays-untested-and-why)

## 1. Corpus shape

| measure | value |
| --- | --- |
| `.py` files | 1852 |
| bytes | 36,036,062 |
| files under `test/` | 1131 |
| files under `lib2to3` | 0 (removed in 3.14) |
| `.py` files under `site-packages` | 0 |
| package dirs with 2+ files | 125 |
| files CPython 3.14 `compile()` rejects | 5 |

## 2. Step 1: per-file default

`extract <file>`, one process per file, `xargs -P 8 -n 1`, every call under
`timeout 10`. Raw table: `python.runs.tsv` (path, rc, ms, lines, bytes, stderr).

| measure | before the fixes | after the fixes |
| --- | --- | --- |
| files run | 1852 | 1852 |
| rc != 0 | 0 | 0 |
| rc = 124 (timeout) | 0 | 0 |
| stderr non-empty | 0 | 0 |
| zero-line output | 3 | 3 |
| total fact lines | 16,171,268 | 16,653,924 |
| wall ms p50 / p90 / p99 / max | 84 / 184 / 681 / 1206 | 18 / 88 / 412 / 855 |

The fixes in section 11 add 482,656 fact lines over 1307 files and remove none.
The two wall-ms columns were measured under different background load and are
not a speed claim; the max is the number that matters, and it stays 14x inside
the 10-second law.

The 3 zero-line files are all non-UTF-8 (finding F9).

## 3. Step 2: per-file by family

200-file sample: the 100 largest plus 100 drawn from the rest.

| family | lines |
| --- | --- |
| default (all) | 7,387,848 |
| `--family cst` | 5,190,198 |
| `--family type` | 40,589 |
| `--family call` | 215,793 |
| `--family df` | 1,941,268 |
| sum of the four | 7,387,848 |

Files where the sum exceeds the default: 0. Files where it falls short: 0. The
family mask partitions the default stream exactly, on all 200 files.

## 4. Step 3: --resolve per package

`extract --resolve <dir>/*.py` over each of the 125 package dirs.

| measure | value |
| --- | --- |
| dirs | 125 |
| files covered | 1831 |
| call sites | 383,919 |
| `resolved_edge` rows | 100,486 |
| corpus unresolved ratio | 0.7383 |
| slowest dir | `test`, 448 files, 6672 ms |

Ranked by unresolved ratio, sites >= 200:

| ratio | sites | resolved | dir |
| --- | --- | --- | --- |
| 0.9610 | 769 | 30 | `test/test_json` |
| 0.9462 | 390 | 21 | `test/test_zipfile/_path` |
| 0.9430 | 1771 | 101 | `test/test_ttk` |
| 0.9276 | 1077 | 78 | `test/test_warnings` |
| 0.9263 | 2726 | 201 | `test/test_sqlite3` |
| 0.3714 | 762 | 479 | `xml/dom` (lowest) |
| 0.4539 | 2529 | 1381 | `email` |
| 0.4636 | 302 | 162 | `xml/sax` |

Why the top 5 miss, classified by whether the callee name has a def among the
supplied paths:

| dir | unresolved | no-def-in-dir | ambiguous-fn | ambiguous-class | class-only-def |
| --- | --- | --- | --- | --- | --- |
| `test/test_json` | 739 | 739 (100%) | 0 | 0 | 0 |
| `test/test_ttk` | 1670 | 1670 (100%) | 0 | 0 | 0 |
| `test/test_warnings` | 999 | 999 (100%) | 0 | 0 | 0 |
| `test/test_zipfile/_path` | 369 | 361 (98%) | 0 | 0 | 0 |
| `encodings` | 1083 | 885 (82%) | 1 | 196 (18%) | 0 |
| `lib/python3.14` (155 files) | 15253 | 9305 (61%) | 5250 (34%) | 114 | 221 |
| `test/test_asyncio` | 12029 | 10418 (87%) | 1521 (13%) | 85 | 5 |
| `idlelib` | 3228 | 2606 (81%) | 609 (19%) | 2 | 4 |

Four of the top five are unittest suites: the dominant unresolved callees are
`assertEqual` (232 in `test_json`, 310 in `test_ttk`, 169 in `test_warnings`),
`assertRaises`, and builtins (`len`, `str`, `isinstance`). Those are defined
outside the supplied path set, so no edge is the correct answer, not a defect.

`encodings` is the one structural case: 196 unresolved `Codec()` calls where
`class Codec` is defined in ~120 sibling files
(`encodings/cp1252.py:8` and its peers). One bare name, 120 candidate blobs,
so the name matcher declines. This is the documented diet weakness in
`extract --help` FAST MODE, working as specified.

`class-only-def` (221 at the top level, e.g. `IPv6Network` 35,
`ArgumentDescriptor` 24, `auto` 22) is a residual I could not close: the same
construct resolves when the pair is isolated. `extract --resolve ipaddress.py
enum.py` emits the `IPv6Network` edges (with a null name, F1), while the
155-file run leaves 35 of those sites without a row. Cause not identified;
1.4% of the top-level residual.

## 5. Step 4: --family diet_scip

Same 125 dirs, same file sets.

| measure | `--resolve` | `--family diet_scip` |
| --- | --- | --- |
| `resolved_edge` rows | 100,486 | 100,486 |
| dirs where the counts differ | 0 of 125 | |
| slowest dir | `test` 6672 ms | `test` 6010 ms |

The call arms are the same code path. `diet_scip` additionally emits
`resolved_type_edge`; `--resolve` emits it only under `--family type`. On the
`email` package that is 1381 call edges plus 94 type edges.

## 6. Step 5: --family scip

`--family scip` over a python root with no marker file returns one
`scip_skip`, verbatim:

```
{"record":"scip_skip","lang":"none","bin":"none","reason":"no_markers","detail":"no marker file at the root; looked for CMakeLists.txt, Cargo.toml, build.gradle, build.gradle.kts, compile_commands.json, go.mod, package.json, pom.xml, pyproject.toml, requirements.txt, setup.py, tsconfig.json"}
```

A stdlib package directory carries none of those. Adding a two-line
`pyproject.toml` to a scratch copy dispatches `scip-python` and the run
succeeds, so this is a corpus property, not a missing arm.

| package | scip rows | `scip_def` | `scip_fn_edge` | `resolved_edge` | ms |
| --- | --- | --- | --- | --- | --- |
| `json` | 955 | 235 | 267 | 51 | 44 |
| `email` | 7899 | 2061 | 2130 | 1381 | 70 |
| `asyncio` | 16563 | 3900 | 5040 | 1538 | 169 |

`scip_fn_edge` counts every symbol a function body mentions, including
parameters and keyword-argument names (`json /dump().` -> `json /allow_nan.`),
so a like-for-like comparison first drops callees whose symbol is not a
function (`().`) or class (`#`).

| package | scip pairs | resolve pairs | both | scip only | resolve only |
| --- | --- | --- | --- | --- | --- |
| `json` | 45 | 36 | 19 | 26 | 17 |
| `email` | 616 | 772 | 356 | 260 | 416 |
| `asyncio` | 1272 | 1324 | 666 | 606 | 658 |

20 edges sampled uniformly from the union of the two one-sided sets across all
three packages (`random.seed(7)`), each classified:

| n | side | example | class |
| --- | --- | --- | --- |
| 9 | scip only | `email/_header_value_parser.py stripped_value -> TokenList`, `asyncio/windows_events.py accept_coro -> exceptions.py CancelledError` | constructor edge: the resolver DOES emit the row, with `callee_name: null` (F1), so the pair never matches on name |
| 6 | resolve only | `asyncio/queues.py __init__ -> locks.py set`, `email/message.py _get_params_preserve -> header.py append` | bare-name collision; several classes define the method, the matcher picked one blob |
| 4 | scip only | `asyncio/unix_events.py __init__ -> selector_events.py __init__`, `asyncio/graph.py capture_call_graph -> futures.py _asyncio_awaited_by` | a member the name matcher cannot see: inherited through a base class, or reached through a value rather than a syntactic call |
| 1 | resolve only | `email/message.py set_payload -> charset.py <null>` | the same F1 constructor row, seen from the resolver side |

Every scip-only miss in the sample is one of two shapes: F1 (the name is
present in the row but null) or an edge a parse-only matcher structurally
cannot see. No sampled edge was a case the name matcher should have got right
and didn't.

## 7. Perf and RSS

Bytes per millisecond, files over 2 KB: 5th percentile 29.8 B/ms. Every file
below that floor is small (2-3 KB) and the cost is process start under `-P 8`,
not a construct. Re-measured with no contention:

| file | under `-P 8` | serial |
| --- | --- | --- |
| `test/support/_hypothesis_stubs/__init__.py` (2444 B) | 290 ms | 79 ms |
| `test/test_importlib/resources/test_open.py` (2673 B) | 272 ms | 39 ms |
| `urllib/response.py` (2361 B) | 151 ms | 40 ms |

`/usr/bin/time -l` over the 20 largest files:

| max RSS | bytes | RSS/byte | file |
| --- | --- | --- | --- |
| 92.8 MB | 395,167 | 246x | `test/test_typing.py` |
| 66.1 MB | 298,121 | 233x | `test/datetimetester.py` |
| 61.3 MB | 223,804 | 287x | `test/test_decimal.py` |
| 59.9 MB | 200,462 | 313x | `test/test_enum.py` |
| 9.0 MB | 582,896 | 16x | `pydoc_data/topics.py` |

Peak is 92.8 MB. The multiplier tracks node count, not bytes:
`pydoc_data/topics.py` is the largest file in the corpus and the cheapest,
because it is one dict of long string literals (1680 fact lines against
test_typing's 215,069).

## 8. Parse errors

CPython 3.14 `compile()` rejects 5 files:

| file | CPython message |
| --- | --- |
| `test/test_future_stmt/badsyntax_future.py` | `from __future__` imports must occur at the beginning of the file |
| `test/tokenizedata/bad_coding.py` | unknown encoding: uft-8 |
| `test/tokenizedata/bad_coding2.py` | encoding problem: utf8 with BOM |
| `test/tokenizedata/badsyntax_3131.py` | invalid character '€' (U+20AC) |
| `test/tokenizedata/badsyntax_pep3120.py` | Non-UTF-8 code, no encoding declared |

tree-sitter emits ERROR nodes in 9 files, 83 nodes total. One of them
(`badsyntax_3131.py`) is a file CPython also rejects. The other 8 are VALID
Python 3.14 the pinned grammar cannot parse:

| construct | PEP | files | ERROR nodes | lines lost |
| --- | --- | --- | --- | --- |
| t-string literal `t"..."` | 750 | `string/templatelib.py`, `annotationlib.py`, `test/test_tstring.py`, `test/test_string/test_templatelib.py`, `test/test_annotationlib.py` | 66 | 66 |
| dedented continuation inside brackets, `(bar.\n baz)` | none | `test/test_compile.py:2360` | 9 | 541 |
| type-param defaults `def f[T: int = int, **P = int, *Ts = int]` | 696 | `test/test_type_params.py:1425` | 3 | 3 |
| unparenthesized `except A, B:` / `except* A, B:` | 758 | `test/test_grammar.py:1412`, `:1437` | 2 | 2 |
| star-unpack in a subscript, `Union[*args, *(kw.values())]` | 646 | `test/test_annotationlib.py:186` | 1 | 1 |
| non-ASCII identifier `€` | n/a, real error | `test/tokenizedata/badsyntax_3131.py` | 2 | 2 |

The `test_compile.py` case is the expensive one: 9 ERROR nodes swallow 541
lines of otherwise ordinary source.

`print "x"` and `exec "x"` parse without an ERROR node: the grammar keeps the
Python 2 `print_statement` and `exec_statement` productions. No Python 2 file
exists in the 3.14 corpus (`lib2to3` was removed), so the "expected parse
errors" count for Python 2 syntax is 0.

## 9. Construct probes

Each probe is a hand-written file run through `--family type,call,df`.

| construct | result |
| --- | --- |
| `match` statement, 6 case shapes | all 6 case bodies mint call sites |
| walrus `:=` | reads flowed, no binding (F5, fixed) |
| `async def` / `await` / `async with` / `async for` | entities and call sites correct |
| decorators, `@property` / `@staticmethod` / `@classmethod` | 4 method defs; `@lru_cache(...)` mints its call site |
| `*args` / `**kwargs`, `/` and `*` separators | param positions correct, splat call sites correct |
| nested classes | `Outer.Inner()` resolves the callee to `Inner` |
| `from . import x`, `from .. import x`, `from .mod import x` | module recorded as `.`, `..`, `.mod` |
| `import a.b.c as abc` | name `abc`, module `a.b.c` |
| `from x import *` | one `namespace` specifier |
| `from __future__ import x` | no specifier (F6, fixed) |
| `if TYPE_CHECKING:` imports | specifiers emitted, guard is transparent |
| `X | None` annotations | union members collected, builtins filtered as noise |
| PEP 695 `def f[T]()`, `class Box[T]` | entities correct, `T` correctly excluded from sigs |
| PEP 695 `type X = ...` | no entity (F7, fixed) |

## 10. Findings

`lang | class | path:line | repro | observed | expected`

| # | class | site | repro | observed | expected | status |
| --- | --- | --- | --- | --- | --- | --- |
| F1 | wrong_fact | `src/project.rs:889` | `extract --resolve tests/fixtures/python/corpus_8.py tests/fixtures/python/corpus_9.py` | the `Widget()` edge carries `callee_name: null` | `callee_name: "Widget"` | OPEN, outside my arm |
| F2 | parse_error | `Cargo.toml:83` (tree-sitter-python 0.23.6) | `extract --family cst tests/fixtures/python/corpus_10.py` | ERROR node at the `t` prefix, enclosing scope lost | a parse | OPEN, outside my arm |
| F3 | parse_error | same | `extract --family cst .../test/test_compile.py` | 9 ERROR nodes covering 541 lines from a dedented continuation inside brackets | a parse | OPEN, outside my arm |
| F4 | missing_fact | `src/lang/python/_0_source.rs:1804` | `extract .../test/encoded_modules/module_iso_8859_1.py` | rc=0, zero rows, zero diagnostics | rc=1 per the documented exit codes, or a named skip row | OPEN, see section 12 |
| F5 | missing_fact | `src/lang/python/_0_source.rs` df walk | `tests/fixtures/python/corpus_6.py` | walrus binds nothing; later reads are free names | a `let_bind` fed by the rhs | FIXED |
| F6 | missing_fact | same, import walk | `tests/fixtures/python/corpus_1.py` | zero specifier rows | two `named` rows, module `__future__` | FIXED |
| F7 | missing_fact | same, entity walk | `tests/fixtures/python/corpus_2.py` | zero entities | two `alias` entities | FIXED |
| F8 | wrong_fact | same, df walk | `tests/fixtures/python/corpus_3.py` | `x += e` never rebinds; the return reads the binding the update replaced, and `e` dangles | a `let_bind` fed by the read and by `e` | FIXED |
| F9 | missing_fact | same, df walk | `tests/fixtures/python/corpus_4.py` | `with` context expression flows nothing; `as` target unbound | `open(path)` mints a `call_res` feeding the `fh` binding | FIXED |
| F10 | missing_fact | same, df walk | `tests/fixtures/python/corpus_5.py` | `except E as err` binds nothing | a `let_bind` for `err` | FIXED |
| F11 | missing_fact | same, df walk | `tests/fixtures/python/corpus_7.py` | `if` / `elif` / `assert` / `raise` expressions mint no df node | one node per call, params feeding the reads | FIXED |

Corpus frequency of the fixed gaps, counted with CPython's own `ast` over all
1852 files:

| construct | occurrences | files |
| --- | --- | --- |
| `if` statement | 31,687 | 1417 |
| `with` statement | 17,639 | 847 |
| `with ... as NAME` | 6,137 | 552 |
| `except` handler | 6,230 | 805 |
| augmented assignment | 3,194 | 547 |
| `except ... as NAME` | 1,580 | 375 |
| `assert` | 1,371 | 338 |
| `match` | 327 | 26 |
| walrus | 274 | 115 |
| PEP 695 type alias | 54 | 9 |
| `from __future__ import` | 43 | 41 |

## 11. Fixes landed

All inside `v6/sprefa-extract/src/lang/python/_0_source.rs`. Failing test
first, then the fix. Test file `tests/43_python_corpus_gaps.rs`, 7 tests, all
7 red before the change and green after.

| fix | change |
| --- | --- |
| F6 | `py_walk_imports` matches `future_import_statement` alongside `import_from_statement`; the module name is the keyword, not a field |
| F7 | `walk_py_entities` gets a `type_alias_statement` arm minting `TypeEntityKind::Alias` from the head identifier, `Alias` or the container of `Pair[T]` |
| F8 | `py_flow_augmented` reads the target, flows the rhs, then rebinds the target from both; the rebind carries the statement span so it never collides with the read |
| F9 | `py_flow_with` walks each `with_item`, flows the context expression, and binds an `as` target from it |
| F10 | `py_flow_except` binds the `as_pattern_target` with no incoming edge; the exception type is a type, never a value read |
| F11 | `py_flow_stmt` gets `if_statement` / `elif_clause` (condition as an expression, every other child as a suite) and `assert_statement` / `raise_statement` |
| F5 | `py_flow_expr` gets a `named_expression` arm returning the binding as the expression's value |

Effect on the corpus: 16,171,268 fact lines to 16,653,924, over 1307 of 1852
files, with no file losing a line and no new non-zero exit.

Gate: `cargo test --features cli` 360 passed, 0 failed, 2 ignored.
`cargo fmt --check` clean. `cargo clippy --features cli --all-targets` reports
nothing on the arm or the new test.

## 12. What stays untested and why

| gap | why |
| --- | --- |
| `match` statement dataflow | capture patterns (`case Point(x=px)`) bind names the df walk does not model. Call sites inside case bodies are correct. 327 occurrences over 26 files; modelling pattern binding is a design question, not a bug fix, so it stays a finding and not a change |
| `del` / `global` / `nonlocal` scope effects | the df model has no unbind and no module-scope plane; a `del x` followed by a read is not distinguishable from a live read |
| `else` and `finally` suites of `while` / `try` | reached through the generic recursion, so statements inside them flow, but the branch structure is not a node |
| F1 fix | the null name comes from `name_at` at `src/project.rs:889` probing only the CallF span table. The brief forbids editing outside `src/lang/python/**` |
| F2, F3 fix | the grammar version is pinned in `v6/sprefa-extract/Cargo.toml:83`, outside my arm. tree-sitter-python 0.25 is on crates.io and the repo already resolves it for another crate, so the upgrade is a one-line experiment for whoever owns the manifest |
| F4 fix | the silent drop is `if let Ok(src) = std::str::from_utf8(content)` at `src/lang/python/_0_source.rs:1804`, which IS my arm, but go (`go.rs:1691`), kotlin (`kotlin.rs:1466`), rust (`rust.rs:2768`) and ts (`ts.rs:3061`) all carry the identical line. Changing one arm to speak up makes the corpus answer language-dependent, and the exit code the help documents is decided in `src/bin/extract.rs`. This wants one cross-arm decision, not a python-local edit |
| `class-only-def` residual (section 4) | 221 sites at the top level whose defs exist and whose isolated pair resolves. Not reproduced in a minimal case, so no fixture and no claim about the cause |
| `--family scip` on the real stdlib tree | the stdlib carries no marker file. Measured on scratch copies of `json`, `email`, `asyncio` with a two-line `pyproject.toml`, which is what step 5 asked for; the untouched tree stays a `scip_skip` |
| `--family cfg` | `extract --help` states rust, go, ts and kotlin have the `kind_role` rows cfg needs, and python emits no cfg rows. Confirmed 0 on every probe; adding them is a new arm, not a corpus finding |
