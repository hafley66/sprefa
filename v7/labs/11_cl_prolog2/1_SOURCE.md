# cl-prolog2 source receipt

## Upstream pin

| field | value |
| --- | --- |
| repository | `https://github.com/cl-model-languages/cl-prolog2` |
| requested ref | `HEAD` |
| resolved commit | `21531c553208e01c0b0b205ea005afaefa7057e3` |
| commit date | `2021-11-21` |
| commit subject | `fix yap pathname` |
| library version | `0.1` from `cl-prolog2.asd` |
| license | MIT, `README.md` |

The pin was verified with:

```sh
git ls-remote https://github.com/cl-model-languages/cl-prolog2 HEAD
git -C /private/tmp/cl-prolog2-lab.uuvw4c/upstream rev-parse HEAD
git -C /private/tmp/cl-prolog2-lab.uuvw4c/upstream status --porcelain
git -C /private/tmp/cl-prolog2-lab.uuvw4c/upstream log -1 --format='%H%n%cs%n%s'
```

Both commit queries returned `21531c553208e01c0b0b205ea005afaefa7057e3` on
the recorded lab date; the status command printed no rows. The remote `HEAD`
lookup records how the mutable branch was resolved and is not a build-time
dependency. `3_BUILD.lisp` repeats the local commit and clean-checkout
validation before loading the ASDF systems.

## Backend coverage

`README.md` names SWI-Prolog, YAP, XSB, B-Prolog, and GNU Prolog.  The checkout contains ASDF backend systems `cl-prolog2.swi`, `cl-prolog2.yap`, `cl-prolog2.xsb`, `cl-prolog2.bprolog`, and `cl-prolog2.gprolog`.

This lab loads `cl-prolog2.swi` and invokes installed `swipl` 10.0.2.

## Dependency route

All dependency state is external to the checkout:

```sh
sbcl --noinform --disable-debugger --no-userinit --no-sysinit \
  --load /private/tmp/cl-prolog2-lab.uuvw4c/quicklisp.lisp \
  --eval '(setf quicklisp-quickstart::*home* #P"/private/tmp/cl-prolog2-lab.uuvw4c/quicklisp/")' \
  --eval '(quicklisp-quickstart:install)' \
  --eval '(quit)'

sbcl --noinform --disable-debugger --no-userinit --no-sysinit \
  --eval '(load #P"/private/tmp/cl-prolog2-lab.uuvw4c/quicklisp/setup.lisp")' \
  --eval '(ql:quickload (list "trivia" "alexandria" "trivia.quasiquote" "external-program" "trivial-garbage"))'
```

Quicklisp dist: `quicklisp` `2026-01-01`.  Direct systems declared by `cl-prolog2.asd`: `trivia`, `alexandria`, `trivia.quasiquote`, `external-program`, and `trivial-garbage`.  The transitive load installed `named-readtables` and `fare-quasiquote`, required by the upstream reader configuration.

No dependency checkout or Quicklisp state is stored in this lab directory.
