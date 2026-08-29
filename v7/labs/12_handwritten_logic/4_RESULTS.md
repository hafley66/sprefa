# Results

Date: 2026-08-28. Runtime: SBCL 2.6.7. Dependency install route: none.

Kernel: `1a_KERNEL.lisp`, 158 nonblank, noncomment Common Lisp lines. The
source has no external library, ASDF, Quicklisp, cache, or vendored-source
dependency. The SBCL executable dynamically links the runtime libraries
recorded below. `3_BUILD.lisp` requires `HANDWRITTEN_OUT` under
`/private/tmp/`; no executable is generated in this lab.

## Commands and raw probe result

```sh
HANDWRITTEN_OUT=/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic \
  sbcl --noinform --disable-debugger --script 2_PROBE.lisp
```

```text
PROBE library=handwritten-cl-kernel version=local
UNIFY (PAIR A (G B))
OCCURS occurs-check=REJECTED
FAIR disjunction-left=diverge answer=RIGHT conjunction-left=diverge answer=RIGHT
PATH adapter=answer-limit-12 answers=("A" "B" "C" "D")
UPDATE adapter=rebuild-after-retraction answers=("A" "B" "C")
BINARY path=/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic bytes=42080472
```

`FAIR` has two deterministic starvation receipts. The first has a suspended,
nonproductive disjunction branch. The second makes the first conjunction
candidate diverge. Each alternate returns `RIGHT` with `take-stream` limited to
one answer.

The cyclic `path(a,Y)` fixture uses only the labeled `answer-limit-12` adapter.
It has no completion check. The update fixture constructs a new relation after
removing `edge(c,d)`.

## Capability classification

| Capability | Result | Observed policy or boundary |
| --- | --- | --- |
| nested term unification | implement | `unify` recursively unifies cons terms using persistent association-list substitutions. |
| occurs check | implement | `X = (F X)` returns `REJECTED`; extensions call `occurs-p`. |
| multiple answers | implement | Lazy streams return proof-path answers in stream order. |
| fair search | implement | `mplus` swaps a suspended left stream with its alternate; `bind` delegates branches through `mplus`. The `FAIR` probe reaches `RIGHT` for disjunction and conjunction. |
| cyclic transitive closure | adapter | `answer-limit-12` yields `A`, `B`, `C`, and `D`; no completion semantics. |
| Datalog fixpoint | absent-from-probe | No bottom-up or seminaive worklist. |
| tabling | absent-from-probe | No variant, subsumptive, or answer table. |
| constraints | absent-from-probe | No constraint domain or propagation store. |
| dynamic facts and retraction | adapter | Rebuild clauses after removing `edge(c,d)`; no incremental repair. |
| standalone image | built | External SBCL executable receipt below. |

## Standalone image receipt

```sh
HANDWRITTEN_OUT=/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic \
  sbcl --noinform --disable-debugger --load 3_BUILD.lisp
HANDWRITTEN_OUT=/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic \
  /private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic
wc -c /private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic
shasum -a 256 /private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic
file /private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic
otool -L /private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic
```

```text
42080472
be4fc038ee5f3af2e476684d491b717401ed6d550fff69e54acfe923b23c661c
/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic: Mach-O 64-bit executable arm64
/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic:
    /usr/lib/libSystem.B.dylib
    /opt/homebrew/opt/zstd/lib/libzstd.1.dylib
```

`BINARY` is dynamic: the probe opens `HANDWRITTEN_OUT` as an unsigned-byte
stream and calls `file-length`. No executable byte count is embedded in source.
The saved-image `main` runs the probe and exits. A `--eval` compile probe did
not run before that main function, so source loading and compilation are not
exposed through this executable's command-line interface.

## Measurements

```sh
HANDWRITTEN_OUT=/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic \
  /usr/bin/time -p /private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic >/dev/null
HANDWRITTEN_OUT=/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic \
  /usr/bin/time -l /private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic >/dev/null
```

Startup samples, wall seconds: `0.01, 0.01, 0.01, 0.01, 0.01`.

RSS sample: `46743552 maximum resident set size`. The same invocation reported
`46417088 peak memory footprint`.

## Report questions

1. Direct SWI capability coverage is absent. The local kernel implements
   unification, multiple answers, fair streams, and Horn clause expansion. It
   has no SWI tabled recursion, CLP, CHR, negation, dynamic database, or
   fixpoint engine.
2. Cyclic closure and updates use adapters: bounded answer collection and
   relation rebuilding. The remaining listed implemented capabilities execute
   in the kernel.
3. Cycle recursion has no native termination algorithm. `answer-limit-12`
   stops enumeration after twelve proof-path answers.
4. SBCL built the Mach-O arm64 executable at the external path above.
5. The executable is `42080472` bytes with SHA-256
   `be4fc038ee5f3af2e476684d491b717401ed6d550fff69e54acfe923b23c661c`;
   dependencies, startup samples, and RSS are recorded above.
6. `1a_KERNEL.lisp` implements unification (`walk`, `occurs-p`, `unify`),
   search (`mplus`, `bind`, `disj`, `conj`), and rule evaluation (`fact`,
   `horn`, `relation`). It implements no caching.
7. Remaining DL7 receipts include parsing, source locations, compiler-rule
   encoding, tabled completion, fixpoint execution, negation, constraints, and
   update repair.
