# WAMCompiler lab results

## Environment and pin

| Item | Receipt |
| --- | --- |
| Upstream | `https://github.com/matsud224/wamcompiler` |
| Pin | `d46a2665734d1ed6c7e73ecda9c4e860631cd858` |
| Pin date | 2019-12-18T17:55:20+09:00 |
| Checkout state | exact `HEAD`; `git status --porcelain` empty before every load |
| Runtime | SBCL 2.6.7, arm64 Darwin |
| Dependencies | none beyond SBCL; one source file, no ASDF system |
| Source checkout | `/private/tmp/sprefa-wamcompiler-lab/wamcompiler` |

`2_PROBE.lisp` rejects a mismatched or dirty checkout. It also rejects a
preloaded `CL-USER` WAMCompiler unless the saved image has marked the exact
pin. WAMCompiler has no package declaration, and its parser interns tokens in
`CL-USER`; the probe therefore dynamically binds `*package*` to `CL-USER`
while invoking its public `repl`.

## Commands run

```sh
git clone https://github.com/matsud224/wamcompiler.git \
  /private/tmp/sprefa-wamcompiler-lab/wamcompiler
git -C /private/tmp/sprefa-wamcompiler-lab/wamcompiler rev-parse HEAD
git -C /private/tmp/sprefa-wamcompiler-lab/wamcompiler status --porcelain

WAMCOMPILER_SRC=/private/tmp/sprefa-wamcompiler-lab/wamcompiler \
  sbcl --noinform --disable-debugger --no-sysinit --no-userinit \
  --script v7/labs/10_wamcompiler/2_PROBE.lisp

WAMCOMPILER_SRC=/private/tmp/sprefa-wamcompiler-lab/wamcompiler \
WAMCOMPILER_OUT=/private/tmp/sprefa-v7-wamcompiler-lab-r3 \
  sbcl --noinform --disable-debugger --no-sysinit --no-userinit \
  --script v7/labs/10_wamcompiler/3_BUILD.lisp

WAMCOMPILER_SRC=/private/tmp/sprefa-wamcompiler-lab/wamcompiler \
WAMCOMPILER_BIN=/private/tmp/sprefa-v7-wamcompiler-lab-r3 \
  /private/tmp/sprefa-v7-wamcompiler-lab-r3
```

The source probe executes every non-divergent section in a fresh SBCL child.
The saved image executes each isolated section in a child copy of itself.
The cyclic all-solutions query has a two-second in-process wall bound. The
occurs-check section is isolated because successful cyclic unification causes
the library's answer printer to exhaust SBCL's control stack.

## Abbreviated probe receipt

```text
PROBE library=wamcompiler version=git/d46a266 commit=d46a2665734d1ed6c7e73ecda9c4e860631cd858
UNIFY raw="\nyes.\nX = a\nY = b\n\nyes.\n"
OCCURS occurs-check=absent result=unification-succeeds-cyclic-reification-stack-overflow
PATH raw="\nyes.\nX = b\n?\nyes.\n"
PATH-BOUND timeout-or-transcript=:TIMEOUT
APPEND-LHS raw="\nyes.\nX = [a,b]\n?\nyes.\n"
APPEND-RHS raw=<contains Y = <unbound> and Z = [a,b,G...|-20]>
CUT raw="\nyes.\nX = a\n?\nyes.\n"
INDEX wamcode="switch-on-term ... switch-on-constant {a => ..., b => ...} ..."
INDEX raw="... X = two ..."
UPDATE summary=<item(b) is absent, then X = after once item(b) is added>
NEG raw="\nyes.\n\nyes.\n\nno.\n"
```

The source run ends with `BINARY blocked:not-built`. The saved-image run ends
with `BINARY 40638456`.

The complete raw transcript is reproducible from the command above. The block
above abbreviates the warning-heavy `APPEND-RHS` and `INDEX` records and
summarizes `UPDATE`. It
contains repeated `Builtin predicate current_op/3 cannot be redefined.` lines
when a second prelude load occurs in one child section. The probe preserves
that behavior rather than suppressing it. `APPEND-RHS` demonstrates a free
tail and retains the VM-generated unbound identity text, so it is recorded
raw instead of normalized.

## Bounded fixture behavior

The accepted Prolog subset runs facts, Horn clauses, `?-` queries, `!`,
`\+`, lists, and the bundled `append/3` after a `?- consult('prelude.pl').`
load. The shared cyclic program accepts:

```prolog
edge(a,b).
edge(b,c).
edge(c,a).
edge(c,d).
path(X,Y):-edge(X,Y).
path(X,Y):-edge(X,Z),path(Z,Y).
```

`?- path(a,X).` produces `X = b` as its first answer. Requesting all answers
does not complete within the two-second bound. No finite completed answer set,
table, or visited-state mechanism was observed.

## Capability classification

| Capability | Result | Receipt / source location |
| --- | --- | --- |
| nested term unification | native | `f(X,g(Y)) = f(a,g(b))` prints `X = a`, `Y = b`; VM `unify`, lines 2000-2035 |
| occurs check | absent | the child reaches `CONTROL-STACK-EXHAUSTED` while handling `X=f(X)`; source inspection finds no occurs check in `unify` / `bind` |
| multiple answers | native | WAM choicepoints plus interactive `;` / `a` protocol; `backtrack`, lines 1961-1968 and VM choicepoint instructions |
| fair search | absent-from-probe | WAM depth-first backtracking was exercised; no fair-stream scheduler was found |
| cyclic transitive closure | absent-from-probe | first answer exists; all-answer request reaches the two-second bound |
| Datalog fixpoint | absent-from-probe | no bottom-up rule evaluator or derived-row worklist in the source |
| tabling | absent-from-probe | no call table, answer table, or SCC completion in the source |
| constraints | absent-from-probe | arithmetic builtins exist, but no CLP domain or propagation store was traced |
| dynamic facts and retraction | partial | `item(b)` fails before its clause and succeeds afterward with `X = after`; no `assert` / `retract` deletion API was found |
| cut | native | `pick(a). pick(b):-!. pick(c).` returns `a` then `b`; compiler emits cut code and VM handles `neck-cut` / `cut` |
| predicate indexing | native | `tag/2` prints `switch-on-term` and `switch-on-constant {a => ..., b => ...}` |
| standalone image | built | `r3` runs the current probe and uses child copies of the saved image for isolated sections |

## Saved-image status and measurements

`3_BUILD.lisp` saves an executable only to `WAMCOMPILER_OUT`, an external
pathname. It suppresses probe execution while loading, verifies the exact
checkout, loads the source, marks pinned-image provenance, and installs the
probe `main` as the image toplevel.

| Metric | Receipt |
| --- | --- |
| Image | `/private/tmp/sprefa-v7-wamcompiler-lab-r3` |
| Executable bytes | 40,638,456 |
| SHA-256 | `b9c1e669c87f3288010de75f07ddd07960619272bcd33058c4eea48675629df4` |
| Format | Mach-O 64-bit executable arm64 |
| Dynamic libraries | `/usr/lib/libSystem.B.dylib`; `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` |
| Startup samples | 5.55, 5.59, 5.66, 5.60, 5.57 seconds wall |
| Peak RSS | 257,376,256 bytes from one `/usr/bin/time -l` run |
| Runtime external files | pinned WAMCompiler checkout; bundled `prelude.pl` is loaded from that checkout |
| Wrong-pin behavior | exit 1 before probe execution, with the actual and expected commit hashes |

The image records its build output path for child execution. `WAMCOMPILER_BIN`
overrides that path when the executable is relocated.

## Remaining DL7-relevant components

The source supplies a parser, WAM clause compiler, first-order unification,
trail-based depth-first backtracking, cut, and first-argument dispatching
code. Compiler-rule execution still requires a tabled or finite-fixpoint rule
engine, answer deduplication policy, stratified negation, a constraint store,
and dynamic fact deletion or generation replacement semantics.
