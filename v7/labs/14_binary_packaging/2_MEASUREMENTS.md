# Measurements

Date: 2026-08-28. Host: Darwin arm64. All newly generated image, state, and generated Prolog source artifacts are under `/private/tmp/sprefa-lab14.v5dvQS`.

## Method

`/usr/bin/time -p` was run five times per locally measured executable with stdout discarded. `/usr/bin/time -l` was run once per shape for RSS. The first sandboxed `-l` attempt could not read `kern.clockrate`; the repeated RSS collection used permitted system measurement access. The subprocess benchmark is one bounded run of 20 SWI child-process queries from the saved SBCL image.

Distribution totals are byte sums of the executable/state plus its non-system runtime payload: `libzstd.1.dylib` is 670,240 bytes; the installed SWI home tree contains 1,503 regular files totaling 58,285,454 bytes and includes `libswipl`; `libgmp.10.dylib` is 452,816 bytes. System libraries and macOS frameworks are excluded.

These totals are payload sums, not receipts for a relocated distribution.
The measured SWI saved-state launcher selects its absolute Homebrew runtime
path; relocation would require the deployment controls documented by SWI.

| Shape | State | Executable or state bytes | Counted distribution bytes | Startup samples, seconds | RSS, bytes | External runtime requirements |
| --- | --- | ---: | ---: | --- | ---: | --- |
| minimal SBCL executable image | measured | 38,606,536 | 39,276,776 | `0.01, 0.01, 0.01, 0.01, 0.01` | 43,155,456 | `libzstd.1.dylib`; macOS `libSystem`. |
| handwritten logic-kernel SBCL executable | measured, reused | 42,080,472 | 42,750,712 | `0.01, 0.01, 0.01, 0.01, 0.01` | 46,661,632 | `libzstd.1.dylib`; macOS `libSystem`. |
| library-backed SBCL executable (`cl-prolog2`) | measured, reused | 45,554,408 | 104,962,918 | `0.11, 0.11, 0.11, 0.11, 0.11` | 52,658,176 | `libzstd.1.dylib`, SWI home tree, `libgmp.10.dylib`, macOS system libraries. The library starts `swipl`. |
| minimal SWI saved-state launcher | measured | 235,215 | 58,973,485 | `0.01, 0.01, 0.01, 0.01, 0.01` | 9,289,728 | Saved launcher selects `/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/bin/arm64-darwin/swipl`; SWI home tree, `libgmp.10.dylib`, macOS system libraries. |
| SBCL executable invoking SWI subprocess | measured | 38,672,080 | 98,080,590 | `0.07, 0.07, 0.07, 0.08, 0.07` | 61,046,784 | `libzstd.1.dylib`, SWI home tree, `libgmp.10.dylib`, macOS system libraries. |
| CL executable loading or linking `libswipl` through an existing library | documented-only | n/a | n/a | n/a | n/a | Local `libswipl.10.0.2.dylib` exists, but no inspected Common Lisp library supplies an in-process route. |

## Bounded cross-runtime benchmark

```text
BENCHMARK shape=sbcl-subprocess-swi count=20 seconds=0.849798 ms-per-query=42.490
```

The query is `once((member(X,[a,b,c,d]),X=d)),write_canonical(X),nl`. Each of the 20 calls starts `swipl`, reads one canonical `d` answer, and exits. The measurement excludes SBCL image construction and includes child-process launch, SWI initialization, query evaluation, stdout collection, and exit.

## Commands

```sh
env LAB14_OUT=/private/tmp/sprefa-lab14.v5dvQS LAB14_SHAPE=minimal \
  sbcl --noinform --disable-debugger --no-userinit --no-sysinit --script 3_BUILD.lisp
env LAB14_OUT=/private/tmp/sprefa-lab14.v5dvQS LAB14_SHAPE=subprocess \
  sbcl --noinform --disable-debugger --no-userinit --no-sysinit --script 3_BUILD.lisp
swipl -o /private/tmp/sprefa-lab14.v5dvQS/minimal-swi-state \
  -c v7/labs/14_binary_packaging/1c_MINIMAL_SWI.pl
env LAB14_BENCH=1 /private/tmp/sprefa-lab14.v5dvQS/sbcl-subprocess-swi
```

The reused artifacts were executed directly at their paths recorded below. `file`, `otool -L`, `wc -c`, and `shasum -a 256` were run against every executable/state artifact. A saved SWI state is a shell launcher containing binary state data, so `otool -L` applies to its selected SWI runtime executable rather than the launcher script.

## Artifact receipts

| Shape | Artifact and SHA-256 | `file` | `otool -L` receipt |
| --- | --- | --- | --- |
| minimal SBCL | `/private/tmp/sprefa-lab14.v5dvQS/minimal-sbcl` (38,606,536 bytes), `fa077068b6911e62ff55205ecd7ba98e6181c4242489539b8d598be19306253f` | Mach-O 64-bit executable arm64 | `/usr/lib/libSystem.B.dylib`; `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` |
| handwritten SBCL | `/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic` (42,080,472 bytes), `be4fc038ee5f3af2e476684d491b717401ed6d550fff69e54acfe923b23c661c` | Mach-O 64-bit executable arm64 | `/usr/lib/libSystem.B.dylib`; `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` |
| `cl-prolog2` SBCL | `/private/tmp/cl-prolog2-lab.uuvw4c/image/cl-prolog2-lab-11-r2` (45,554,408 bytes), `8d4af37c3a891b73ee969a81de8613917d9a058c24e05a46c93700eed2dac53e` | Mach-O 64-bit executable arm64 | `/usr/lib/libSystem.B.dylib`; `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` |
| SWI saved state | `/private/tmp/sprefa-lab14.v5dvQS/minimal-swi-state` (235,215 bytes), `aceedd41cdc22d6d530e3277e78e8c1c51e34d94ee3e118388f590d7d5fce16a` | POSIX shell script executable (binary data) | launcher is not a Mach-O object; selected runtime receipt follows |
| subprocess SBCL | `/private/tmp/sprefa-lab14.v5dvQS/sbcl-subprocess-swi` (38,672,080 bytes), `9c6c183c3856a37733ebf70827595910a896058bdd665867eddb296dc929fd31` | Mach-O 64-bit executable arm64 | `/usr/lib/libSystem.B.dylib`; `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` |
| `libswipl` documented input | `/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/lib/arm64-darwin/libswipl.10.0.2.dylib` (1,406,048 bytes), `11d190f723db01bb3cc755a4644e1e3b81352b4e74ffb1ebb41f873aa026f7c6` | Mach-O 64-bit dynamically linked shared library arm64 | self install-name under `/opt/homebrew/opt/swi-prolog`; `/usr/lib/libncurses.5.4.dylib`; `/usr/lib/libform.5.4.dylib`; `/opt/homebrew/opt/gmp/lib/libgmp.10.dylib`; `/usr/lib/libz.1.dylib`; `/usr/lib/libSystem.B.dylib`; CoreFoundation |

Selected runtime for the SWI saved state:

```text
path: /opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/bin/arm64-darwin/swipl
bytes: 33792
sha256: 84b8da9369c2e03a15e71df813a270507cfc3546c06527f7a416db7a586b1d74
file: Mach-O 64-bit executable arm64
otool -L:
  @rpath/libswipl.10.dylib
  /usr/lib/libSystem.B.dylib
```

Raw `otool -L` output for the measured Mach-O images:

```text
minimal-sbcl:
  /usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1345.120.2)
  /opt/homebrew/opt/zstd/lib/libzstd.1.dylib (compatibility version 1.0.0, current version 1.5.7)
12_handwritten_logic:
  /usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1345.120.2)
  /opt/homebrew/opt/zstd/lib/libzstd.1.dylib (compatibility version 1.0.0, current version 1.5.7)
cl-prolog2-lab-11-r2:
  /usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1345.120.2)
  /opt/homebrew/opt/zstd/lib/libzstd.1.dylib (compatibility version 1.0.0, current version 1.5.7)
sbcl-subprocess-swi:
  /usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1345.120.2)
  /opt/homebrew/opt/zstd/lib/libzstd.1.dylib (compatibility version 1.0.0, current version 1.5.7)
minimal-swi-state:
  is not an object file
SWI runtime executable:
  @rpath/libswipl.10.dylib (compatibility version 10.0.0, current version 10.0.2)
  /usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1345.120.2)
libswipl.10.0.2.dylib:
  /opt/homebrew/opt/swi-prolog/lib/swipl/lib/arm64-darwin/libswipl.10.dylib (compatibility version 10.0.0, current version 10.0.2)
  /usr/lib/libncurses.5.4.dylib (compatibility version 5.4.0, current version 5.4.0)
  /usr/lib/libform.5.4.dylib (compatibility version 5.4.0, current version 5.4.0)
  /opt/homebrew/opt/gmp/lib/libgmp.10.dylib (compatibility version 16.0.0, current version 16.0.0)
  /usr/lib/libz.1.dylib (compatibility version 1.0.0, current version 1.2.12)
  /usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1345.120.2)
  /System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation (compatibility version 150.0.0, current version 2503.1.0)
```

## Probe receipts

```text
PROBE shape=minimal-sbcl
PROBE shape=sbcl-subprocess-swi answer=d
d
```

The final `d` is the SWI saved-state launcher's output.
