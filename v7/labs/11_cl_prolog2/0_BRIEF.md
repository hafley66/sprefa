# cl-prolog2 Bridge Lab Brief

Read the complete `common-lisp-logic` skill and its references. Own only this folder. Do not commit.

Upstream: `https://github.com/cl-model-languages/cl-prolog2`

Determine supported Prolog backends and run the shared fixture against installed SWI-Prolog 10.0.2 when supported. Trace transport, term conversion, variable identity, query lifecycle, multiple solutions, exceptions, threading, and runtime loading. Measure per-query bridge cost with a bounded local benchmark.

Attempt an SBCL executable image and inspect dynamic dependencies. Record whether the result invokes `swipl`, loads `libswipl`, or uses another transport. Write the required lab files.
