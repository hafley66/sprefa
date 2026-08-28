# cl-datalog Source Record

| Field | Value |
| --- | --- |
| Upstream | https://github.com/thephoeron/cl-datalog |
| Commit | `da2fb09a8c55cb9c4488358ee5dff4ab49ae473f` (2015-09-03, master HEAD) |
| Version | 0.0.1 (from `cl-datalog.asd`) |
| License | MIT (`LICENSE`, Copyright 2015 Colin J.E. Lupton) |
| SBCL | 2.6.7 Homebrew arm64 |

## Install commands (executed)

```sh
git clone https://github.com/thephoeron/cl-datalog /tmp/cl-datalog-upstream
cd .lab-cache
curl -fL -o quicklisp.lisp https://beta.quicklisp.org/quicklisp.lisp
sbcl --noinform --disable-debugger --load quicklisp.lisp \
  --eval '(quicklisp-quickstart:install :path #P"./.quicklisp/")' --quit
```

Local Quicklisp lives at `v7/labs/4_cl_datalog/.lab-cache/.quicklisp/` (untracked, outside Git). Clone lives at `/tmp/cl-datalog-upstream` (outside the repo). No global Quicklisp mutation.

Dependency identity: Quicklisp dist `2026-01-01`; `trivial-types` release
`trivial-types-20120407-git`, ASDF version `0.1`, archive MD5
`b14dbe0564dcea33d8f4e852a612d7db`, archive SHA1
`acf9e5a4b0ef99bdcb121cfbc8f07c647c302e57`.

## Source inventory at commit da2fb09

| File | Lines | Content |
| --- | --- | --- |
| `packages.lisp` | 8 | `(defpackage cl-datalog (:use :cl :cl-user :trivial-types))` — exports nothing |
| `cl-datalog.lisp` | 5 | `(in-package :cl-datalog)` and nothing else |
| `cl-datalog.asd` | 20 | defsystem, depends-on `:trivial-types` |
| `cl-datalog-test.asd` + `t/cl-datalog.lisp` | — | prove-based sanity test that exercises only `+`, `*`, `mod`; tests no Datalog behavior |
| `docs/index.md` | 1 | "CL-DATALOG / Datalog DSL for Common Lisp" |

The authored implementation surface consists only of package declarations and
`in-package` forms. There are no functions, macros, structs, terms, unifier,
rule store, or evaluator.
