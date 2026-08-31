# Local Measurements

Machine: arm64 macOS 14.6.1. Measurements were run 2026-08-29 with SBCL
2.6.7 and SWI-Prolog 10.0.2. Each startup sample is an independent process.
`/usr/bin/time -p` supplied wall samples. `/usr/bin/time -lp` supplied the
single peak-RSS sample after the sandboxed form was denied `kern.clockrate`
access.

`direct distribution bytes` includes the executable plus each non-system
dynamic library named by `otool -L`: Homebrew `libzstd.1.dylib` is 670,240
bytes and `libswipl.10.0.2.dylib` is 1,406,048 bytes. `/usr/lib/libSystem.B.dylib`
is OS-supplied and excluded.

| Shape | Executable bytes | Direct distribution bytes | Startup wall seconds, five samples | Peak RSS bytes |
| --- | ---: | ---: | --- | ---: |
| minimal SBCL image | 38,606,536 | 39,276,776 | 0.03, 0.01, 0.01, 0.01, 0.01 | 43,630,592 |
| handwritten kernel SBCL image | 42,080,472 | 42,750,712 | 0.02, 0.01, 0.01, 0.01, 0.01 | 46,923,776 |
| Paiprolog SBCL image | 40,769,552 | 41,439,792 | 0.13, 0.13, 0.13, 0.13, 0.13 | 53,755,904 |
| minimal SWI saved state | 269,693 | 1,675,741 | 0.02, 0.01, 0.01, 0.01, 0.01 | 10,977,280 |
| SBCL image invoking SWI subprocess | 38,606,536 | 39,276,776 plus external SWI installation | 0.11, 0.08, 0.08, 0.08, 0.08 | 61,947,904 |

The subprocess row has no measured distributable closure. Its executable has
only the SBCL and zstd Mach-O closure above, then requires `swipl` on `PATH`.
The installed SWI-Prolog 10.0.2 Cellar directory measures 31,820 KiB
(32,583,680 bytes as 1 KiB blocks), which is an installed-tree count rather
than a dependency-minimized distribution count.

## Artifact receipts

| Shape | External artifact | SHA-256 | `file` |
| --- | --- | --- | --- |
| minimal SBCL | `/private/tmp/sprefa-v7-binary-packaging.UPeGex/minimal-sbcl` | `db96737c7cb9fc274bae658117ff8e22bcb0ff39c9630d38d772a4fb1f624d78` | Mach-O 64-bit executable arm64 |
| SBCL subprocess | `/private/tmp/sprefa-v7-binary-packaging.UPeGex/sbcl-swi-subprocess` | `ee2b3c9a26dc6ee7d0b2eba7c44c5c059e58db0b444735c6080c252c4057ccb8` | Mach-O 64-bit executable arm64 |
| SWI saved state | `/private/tmp/sprefa-v7-binary-packaging.UPeGex/swi-saved` | `ae946327a3fcc6bbbc35101aa5fd2ff09da1f2696cbaf6db95138dafca0bc5b2` | Mach-O 64-bit executable arm64 |
| handwritten kernel | `/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic` | `be4fc038ee5f3af2e476684d491b717401ed6d550fff69e54acfe923b23c661c` | Mach-O 64-bit executable arm64 |
| Paiprolog | `/private/tmp/sprefa-v7-paiprolog-lab-012d6bb-20260828` | `3b60739f1ca822c7f97ded738c3f2943e7be33b935e55a91f64ae74e2f0525ab` | Mach-O 64-bit executable arm64 |

`otool -L` receipts:

```text
minimal-sbcl, sbcl-swi-subprocess, handwritten-kernel, paiprolog:
  /usr/lib/libSystem.B.dylib
  /opt/homebrew/opt/zstd/lib/libzstd.1.dylib

swi-saved:
  @rpath/libswipl.10.dylib
  /usr/lib/libSystem.B.dylib
```

The SWI saved state has one `LC_RPATH`:

```text
/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/lib/arm64-darwin
```

The bounded probes printed `SBCL-MINIMAL`, `SWI-SAVED PATH [a,b,c,d]`, and
`QUERY [a,b,c,d]`. The Paiprolog and handwritten rows re-ran their retained
lab probes. Paiprolog printed its existing compile notes and completed with
`BINARY 40769552`; the handwritten probe completed with `BINARY ... bytes=42080472`.

## Cross-runtime benchmark

The SBCL subprocess image dynamically asserts the cyclic path fixture in a
fresh SWI child, tables `path/2`, bounds `setof/3` with
`call_with_time_limit(1, ...)`, and requires `QUERY [a,b,c,d]`.

```sh
/usr/bin/time -p sh -c \
  'for run in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
     /private/tmp/sprefa-v7-binary-packaging.UPeGex/sbcl-swi-subprocess >/dev/null
   done'
```

Result: 1.90 seconds for 20 successful queries, 95 milliseconds per query.
The timed boundary includes SBCL image startup, one `swipl` child startup,
dynamic clause assertion, tabled closure, answer serialization, and process
exit.

## Exact measurement commands

```sh
wc -c <artifact>
shasum -a 256 <artifact>
file <artifact>
otool -L <artifact>
for run in 1 2 3 4 5; do /usr/bin/time -p <artifact> >/dev/null; done
/usr/bin/time -lp <artifact> >/dev/null
```

For the handwritten retained image, prefix the command with
`HANDWRITTEN_OUT=/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic`.
For Paiprolog, prefix it with
`PAIPROLOG_LAB_BINARY=/private/tmp/sprefa-v7-paiprolog-lab-012d6bb-20260828`.
