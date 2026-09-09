# Single-binary and host shapes

## Measured shapes

| Shape | Process boundary | Measurement state | Distribution boundary |
| --- | --- | --- |
| minimal SBCL image | one SBCL process | measured | SBCL runtime image plus `libzstd.1.dylib`. |
| handwritten kernel SBCL image | one SBCL process | measured from current lab 12 artifact | SBCL runtime image plus `libzstd.1.dylib`. |
| `cl-prolog2` SBCL image | SBCL parent and one `swipl` child | measured from current lab 11 artifact | SBCL image, `libzstd.1.dylib`, and the external SWI runtime tree. |
| SWI saved state | saved-state launcher and one SWI runtime process | measured | launcher state plus the external SWI runtime tree. |
| local SBCL subprocess adapter | SBCL parent and one `swipl` child | measured | SBCL image, `libzstd.1.dylib`, and the external SWI runtime tree. |
| CL image plus `libswipl` | one host process containing SBCL and SWI | documented-only | no existing local Common Lisp adapter supplied this route. |

The first two SBCL shapes emit no SWI calls. The `cl-prolog2` and local subprocess shapes preserve the runtime boundary as bytes written to a child process's command arguments and bytes read from stdout. The current `cl-prolog2` result records its generated Prolog source file and `uiop:run-program` launch; `otool -L` of the SBCL image contains no `libswipl` entry.

## ECL static and shared embedding

ECL documents this C-host sequence for a compiled Lisp library:

```text
compile Lisp files -> ECL object files
object files -> c:build-static-library or c:build-shared-library
C main -> cl_boot -> ecl_init_module(init-name) -> Lisp calls -> cl_shutdown
```

For static or shared output, `:init-name` supplies the C-visible module initializer. The [ECL system-building manual](https://ecl.common-lisp.dev/static/manual/System-building.html) gives `c:build-static-library`, `c:build-shared-library`, and `c:build-program`; its ASDF examples call `cl_boot`, `ecl_init_module`, and `cl_shutdown`. The [ECL embedding reference](https://ecl.common-lisp.dev/static/manual/Embedding-ECL.html) requires `cl_boot` before ECL object creation or evaluation. A native thread that will execute Lisp imports itself with `ecl_import_current_thread` and releases itself before exit. A moved ECL installation may need `ECLDIR`.

Local status: ECL is absent from `PATH`, so no ECL object, library, program, byte count, startup samples, or RSS receipt was generated.

## C or Rust host containing ECL and SWI

The documented dual-runtime process has a native owner that initializes ECL, initializes SWI, loads its ECL module and Prolog state, then routes requests through explicit C ABI calls:

```text
C or Rust main
  -> ECL cl_boot
  -> ECL ecl_init_module for the generated Lisp library
  -> SWI PL_initialise
  -> request thread: ECL thread import when it executes Lisp
  -> request thread: attached or selected SWI engine before term_t/query use
  -> close SWI queries and foreign frames; release ECL/SWI thread state
  -> SWI cleanup; ECL cl_shutdown
```

The [SWI foreign interface](https://www.swi-prolog.org/pldoc/man?section=foreign) documents C host embedding and calls from C into Prolog. The [SWI mixed-thread manual](https://www.swi-prolog.org/pldoc/man?section=foreignthread) requires an engine for every native thread that uses `term_t`; it documents both attached thread engines and independent pooled engines. For Rust, the [Rust FFI reference](https://doc.rust-lang.org/nomicon/ffi.html) defines the `unsafe extern "C"` declarations and static or dynamic native linkage boundary. The ECL static/shared module remains C ABI compatible with that host sequence.

No C or Rust host, ECL module, or new embedding library was implemented in this lab. These rows have no local byte, startup, RSS, hash, `file`, or `otool` measurement.

## `libswipl` Common Lisp route

The local SWI installation exposes a shared runtime:

```text
PLLIBSWIPL=/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/lib/arm64-darwin/libswipl.10.0.2.dylib
PLSHARED=yes
PLTHREADS=yes
```

The local `cl-prolog2` SWI backend is a subprocess adapter. No inspected in-process Common Lisp library supplied `PL_initialise`, term construction, query, and engine lifecycle bindings. Consequently the sixth requested shape is documented-only and no CFFI or other embedding layer was added.

## Stop boundary and blockers

The lab stopped within the requested ten-minute boundary.

| Item | State |
| --- | --- |
| ECL static/shared local measurement | blocked by no `ecl` executable, headers, or `libecl` under the installed Homebrew paths. |
| CL plus `libswipl` measurement | blocked by no existing inspected Common Lisp library with an in-process `libswipl` route. |
| C/Rust dual-runtime measurement | documented-only: no ECL installation, generated ECL module, or pre-existing dual-runtime host artifact was available. |
