# Lab 16 Source: logadat

| Item | Value |
| --- | --- |
| Upstream | https://github.com/taarotman/logadat |
| Pinned commit | `23fc43cc918e0aaac2aace1410e7283ef675153a` ("now logadat feels somewhat like an actual language", 2025-12-20 22:18:25 +0700) |
| Cleanliness | clean `git status` at the pin; `git log -1 --format='%H %ad %s'` receipt in `4_RESULTS.md` |
| License | MIT (`LICENSE`, "Copyright (c) 2025 taarotman") |
| Packaging | single file `logadat.lisp`, 482 lines; no ASDF system, no Quicklisp dist entry (absent from Quicklisp as of 2026-08-28), no dependencies |
| Install route | one route: `git clone` then `(load "logadat.lisp")`. The file has no `in-package` form; the probe loads it into a lab-owned package so every library symbol is package-local and no user package is mutated |
| Checkout location | `/private/tmp/sprefa-v7-lab16/logadat` (temporary; outside the repository) |
| Other repo files | `README.org` (3 lines, title only), `writeup.org` (12 lines) |
| Runtime | SBCL 2.6.7 (Homebrew arm64), macOS Darwin arm64 |

## Implementation inventory (logadat.lisp line references)

| Concern | Implementation | Lines |
| --- | --- | --- |
| Fact storage | `collect-facts`: EDB is a hash table keyed on predicate name; value is the list of ground tuples; `validate-fact` enforces equal tuple arity per predicate | 203-223 |
| Rule representation | `collect-preds`: IDB is a hash table of `predicate` CLOS instances (`pred-name`, `term-length`, `rule-list`, `rules-rewrite`, `current-value`); rule = head + body datoms; head arity must match across clauses | 161-201 |
| Variable handling | no logic variables. "Lambda variables" are unbound symbols starting with a letter or `_` (`lambdavar-p`); binding is done by zipped comprehension (`inzip`) over destructuring lambda lists; constant positions become `equal` constraints via `pm-quals` | 49-54, 130-152, 66-74 |
| Rule evaluation | rule body becomes a list comprehension: `rewrite-atoms` rewrites `(in vars pred)` to `(in vars '<value-list>)` (EDB facts or IDB current value); `rule-compr-gen` + `to-inzip` generate a `compr-pm` form; `rule-to-compr` round-trips it through `write-to-string`/`read-from-string`/`eval` (comment: "hacky solution because eval mutates the previous declared quotes") | 226-284, 268-273 |
| Recursion / fixpoint | `naive-evaluation`: rewrite every rule against current values, evaluate all rules, compare old/new predicate value sets with `predicate=` (`set-exclusive-or` with `:test #'equal`, erroring on difference), recurse until no set changes. Bottom-up naive fixpoint; termination comes from set monotonicity over the finite Herbrand base of the ground tuples, with `remove-duplicates` and `lunion` keeping values finite | 255-316, 293-301 |
| Deduplication | `eval-rules` reduces per-rule rows with `lunion` (union under `equal`) and `remove-duplicates :test #'equal` | 287-290, 81-83 |
| Updates / retraction | no assert/retract API. Programs are rebuilt wholesale: new `(facts ...)`/`(rules ...)` forms, then `naive-evaluation` reruns from scratch. Retraction = rebuild the EDB without the tuple + full re-evaluation; no incremental maintenance | 203-223, 312-316 |
| Queries | `query-eval`/`queries-eval`/`queries` macros run `compr-pm` patterns against the evaluated predicate value lists or EDB | 325-353 |
| Program entry | `logadat` macro collects `:facts`/`:rule`/`:query`/`:eval` forms (`collect-body`) and expands to `queries (naive-evaluation ...) ...` | 361-378 |
| Evaluation strategy | naive evaluation only; a `seminaive-evaluation` exists but is commented out (318-322) |

## Loading note

`rule-to-compr` reads generated comprehension forms with the current
`*package*`, so every use of the library (including the probe's direct calls
into `naive-evaluation` internals) runs with the package that loaded
`logadat.lisp`. The probe honors this by binding `*package*` (`in-vendor`
macro in `2_PROBE.lisp`).
