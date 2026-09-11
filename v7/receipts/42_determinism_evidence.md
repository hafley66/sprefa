# DL7 Prolog determinism evidence

## Scope

Added instrumentation and test tooling only. No compiler, evaluator, checker,
graph, or IR predicate was changed. The default report probes a bounded
selected set of existing DL7 predicates with recorded concrete calls:

```text
dl7_checker:check_datalog/4
dl7_evaluator:evaluate/4
dl7_evaluator:integer_comparison/3
dl7_evaluator:stratify_rules/3
```

The new Prolog tooling contains no cut clauses.

Owned files:

- [`1_determinism_evidence.pl`](/Users/chrishafley/projects/sprefa/v7/bench/1_determinism_evidence.pl)
- [`22_determinism_evidence.test.pl`](/Users/chrishafley/projects/sprefa/v7/test/22_determinism_evidence.test.pl)
- [`v7-determinism-evidence.sh`](/Users/chrishafley/projects/sprefa/scripts/v7-determinism-evidence.sh)
- this receipt

## Interfaces

The report module exports:

```prolog
main/0
default_config(-Config)
default_capture_config(-Config)
compile_fixture_report(+Fixture, -Report)
determinism_report(+Config, -Report)
aggregate_captured_entries(+CapturedEntries, -ObservedCalls)
classify_call(+QualifiedCall, -GroundnessMode, -SolutionClass)
leftover_choicepoint(+QualifiedCall, -LeavesChoicepoint)
```

The direct helper configuration remains bounded to at most 16 selected
predicate indicators and 32 recorded calls:

```prolog
evidence_config(
    [selected(Module, Name, Arity), ...],
    [recorded(Label, QualifiedCall), ...])
```

The automatic capture configuration is:

```prolog
capture_config(
    [selected(Module, Name, Arity, pure_or_non_replayable), ...],
    MaxReplayInputs)
```

The default CLI invokes
`compile_fixture_report/2` on
`v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`. It installs named
temporary SWI wrappers only for the allowlisted predicate indicators, copies
each entry call before delegation, fingerprints it immediately, and removes
all wrappers through `setup_call_cleanup/3` on success, failure, or exception.
Only `pure` entries are replayed. `evaluate/4` and `check_datalog/4` are
observed as `non_replayable` and are never executed a second time.

`classify_call/3` computes `groundness_mode/2` once and uses `findnsols/4` with
a marker template and limit `2`, so
the result is exactly `zero`, `one`, or `multiple` without retaining answer
terms. Groundness is an ordered hyphenated mode such as
`ground-ground-variable-variable`. A normalized `term_hash/2` fingerprint is
the only call identity retained in the report.

Captured entries retain two internal forms: a replayable `CallCopy` and a
canonical normalized call made with `copy_term/2` followed by `numbervars/3`.
`aggregate_captured_entries/2` groups on the exact canonical call, along with
predicate, groundness mode, and replayability. The integer fingerprint remains
the only serialized identity. This prevents distinct normalized calls that
share a supplied or computed hash from being merged. The exported aggregation
predicate is a narrow pure seam for the collision regression test.

`leftover_choicepoint/2` first observes the first answer with a capped probe,
then uses `call_cleanup/2` to inspect whether cleanup has already fired. The
choicepoint flag is read before the probe is committed. `once/1` only makes the
evidence probe itself deterministic after that flag is recorded. Zero-answer
calls report `none`; successful deterministic and nondeterministic calls report
`false` and `true` respectively.

The automatic JSON schema is:

```text
schema: "dl7-determinism-evidence-v1"
fixture,
compile_status,
observed_calls: [{predicate, groundness, call_count, replayability, fingerprint}],
unique_inputs,
replayed_inputs,
classification: [{predicate, status, solution_class, leaves_choicepoint, ...}]
```

JSON is written to stdout. A concise fixed-column `DL7-DETERMINISM` table is
written to stderr. The compiler's separate trace summary also remains on
stderr and can contain run-dependent wall fields. Full calls and answer terms
are excluded from the report.

## Default report

The shell entry point is:

```bash
./scripts/v7-determinism-evidence.sh
```

One nearest-shadow run observed `34` unique inputs and replayed `29` pure
inputs. The captured report included these stable examples:

```text
dl7_checker:check_datalog/4        ground-ground-variable-variable  2 non_replayable 937236472
dl7_evaluator:evaluate/4           ground-ground-variable-variable  1 non_replayable 1468190788
dl7_evaluator:integer_comparison/3 ground-variable-variable        294 pure 428544702
dl7_evaluator:integer_comparison/3 variable-variable-variable      12 pure 1094319208
dl7_evaluator:stratify_rules/3     ground-variable-variable        3 pure 1293250615
```

The report contains other fingerprints for the same predicates. The
`integer_comparison/3` counts greater than `1` are captured from the real
compile entry stream, not hand-authored input calls. Classification rows mark
pure inputs as `replayed` with `zero`, `one`, or `multiple`, and mark stateful
inputs as `non_replayable` without executing them again.

## Tests and gates

The new test file passed `9/9` in one SWI process under a 20-second command
timeout. Individual test times were at most `1.154` seconds. Coverage includes:

- zero, one, and multiple solution classes;
- groundness mode fingerprints;
- deterministic, residual-choicepoint, and zero-answer calls;
- aggregated call counts and integer-only fingerprints;
- exact canonical grouping when two distinct calls are supplied the same
  fingerprint;
- deterministic machine-readable JSON and text-table output across two fresh
  process runs;
- one real nearest-shadow compile with repeated observed call counts greater
  than `1`;
- explicit non-replayable classification for observed `evaluate/4` calls;
- wrapper removal after a successful compile and after a compile exception;
- nonzero exit `2` for unknown predicate and invalid configuration;
- unknown predicate rejection before recorded calls execute.

The shell entry passed `bash -n`. Default execution exited `0`; both
`--unknown-predicate` and `--invalid-config` exited `2` with a
`DL7-DETERMINISM-ERROR` line.

No parser files were edited. No CI coverage was added.
