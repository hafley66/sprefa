# sī-Kanren source and install receipt

## Upstream

| Field | Value |
| --- | --- |
| Library | [rgc69/si-kanren](https://github.com/rgc69/si-kanren) |
| Upstream HEAD checked | `93f051fcc2b46649d214eab951cdd4ed1de869da` |
| Commit date | `2025-12-24T12:03:02+01:00` |
| Commit subject | `Wrap playground demo queries with #+nil blocks` |
| ASDF version | `0.1.0` |
| License | MIT, [`LICENSE`](https://github.com/rgc69/si-kanren/blob/93f051fcc2b46649d214eab951cdd4ed1de869da/LICENSE) |
| Quicklisp dist | `2026-01-01` |
| Quicklisp release | `si-kanren-20260101-git` |

The supplied brief named an upstream update dated 2025-12-30. Remote `HEAD` at probe time resolved to the commit above, dated 2025-12-24. The Quicklisp 2026-01-01 archive was loaded for the executable probe. The separate upstream checkout provides the pinned source trace.

## Isolated install

All downloaded source, Quicklisp state, compiled FASLs, and executable artifacts are below:

```text
/private/tmp/sprefa-si-kanren.B7fXyx/
```

Commands:

```sh
mktemp -d /private/tmp/sprefa-si-kanren.XXXXXX
git clone https://github.com/rgc69/si-kanren.git /private/tmp/sprefa-si-kanren.B7fXyx/si-kanren
curl -fL https://beta.quicklisp.org/quicklisp.lisp \
  -o /private/tmp/sprefa-si-kanren.B7fXyx/quicklisp.lisp
cd /private/tmp/sprefa-si-kanren.B7fXyx
sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
  --load quicklisp.lisp \
  --eval '(quicklisp-quickstart:install :path #P"/private/tmp/sprefa-si-kanren.B7fXyx/quicklisp/")' \
  --eval '(quit)'
sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
  --load /private/tmp/sprefa-si-kanren.B7fXyx/quicklisp/setup.lisp \
  --eval '(ql:quickload "si-kanren")' \
  --eval '(quit)'
```

The package exports `run`, `run*`, `runi`, `fresh`, `conde`, `==`, `=/=`, `symbolo`, `numbero`, and `absento`. The probe calls those exports only. Its fixture relations are ordinary Common Lisp functions composed from those public forms.

## Source traces

| Concern | File and lines | Entry points |
| --- | --- | --- |
| Logic variables and substitutions | `src/si-kanren.lisp:23-36` | `lvar`, `walk`, `ext-s`, `unit` |
| Occurs check and unification | `src/si-kanren.lisp:38-58` | `occurs?`, `unify` |
| Fresh-variable state transition | `src/si-kanren.lisp:60-66` | `call/fresh` |
| Equality plus constraint propagation | `src/si-kanren.lisp:69-101` | `==` |
| Stream search | `src/si-kanren.lisp:103-122` | `mplus`, `bind`, `disj`, `conj` |
| Disequality store | `src/si-kanren.lisp:130-187` | `disequality`, `=/=`, `normalize-d<s/t/a` |
| Type constraints | `src/si-kanren.lisp:192-330` | `typeo`, `symbolo`, `numbero` |
| Absento constraints | `src/si-kanren.lisp:334-474` | `a-add`, `reform-A`, `absento` |
| Constraint bridge | `src/si-kanren.lisp:475-553` | `check-a/t->disequality` |
| State layout and bounded extraction | `src/wrappers.lisp:3-45` | `make-st`, `empty-state`, `take` |
| Reification and constraint printing | `src/wrappers.lisp:47-123` | `reify-state/1st-var`, `mK-reify` |
| Public query and goal syntax | `src/wrappers.lisp:125-177` | `conde`, `fresh`, `run`, `run*` |
| Answer normalization | `src/wrappers.lisp:324-472` | `normalize-fresh`, `normalize`, `normalize-conde` |

The documented state shape is `cs = '(((s) . c) (d) (t) (a))`: substitution and counter, disequality store, type store, and absento store. `make-st` and the four accessors at `src/wrappers.lisp:3-24` implement that layout.
