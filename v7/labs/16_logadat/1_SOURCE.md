# Logadat Source Record

| Field | Value |
| --- | --- |
| Upstream | https://github.com/taarotman/logadat |
| Commit | `23fc43cc918e0aaac2aace1410e7283ef675153a` |
| Commit date | 2025-12-20T22:18:25+07:00 |
| Commit subject | `now logadat feels somewhat like an actual language` |
| Files at pin | `logadat.lisp` (482 lines), `LICENSE`, `README.org`, `writeup.org` |
| License | MIT, `LICENSE`, Copyright (c) 2025 taarotman |
| Dependencies | none declared; no ASDF system or Quicklisp dependency |
| Runtime | SBCL 2.6.7 on Darwin arm64 |

## Isolated install route

```sh
LAB_TMP=$(mktemp -d /private/tmp/sprefa-logadat.XXXXXX)
git clone --filter=blob:none https://github.com/taarotman/logadat "$LAB_TMP/upstream"
git -C "$LAB_TMP/upstream" checkout --detach 23fc43cc918e0aaac2aace1410e7283ef675153a
export LOGADAT_UPSTREAM="$LAB_TMP/upstream"
export LOGADAT_OUT="$LAB_TMP/logadat-lab"
```

The checkout, no-dependency Quicklisp state, executable image, and temporary
probe data stay below the fresh `/private/tmp/sprefa-logadat.*` root. No
Quicklisp installer runs because the pinned upstream has no declared
dependencies.

## Public loading boundary

`logadat.lisp` declares no package or ASDF system. `2_PROBE.lisp` loads it
into the fresh `SPREFA-LOGADAT-UPSTREAM` package, preserving its unmodified
macros and functions while avoiding `CL-USER` definitions. The public macro
used for the fixture is `LOGADAT`; `FACTS`, `RULES`, and `QUERIES` are the
source-level construction and query macros.

## Source trace at the pinned commit

| Facility | Source function or macro | Lines | Receipt |
| --- | --- | --- | --- |
| fact declaration and validation | `facts`, `collect-facts`, `validate-fact` | 203-223 | facts become an EDB hash table keyed by predicate symbol; each value is a list of same-arity tuples. |
| rule declaration | `rules`, `collect-preds`, class `predicate` | 161-201 | rules are held per predicate with head arity and current result slots. |
| rule rewrite | `rewrite-preds-rules`, `rewrite-atoms` | 226-252 | body `in` atoms are rewritten to the current IDB value or EDB tuples. |
| evaluation | `eval-preds`, `rule-to-compr`, `rule-compr-gen`, `eval-rules` | 255-291 | rules become list comprehensions; `eval-rules` applies `remove-duplicates` with `equal`. |
| recursion and termination | `naive-evaluation`, `predicate=`, `predicate=nilerr` | 293-316 | recursively evaluates a new predicate map until `set-exclusive-or` finds no changed relation rows. |
| query projection | `query-eval`, `get-rule-or-fact`, `queries-eval`, `queries` | 325-353 | query terms are pattern-matched against the completed IDB or EDB relation. |
| top-level DSL | `collect-body`, `logadat` | 361-378 | `:facts`, `:rule`, `:query`, and optional `:eval` forms expand into facts, rules, evaluation, and query forms. |

The file defines no `unify`, occurs-check, library-owned `assert`/`update`/
`retract`, or table-management facility. The only `seminaive-evaluation`
definition is commented out at lines 318-322.
