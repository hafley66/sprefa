# Sources

## Local inputs

| Input | Version or pin | Role | Local receipt |
| --- | --- | --- | --- |
| SBCL | 2.6.7, Homebrew arm64 | saved-image builder and runtime | `sbcl --version` |
| SWI-Prolog | 10.0.2, arm64 Darwin | saved-state builder and subprocess | `swipl --version` |
| `libswipl` | 10.0.2, 1,406,048 bytes | direct dynamic dependency of the SWI saved state | `swipl --dump-runtime-variables`; `wc -c` |
| Paiprolog | `012d6bb255d8af7f1c8b1d061dcd8a474fb3b57a` | retained library-backed SBCL image from lab 3 | `v7/labs/3_paiprolog/1_SOURCE.md` |
| handwritten kernel | local lab 12 source | retained SBCL image | `v7/labs/12_handwritten_logic/1_SOURCE.md` |
| cl-prolog2 | `21531c553208e01c0b0b205ea005afaefa7057e3` | inspected existing CL-to-SWI transport | `v7/labs/11_cl_prolog2/{1_SOURCE.md,4_RESULTS.md}` |

The new sources are `4_CL_IMAGE_MAIN.lisp` and `5_MINIMAL_SWI.pl`. `6_BUILD.lisp`
only writes a requested SBCL image to `BINARY_PACKAGING_OUT` under
`/private/tmp/`.

## Commands

```sh
artifact_dir=$(mktemp -d /private/tmp/sprefa-v7-binary-packaging.XXXXXX)

BINARY_PACKAGING_SHAPE=minimal-sbcl \
BINARY_PACKAGING_OUT="$artifact_dir/minimal-sbcl" \
  sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 6_BUILD.lisp

BINARY_PACKAGING_SHAPE=sbcl-swi-subprocess \
BINARY_PACKAGING_OUT="$artifact_dir/sbcl-swi-subprocess" \
  sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 6_BUILD.lisp

swipl -q -f none --no-packs -s 5_MINIMAL_SWI.pl \
  -g "qsave_program('$artifact_dir/swi-saved',[stand_alone(true),goal(main),toplevel(halt),autoload(true),op(save)])" \
  -g halt
```

The SWI build deliberately omits `foreign(save)`. Repository failure record
`docs/failure-modes.md:2516` records that option stripping Homebrew-installed
foreign objects in place on this host.

## Documentation

- [SBCL `save-lisp-and-die`](https://www.sbcl.org/manual/#Function-sb_002dext_003asave_002dlisp_002dand_002ddie)
- [SWI-Prolog saved states](https://www.swi-prolog.org/pldoc/man?section=saved-states)
- [SWI-Prolog embedding](https://www.swi-prolog.org/pldoc/man?section=embedded)
- [ECL system building](https://ecl.common-lisp.dev/static/manual/System-building.html)
- [ECL embedding](https://ecl.common-lisp.dev/static/manual/Embedding-ECL.html)
