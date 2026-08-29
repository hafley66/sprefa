# cl-kanren Source Record

## Upstream pointer resolution (required by brief)

The brief names `https://github.com/copperteal/cl-kanren`. The lab inventory
(`v7/labs/1_inventory/1_SOURCES.md`) names `https://codeberg.org/cage/cl-kanren`
as the canonical cl-kanren, matching Quicklisp `source.txt` for the
`cl-kanren` release. Both were cloned and compared:

| Field | copperteal/cl-kanren (GitHub) | cage/cl-kanren (Codeberg) |
| --- | --- | --- |
| License | GPL-3.0 | BSD (COPYING, (c) 2016 cage) |
| History | 5 commits, initial 2025-09-07 | 22 commits, 2014-03 to 2023-06 |
| Architecture | self-described TRS2-lexicon miniKanren reimplementation; substitution as binary tree; `SUBS-WEAVE`/`MAPSUBS` fairness; no binary arithmetic | miniKanren layer over a microKanren core (`mu-kanren.lisp`); assoc-list substitutions; occurs check |
| Quicklisp | absent from dist | release `cl-kanren` 2024-10-12, systems `cl-kanren`, `cl-kanren-test` |
| Relationship | same project name only; different author, license, and codebase; no shared git history | canonical upstream in the inventory and Quicklisp |

They are unrelated projects that share the name. Selection: **cage/cl-kanren
(Codeberg)**, because the inventory maps lab `8_cl_kanren` to it and it is the
Quicklisp-distributed `cl-kanren` (dist 2026-01-01). The copperteal pointer is
recorded here as a distinct, newer, GPL-3.0 same-named project; it was not
silently substituted for the selected upstream.

| Field | Value |
| --- | --- |
| Upstream | https://codeberg.org/cage/cl-kanren |
| Commit | `ad40ba1abb909f84f56ec503d225d1968ee82912` (2023-06-20, HEAD, clean tree) |
| Version | 0.1.0 (`cl-kanren.asd`) |
| License | BSD (COPYING) |
| Dependency | `alexandria` (Quicklisp dist 2026-01-01) |
| SBCL | 2.6.7 Homebrew arm64 |

## Install commands (executed)

```sh
git clone https://codeberg.org/cage/cl-kanren.git /private/tmp/sprefa-lab8-cache/cl-kanren-cage
cd /private/tmp/sprefa-lab8-cache
curl -fsSL -o quicklisp.lisp https://beta.quicklisp.org/quicklisp.lisp
sbcl --no-sysinit --no-userinit --load quicklisp.lisp \
  --eval '(quicklisp-quickstart:install :path #P"/private/tmp/sprefa-lab8-cache/.quicklisp/")' --quit
sbcl --no-sysinit --no-userinit --load .quicklisp/setup.lisp \
  --eval '(ql:quickload :alexandria :silent t)' --quit
```

Local Quicklisp lives at `/private/tmp/sprefa-lab8-cache/.quicklisp/` (external
to Git). Clone lives at `/private/tmp/sprefa-lab8-cache/cl-kanren-cage` (external to
Git). `--no-sysinit --no-userinit` is required because `~/.sbclrc` preloads a
different lab's Quicklisp; the lab never touches that global setup. The 2021
Quicklisp client lacks an https scheme handler; the default dist URLs are
plain `http` and no adapter was needed for loading.

## Source inventory at commit ad40ba1

| File | Lines | Content |
| --- | --- | --- |
| `packages.lisp` | 177 | packages `mu-kanren`, `interface`, `mu-kanren-goodies`, `mini-kanren`, `cl-kanren` |
| `mu-kanren.lisp` | 171 | microKanren core: `mu-var` class, `walk`, `occurs-check`+`extend-subst`, generic `unify-impl`, `==`, `mplus`, `bind`, `disj`, `conj` |
| `interface.lisp` | 129 | `equivp`/`unify-impl`/`walk-impl`/`reify-subst-impl` methods (lists, vectors, strings), `walk*`, `reify-subst`, `reify-name` |
| `mu-kanren-goodies.lisp` | 140 | `conde`, `fresh`, `run`/`run*`, `ifte`, `once`, `zzz`, `take`/`take-all`, `+succeed+`/`+fail+` |
| `mini-kanren.lisp` | 282 | `project`, `conda`/`condu`/`condi`, `all`/`alli`, list/tree relations (`appendo`, `membero`, `listo`, `flatteno`), relation constructors |
| `tests/` | n/a | clunit test suite (system `cl-kanren-test`) |

Total authored implementation: 899 lines across 5 files.
