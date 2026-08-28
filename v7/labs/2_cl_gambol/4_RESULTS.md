# cl-gambol lab results

## Environment

| Item | Value |
| --- | --- |
| Library | cl-gambol 0.03, upstream commit `d4d53a1e29a360f8aaab9134da89b8c6966fe16e` |
| Runtime | SBCL 2.6.7 (Homebrew arm64) |
| Host | macOS (Darwin, arm64) |
| Dependency route | direct `asdf:load-asd` of the temp checkout; no Quicklisp |
| Checkout location | `/var/folders/z2/.../opencode/cl-gambol` (temporary; not in git) |

## Commands

```sh
GAMBOL_SRC=<checkout> sbcl --noinform --disable-debugger --script 2_PROBE.lisp
GAMBOL_SRC=<checkout> sbcl --noinform --disable-debugger --script 3_BUILD.lisp
stat -f '%z bytes' cl-gambol-lab
file cl-gambol-lab && otool -L cl-gambol-lab
for i in 1 2 3 4 5; do /usr/bin/time -p ./cl-gambol-lab; done
/usr/bin/time -l ./cl-gambol-lab
```

## Probe output

Deterministic probe records are reproduced below. Repeated ASDF version warnings and the expected SBCL stack-guard warning from the occurs-check child are omitted.

```text
PROBE library=cl-gambol version=0.03/d4d53a1e29a360f8aaab9134da89b8c6966fe16e
UNIFY ((?X . A) (?Y . B))
OCCURS occurs-check=absent result=unify-succeeds-cyclically-reification-stack-overflows
PATH answers=(A B C D) count=100 capped=T
UPDATE after-retract=(B C) after-reassert=(B B C)
DUPES (B B C)
ORDER (Z A)
FAIR capped=T done-reached=NIL answers=(A) count=20
APPEND-LHS ((A B))
APPEND-RHS (?YS)
NEG (T) NIL
FIXPOINT-ADAPTER from-a=(A B C D)
BINARY 40179640
```

Notes on the raw lines:

- UNIFY: nested compound unification `f(?x, g(?y)) = f(a, g(b))` binds `?x=A`, `?y=B` (single answer).
- PATH: 100 answers to `(path a ?x)` over the cyclic `edge` set cover all four nodes; the answer stream is infinite (each node recurs forever via the cycle `a→b→c→a`). The child process and answer cap stop enumeration.
- UPDATE: `(B C)` after retracting `dedge(c,b)` is correct semantics: B via `dedge(a,b)`, C via `dedge(a,c)`. Re-asserting restores the duplicate proof `(B B C)`. Retraction of facts works; the library documents that rules cannot be retracted.
- ORDER: facts are inserted as Z then A, and the unsorted answer stream is `(Z A)`.
- FAIR: an infinite first alternative emits 20 A answers and prevents the later DONE fact from running, establishing starvation under depth-first search.
- APPEND-RHS: the backward direction `app((a b), ?ys, ?zs)` returned an unbound `?YS` (append ran in "first argument fixed" mode without constraining `?ys`); the LHS direction `app(?xs, (c d), (a b c d))` correctly returned `?xs=(a b)`.
- OCCURS section: `X = f(X)` unification succeeds by binding `?X` to a cyclic molecule; reification (`expand-logical-vars`) then overflows the control stack (`SB-KERNEL::CONTROL-STACK-EXHAUSTED`). The probe catches that condition in a child process. The in-process `sb-ext:with-timeout` cannot fire here because the stack overflow lands before the timer interrupt is delivered.

## Binary measurements (standalone image)

| Metric | Value |
| --- | --- |
| Image | `cl-gambol-lab`, Mach-O 64-bit executable arm64, built via `sb-ext:save-lisp-and-die :executable t` |
| Executable bytes | 40,179,640 |
| Dynamic deps (`otool -L`) | `/usr/lib/libSystem.B.dylib`, `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` (Homebrew zstd, non-system) |
| Startup samples (wall) | 0.39s (first/cold), then 0.01s, 0.01s, 0.01s, 0.01s |
| Peak RSS | 44,646,400 bytes (`maximum resident set size`); a second run reported footprint 44,368,960 |
| Smoke test | prints `(((?X . B)))` for `(path a ?x)`, exit 0 |
| Source loading in image | `compile`/`load` of new source remains available (full SBCL image saved, no core compression) |

## Capability classification

| Capability | Result | Detail |
| --- | --- | --- |
| nested term unification | native | car/cdr recursion over Lisp conses, `equalp` on atoms (prolog.lisp:696) |
| occurs check | native | observed policy omits the occurs check: `pl-bind` binds unconditionally; `X = f(X)` unifies into a cyclic structure and reification stack-overflows |
| multiple answers | native | `pl-solve-all` / continuation-based `pl-solve-next`; depth-first, rule insertion order (ORDER raw line) |
| fair search | absent-from-probe | pure DFS with continuations; FAIR demonstrates that an infinite first alternative starves a later fact |
| cyclic transitive closure | absent-from-probe | no termination mechanism; DFS enumerates the infinite proof stream; requires an answer cap or external fixpoint |
| Datalog fixpoint | adapter | bottom-up closure over a hardcoded copy of the Gambol fixture reaches `{a,b,c,d}` from `a`; reading facts into the adapter remains required |
| tabling | absent-from-probe | no call/answer tables, no subsumption, no SCC completion |
| constraints | absent-from-probe | only `(lop ...)` Lisp-predicate guards; no constraint store |
| dynamic facts and retraction | native | `pl-assert`/`pl-asserta`/`pl-retract`; retract removes first matching fact, rules cannot be retracted; verified clean update cycle |
| standalone image | built | SBCL saved image, 40,179,640 bytes, one non-system dylib (zstd) |

## Report questions

1. **SWI coverage directly:** first-order unification, depth-first multiple answers, cut, assert/retract of facts, and Lisp-bridge predicates (`lop`/`lisp`/`is`).
2. **Adapters needed:** Datalog fixpoint (bottom-up loop in Lisp), termination bounds for recursive queries (answer caps or visited-state), deduplication of repeated proofs (`DUPES (B B C)` shows proof-path duplication), occurs-check guard.
3. **Cyclic recursion:** does not terminate; mechanism = depth-first continuation search with no tabling and no memoization (prolog.lisp:446-616).
4. **SBCL image:** yes, built successfully with `save-lisp-and-die`.
5. **Measurements:** 40,179,640 bytes; deps `libSystem.B.dylib` + `libzstd.1.dylib`; warm startup 0.01s (cold first sample 0.39s); peak RSS ~44.6 MB.
6. **Implementing files:** unification = `prolog.lisp` `unify/unify1/unify2/pl-bind` (684-713, 205); variables/environments = `calcify`, `make-goals`, molecule macros (128-183, 775-796); search = `pl-search`, `search-rules`, `match-rule-head`, `continue-on`, `do-cut` (446-634); rule store/caching = `*prolog-rules*` hash, `add-rule-to-index`, `get-rule-molecules` (348-415, 762); dynamic facts = `pl-assert*`/`pl-retract` (807-915).
7. **Before DL7 compiler rules could run:** add occurs check (or term-graph representation), a termination mechanism (tabling or visited-state adapter), proof deduplication at the adapter boundary, incremental answer invalidation after retraction (current tables are not a concept here; every query re-searches), and package-explicit term construction so variable identity survives the phase-0 boundary. Rule retraction is unsupported by the library itself, so rule-set updates need a rebuild-the-rulebase adapter (`with-rulebase` + `clear-rules`).
