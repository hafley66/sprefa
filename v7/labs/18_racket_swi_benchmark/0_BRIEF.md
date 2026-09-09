# Racket and SWI Logic Benchmark

Measure locally installed Racket 9.3 Datalog, Racket 9.3 Racklog, and
SWI-Prolog 10.0.2 on matching assertion and transitive-closure fixtures.

Record separately:

- engine-only elapsed time after process startup;
- complete process wall time where useful;
- assertion count and closure answer count;
- cyclic single-source closure;
- acyclic all-pairs closure;
- installed runtime, package tree, executable, and distribution bytes;
- the semantic facilities each engine supplies.

Generated executables, distributions, downloaded sources, and raw timing logs
belong under `/private/tmp`, never in Git.

