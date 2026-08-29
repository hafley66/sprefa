# Single-Binary Shapes

## Locally measured shapes

| Shape | Process boundary | Artifact count | Runtime closure |
| --- | --- | ---: | --- |
| SBCL minimal | none | 1 Mach-O | image plus dynamic zstd |
| SBCL handwritten kernel | none | 1 Mach-O | image plus dynamic zstd |
| SBCL Paiprolog | none | 1 Mach-O | image plus dynamic zstd; library source is saved into the image |
| SWI minimal saved state | none | 1 Mach-O | saved state plus dynamic `libswipl.10.dylib` |
| SBCL to SWI subprocess | `SBCL image -> swipl` | 2 executable processes | SBCL image plus zstd and an external SWI installation |

The measured bytes, dependencies, startup samples, RSS, and exact commands
are in `2_MEASUREMENTS.md`.

## In-process Common Lisp to SWI shape

Local inspection found `libswipl.10.0.2.dylib` at:

```text
/opt/homebrew/opt/swi-prolog/lib/swipl/lib/arm64-darwin/libswipl.10.0.2.dylib
```

The installed `cl-prolog2.swi` route was inspected in lab 11. It prints
Prolog source into a temporary file and launches `swipl`; `rg` over its
`src` and `swi` directories found no `CFFI`, `libswipl`, foreign-frame, or
query-lifecycle route. No other local Common Lisp library supplying an
in-process SWI binding was found. The requested CL executable loading or
linking `libswipl` is therefore unavailable for this lab boundary.

The C embedding route in the SWI documentation begins with `PL_initialise()`;
each active foreign caller needs the documented engine, term-reference,
foreign-frame, query, exception, and cleanup lifecycle. A CL adapter would
need an existing CFFI binding or an implementation of that surface. This lab
does not add one.

## ECL packaging research

ECL's system-building manual documents `c:build-*` targets for an executable,
a shared library, and a static library. For ASDF systems it documents
`asdf:make-build`. A static or shared ECL library requires its generated
module initializer to be called by the C host. ECL was absent from `PATH` on
this machine, so these shapes are documented rather than locally measured.

| Shape | Build boundary | Required runtime ownership |
| --- | --- | --- |
| ECL executable | ASDF system to ECL executable | ECL-selected runtime artifacts and target dynamic libraries |
| ECL shared library | ASDF system to `.dylib` | host calls ECL initialization and generated module initializer |
| ECL static library | ASDF system to archive | native linker includes archive; host calls ECL initialization and generated module initializer |

Sources: [ECL system building](https://ecl.common-lisp.dev/static/manual/System-building.html)
and [ECL embedding](https://ecl.common-lisp.dev/static/manual/Embedding-ECL.html).

## Native C or Rust owner for both runtimes

```text
native executable
  -> ECL runtime and generated module
  -> libswipl + saved Prolog state
  -> native compiler components
```

The SWI C interface documents `PL_initialise()` before subsequent SWI calls.
The host owns the SWI argument vector for the engine lifetime; saved-state
selection and Prolog-home resolution depend on it. SWI also documents
`PL_set_resource_db_mem()` before initialization for a resource database held
in host memory. The native host must therefore define, per compiler request:

1. ECL initialization and generated-module initialization.
2. SWI initialization, one attached engine per calling thread, foreign-frame
   lifetime, query close, exception conversion, and cleanup.
3. ownership conversion for CL objects, Prolog terms, and native buffers.
4. saved-state and dynamic-library deployment paths.

Sources: [SWI embedding](https://www.swi-prolog.org/pldoc/man?section=embedded),
[PL_initialise](https://www.swi-prolog.org/pldoc/man?CAPI=PL_initialise), and
[SWI linking](https://swish.swi-prolog.org/pldoc/man?section=plld).

No C or Rust combined-runtime host was built in this lab. Its executable,
distribution, startup, RSS, and call-cost cells are unavailable.
