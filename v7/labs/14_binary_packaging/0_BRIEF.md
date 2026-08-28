# Common Lisp And SWI Packaging Brief

Read the complete `common-lisp-logic` skill and its references. Own only this folder. Do not commit.

This lab starts after at least one Common Lisp logic lab and the handwritten kernel have runnable SBCL images.

Measure these shapes where locally possible:

1. minimal SBCL executable image
2. handwritten logic-kernel SBCL executable
3. one successful library-backed SBCL executable
4. minimal SWI saved-state executable
5. CL executable invoking SWI as a subprocess
6. CL executable loading or linking `libswipl` when an existing library supplies that route

Record file bytes, total required distribution bytes, `file`, `otool -L`, five startup samples, peak RSS, and one bounded cross-runtime query benchmark. Research ECL static/shared embedding and a native Rust or C host containing both runtimes. Separate locally measured rows from documented shapes.

Write `1_SOURCES.md`, `2_MEASUREMENTS.md`, `3_SINGLE_BINARY_SHAPES.md`, and minimal numbered source/build files.
