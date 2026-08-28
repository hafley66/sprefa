# Lab 17 Results: si-kanren

## Environment

| Item | Value |
| --- | --- |
| Library | si-kanren, upstream commit `93f051fcc2b46649d214eab951cdd4ed1de869da` (2025-12-24) |
| Quicklisp | dist 20260101, system `si-kanren` 0.1.0; tarball tree-identical to the pinned commit (`diff -rq` over the checkout minus `.git` produced no differences) |
| License | MIT |
| Runtime | SBCL 2.6.7 (Homebrew arm64) |
| Host | macOS (Darwin, arm64) |
| Checkout | `/private/tmp/sprefa-v7-lab17/upstream` (git) and `/private/tmp/sprefa-v7-lab17/.quicklisp/dists/quicklisp/software/si-kanren-20260101-git` (load source); temporary, outside the repository |
| Dependency route | one route: lab-local Quicklisp installed with `--no-sysinit --no-userinit`; no global Quicklisp mutation |
| Fixture | shared cyclic graph: `edge(a,b) edge(b,c) edge(c,a) edge(c,d)`; transitive `path` |

## Commands

```sh
curl -fL -o /private/tmp/sprefa-v7-lab17/quicklisp.lisp https://beta.quicklisp.org/quicklisp.lisp
sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
  --load /private/tmp/sprefa-v7-lab17/quicklisp.lisp \
  --eval '(quicklisp-quickstart:install :path #P"/private/tmp/sprefa-v7-lab17/.quicklisp/")' --quit

git clone https://github.com/rgc69/si-kanren /private/tmp/sprefa-v7-lab17/upstream
# pin: 93f051fcc2b46649d214eab951cdd4ed1de869da == quicklisp si-kanren-20260101-git tree

QL_SETUP=/private/tmp/sprefa-v7-lab17/.quicklisp/setup.lisp \
SIK_COMMIT=93f051fcc2b46649d214eab951cdd4ed1de869da \
  sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 2_PROBE.lisp

QL_SETUP=/private/tmp/sprefa-v7-lab17/.quicklisp/setup.lisp \
SIK_COMMIT=93f051fcc2b46649d214eab951cdd4ed1de869da \
SIK_OUT=/private/tmp/sprefa-v7-lab17/si-kanren-lab-image \
  sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 3_BUILD.lisp

/private/tmp/sprefa-v7-lab17/si-kanren-lab-image
/usr/bin/time -p /private/tmp/sprefa-v7-lab17/si-kanren-lab-image   # x5
/usr/bin/time -lp /private/tmp/sprefa-v7-lab17/si-kanren-lab-image  # peak RSS
file /private/tmp/sprefa-v7-lab17/si-kanren-lab-image
shasum -a 256 /private/tmp/sprefa-v7-lab17/si-kanren-lab-image
otool -L /private/tmp/sprefa-v7-lab17/si-kanren-lab-image
```

## Probe output (source run)

```text
PROBE library=si-kanren version=93f051fcc2b46649d214eab951cdd4ed1de869da pkg=SI-KANREN
BUG ground==ground empty-s result=NIL
UNIFY qr=((A B))
OCCURS occurs-check=T policy=unconditional-failure result=NIL
ORDER-DUPES ((5) (5))
APPEND-LHS ((A B C D))
APPEND-RHS ((A B))
FAIR cap=1 answers=(DONE) bound=5s
PATH-FROM-A sorted-unique=((A A) (A B) (A C) (A D)) k-max=4
PATH-UNBOUNDED result=TIMEOUT bound=5s n=100000
DISEQ ((B))
NUMBERO ((5))
SYMOLO ((A))
ABSENTO ((A B))
ABSENTO-FORBIDDEN NIL
BINARY blocked:not-built
```

## Image run

```text
PROBE library=si-kanren version=93f051fcc2b46649d214eab951cdd4ed1de869da image=built
EDGE ((SI-KANREN-BUILD::A SI-KANREN-BUILD::B) (SI-KANREN-BUILD::B SI-KANREN-BUILD::C)
      (SI-KANREN-BUILD::C SI-KANREN-BUILD::A) (SI-KANREN-BUILD::C SI-KANREN-BUILD::D))
```

## Notes on each line

- BUG: `(== 'a 'a)` on the empty substitution yields no answer (result `NIL`).
  Root cause is the cond fall-through at si-kanren.lisp:57 (see
  `1_SOURCE.md`). All probe ground positions go through a `g==` adapter that
  first binds one dummy variable so the substitution is non-empty. This is an
  adapter around an upstream defect, not a library feature.
- UNIFY: nested `f(q g(r)) = f(a g(b))` binds `q=a r=b` in one answer; car/cdr
  cons recursion in `unify` (si-kanren.lisp:46-58).
- OCCURS: `X = (X)` fails. `occurs?` is called unconditionally in both lvar
  branches of `unify`; exact observed policy: on, non-togglable, silent
  failure (no error).
- ORDER-DUPES: `disj+` of two `q=5` clauses answers `(5 5)` in clause order;
  no dedup and no reordering.
- APPEND: both directions give the single expected answer.
- FAIR: `mplus` swaps streams at thunk boundaries (si-kanren.lisp:103-107);
  an infinite no-answer left branch (delayed with `zzz`) does not starve the
  right branch's `done` (cap=1 → `(DONE)`). Recursive goals must be wrapped in
  `zzz` manually; eager evaluation otherwise recurses without a thunk boundary
  (observed as a 5 s timeout before adding `zzz`).
- PATH-FROM-A: the library has no tabling; unbounded `patho` diverges
  (PATH-UNBOUNDED timed out at 5 s). Termination here comes from a
  depth-bounded adapter: `patho_k` enumerates k-step paths for k = 1..4
  (ground-depth bound at adapter construction time), yielding exactly the
  expected closure from `a`: `{a-b, a-c, a-d, a-a}` including the cycle
  a→b→c→a.
- DISEQ: `disj+ q=a/b` with `(=/= q 'a)` answers only `(B)`; the residual
  disequality store rejects the `a` branch (si-kanren.lisp:145-172).
- NUMBERO / SYMOLO: type-store constraints filter by `numberp` / `symbolp`
  via `typeo` (si-kanren.lisp:329-330).
- ABSENTO: `(absento c q)` admits `(a b)` and forbids `(a c)`; the restricted
  version (first argument must be a symbol) per the Joshi/Byrd tutorial
  reconstruction.
- BINARY: filled by the image below.

## Saved executable image measurements

| Metric | Value |
| --- | --- |
| Image | `/private/tmp/sprefa-v7-lab17/si-kanren-lab-image`, Mach-O 64-bit executable arm64, `sb-ext:save-lisp-and-die :executable t` |
| Executable bytes | 42,473,744 |
| SHA-256 | `aa8e10ab1651d1c01c43aea726ee8399e19ab426760a287aad93a19746654008` |
| Dynamic deps (`otool -L`) | `/usr/lib/libSystem.B.dylib`, `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` (Homebrew zstd, non-system) |
| Startup samples (wall, `/usr/bin/time -p`, 5 runs) | 0.01, 0.01, 0.01, 0.01, 0.01 s |
| Peak RSS (`/usr/bin/time -lp`) | 47,038,464 bytes maximum resident set size |
| Smoke test | image exits 0 and reproduces the four edge tuples with `image=built` |
| Source/compilation in image | available: full SBCL save including the Quicklisp-installed si-kanren source; Quicklisp setup path was baked into the loading environment |

## Capability classification

| Capability | Result | Detail |
| --- | --- | --- |
| nested term unification | native | cons-recursive `unify` with `walk` (si-kanren.lisp:46-58); compound terms with equal ground heads need the `g==` workaround due to the empty-substitution bug |
| occurs check | native | unconditional, silent failure; observed `X = (X)` fails |
| multiple answers | native | stream order follows `disj+`/`conde` clause order; duplicates preserved; `run*` prints via `mK-reify` |
| fair search | native | `mplus` interleaves at thunks; infinite no-answer branch does not starve a finite branch; recursive goals require manual `zzz` delays |
| cyclic transitive closure | adapter | no tabling; unbounded recursion diverges (5 s timeout receipt); depth-bounded `patho_k` adapter produced the full closure including the cycle |
| Datalog fixpoint | absent-from-probe | no bottom-up saturation layer |
| tabling | absent-from-probe | no call/answer tables, no subsumption |
| constraints | native | disequality `=/=`, `symbolo`, `numbero`, restricted `absento`; store shape `cs = '(((s) . c) (d) (t) (a))` |
| dynamic facts and retraction | adapter | facts compiled into `disj+`/`conde` relation closures; retraction = rebuild the relation; no incremental behavior |
| saved executable image | built | 42,473,744 bytes, deps libSystem + Homebrew libzstd, 0.01 s startup, 47.0 MB peak RSS |

## Report questions

1. **SWI coverage directly:** first-order unification with occurs check,
   multiple ordered answers, fair interleaved disjunction, relational append,
   disequality, symbol/number type constraints, and restricted absento.
2. **Adapters needed:** fact stores as closures (no assert/retract), depth
   bounds for recursive transitive closure, deduplication and set-valued
   queries at the boundary, a `g==` workaround for the empty-substitution
   ground-equality bug, and programmatic answer extraction (the library's
   `run*` prints instead of returning values).
3. **Cyclic recursion:** diverges without a bound (PATH-UNBOUNDED timeout);
   termination here came from the explicit k-step depth-bounded adapter. No
   tabling or visited-state machinery exists in the library.
4. **SBCL image:** yes; Quicklisp-installed system saves into a working
   executable.
5. **Measurements:** 42,473,744 bytes; deps `libSystem.B.dylib` +
   `libzstd.1.dylib`; five startup samples all 0.01 s; peak RSS 47,038,464
   bytes.
6. **Implementing files:** `src/si-kanren.lisp` — substitutions/stream
   primitives 23-45, occurs+unify 38-58, `==` with constraint pipeline 69-102,
   streams 103-113, conj/disj 116-128, disequality 145-172, type store
   192-331, absento 334-475, bridge check 476-510; `src/wrappers.lisp` — state
   accessors 3-24, pull/take 26-42, reification 47-115, `zzz`/`conde`/`fresh`
   125-147, run/run* 149-178, answer normalization 220-510.
7. **Before DL7 compiler rules could run:** fix or work around the
   empty-substitution unify bug at the engine level, add tabling or a
   visited-state adapter for cyclic recursive rules, add an incremental fact
   store with retraction, make reification return values instead of printing,
   and pin a package/export boundary (the library currently has none, so
   symbol capture depends on the loading package).
