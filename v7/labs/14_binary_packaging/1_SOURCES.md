# Sources and local inputs

Date: 2026-08-28. Host: Darwin arm64.

| Input | Version or receipt | Use in this lab |
| --- | --- | --- |
| SBCL | 2.6.7, `/opt/homebrew/bin/sbcl` | Saved executable images. |
| SWI-Prolog | 10.0.2 arm64-darwin, `/opt/homebrew/bin/swipl` | Saved state and subprocess runtime. |
| ECL | `command -v ecl` returned no path | Documented-only ECL rows. |
| handwritten kernel | `v7/labs/12_handwritten_logic/3_BUILD.lisp`; external image `/private/tmp/sprefa-lab12.kLyNlj/12_handwritten_logic` | Reused current image and committed build script. |
| library-backed bridge | `v7/labs/11_cl_prolog2/3_BUILD.lisp`; pinned external image `/private/tmp/cl-prolog2-lab.uuvw4c/image/cl-prolog2-lab-11-r2` | Reused current image and committed build script. |
| `cl-prolog2` SWI backend | pin `21531c553208e01c0b0b205ea005afaefa7057e3`, external checkout `/private/tmp/cl-prolog2-lab.uuvw4c/upstream` | Existing library route inspected for `libswipl`; its SWI backend launches `swipl` through `uiop:run-program`. |

## Authoritative documentation

- [ECL system building](https://ecl.common-lisp.dev/static/manual/System-building.html): ECL documents object files, static libraries, shared libraries, and programs. `c:build-static-library` and `c:build-shared-library` accept `:init-name`; C calls `cl_boot`, then `ecl_init_module`, then `cl_shutdown`. `c:build-program` constructs an executable.
- [ECL embedding](https://ecl.common-lisp.dev/static/manual/Embedding-ECL.html): `cl_boot` precedes ECL object creation or evaluation; `cl_shutdown` closes the environment. A foreign thread must call `ecl_import_current_thread` before executing Lisp and `ecl_release_current_thread` before exit. `ECLDIR` locates moved installation data.
- [SWI-Prolog foreign interface](https://www.swi-prolog.org/pldoc/man?section=foreign): C may call Prolog predicates and host `main` may embed the Prolog engine. The manual indexes `PL_initialise`, query creation, foreign frames, and `swipl-ld` for linking an embedded application.
- [SWI-Prolog mixed native/Prolog threads](https://www.swi-prolog.org/pldoc/man?section=foreignthread): a native thread that calls Prolog needs an attached engine. The one-to-one API is `PL_thread_attach_engine` and `PL_thread_destroy_engine`; the engine-pool API is `PL_create_engine`, `PL_set_engine`, and `PL_destroy_engine`.
- [SWI-Prolog deployment](https://www.swi-prolog.org/pldoc/man?section=runtime): saved-state deployment uses `qsave_program` and documents its runtime and foreign-code boundaries.
- [Rust foreign-function interface](https://doc.rust-lang.org/nomicon/ffi.html): `unsafe extern "C"` declares C ABI functions; `#[link]` selects dynamic or static native libraries; dynamic native dependencies reach the final binary boundary while static libraries are integrated into it.

No new embedding library was added. The only inspected Common Lisp bridge with the local SWI backend is `cl-prolog2`; the prior lab's source trace and image receipt record child-process transport rather than `libswipl` loading.
