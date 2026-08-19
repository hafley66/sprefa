# v6/prolog

The `.dl6` compiler and the oracle. This page covers the compiler's shipped
executable only; the language itself is `compile/SYNTAX.md`.

| section | what it answers |
|---|---|
| [dl6c](#dl6c) | install, usage, exit codes |
| [The other two doors](#the-other-two-doors) | compiling without installing |

## dl6c

One executable, built by `qsave_program/2` with `stand_alone(true)`. It carries
the whole compiler, so it needs no `v6/prolog` checkout at run time.

```bash
just build-dl6c     # v6/prolog/target/dl6c, HEAD's short sha stamped in
just install-dl6c   # rm then cp into ~/.cargo/bin/dl6c
```

```bash
dl6c <in.dl6> --target rust|ts --out <dir>
dl6c --version      # dl6c <sha>
```

`--out` is a directory; the emitted module takes the input's base name and the
target's extension (`--target rust` writes `<dir>/<in>.rs`).

The executable carries the compiler but not SWI's foreign libraries: it still
loads `uri.so`, `json.so`, `crypto4pl.so`, `pcre4pl.so` and `files.so` from an
installed SWI-Prolog 10 (`qsave_program`'s `foreign(save)` is a defect on
macOS arm64, `docs/failure-modes.md` row 56).

| exit | meaning |
|---|---|
| 0 | compiled |
| 2 | a named unsupported construct, named on stderr |
| 1 | anything else, including a usage error |

Same three codes `bop check` uses (`compile/scripts/bop_check.pl`).

## The other two doors

`compile/scripts/compile_dl6.sh <in.dl6> <out.ts>` and the `swipl -l compile.pl
-l emit_rust.pl -g "compile_dl6(...)"` line at `../sprefa-engine-rs/grade.sh:36`
call the same `compile_dl6/3`. `compile/scripts/dl6c_roundtrip.sh` diffs their
bytes against `dl6c`'s.
