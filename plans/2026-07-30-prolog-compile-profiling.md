# Prolog compile profiling

Date: 2026-07-30

Base verified read-only:

```text
a4629623ff484eeb460487fbda96506980a091a6
```

## Delivered surface

`v6/prolog/compile/scripts/compile_dl6.sh` retains its existing command when
`DL_PERF_LOG` is empty. When the variable names a file, the shell loads
`v6/prolog/compile/6_profile.pl` and calls `compile_dl6_profiled/2`.

The profiled path runs the same predicates and preserves their order:

| `tick` | `phase` | measured call |
|---:|---|---|
| 1 | `parse` | `parse_dl:parse_dl_file/4` |
| 2 | `plan` | `compile:program_plan/2`, including checks, analysis, and rule ordering |
| 3 | `lower` | `lower:lower_program/2` |
| 4 | `boot` | `lower:boot_statements/5` |
| 5 | `emit` | `emit_ts:emit_program/5` |
| 6 | `write` | output open, format, and close |

Each compile appends six JSON objects to `DL_PERF_LOG`, one object per line.
`tick` restarts at 1 for each compile. This follows the existing tsv2
`DL_PERF_LOG` conventions: JSONL, one aggregate record per measured unit,
lower snake case, and elapsed values named with an `_ms` suffix. The field set
is stable across success, failure, and exception:

```json
{"tick":2,"phase":"plan","source":"input.dl6","wall_ms":3545,"cpu_ms":3522.83,"inferences":83386137,"gc_count":1,"gc_reclaimed_bytes":125744,"gc_ms":0,"gc_left_bytes":9000,"table_count":0,"table_answers":0,"table_reuses":0,"table_space_bytes":0,"table_compiled_space_bytes":0,"status":"ok","error":null}
```

`status` is `ok`, `failed`, or `error`. `error` is null except for an exception,
where it contains SWI's rendered exception message.

## SWI mechanisms

### `statistics/2`: selected for phase records

Each phase takes snapshots before and after the call using these SWI keys:

| key | value used |
|---|---|
| `cputime` | seconds since thread start, subtracted and converted to `cpu_ms` |
| `inferences` | call and redo port count, subtracted |
| `walltime` | first element of `[MillisecondsSinceStart, MillisecondsSinceLast]`, subtracted into `wall_ms` |
| `garbage_collection` | deltas from `[Count, ReclaimedBytes, TimeMilliseconds, BytesLeft]`; `gc_left_bytes` retains the ending gauge |

The first `walltime` element is used because the second element has
since-last-call state shared by all callers. Differences between two first
elements give a phase-local elapsed time without depending on that shared
slot.

The 60-statement chain produced:

| phase | wall ms | CPU ms | inferences | GC count | reclaimed bytes | GC ms |
|---|---:|---:|---:|---:|---:|---:|
| parse | 6 | 4.82 | 73,868 | 1 | 130,240 | 0 |
| plan | 3,545 | 3,522.83 | 83,386,137 | 1 | 125,744 | 0 |
| lower | 5 | 4.16 | 32,398 | 3 | 377,920 | 0 |
| boot | 0 | 0.06 | 973 | 0 | 0 | 0 |
| emit | 14 | 12.44 | 98,265 | 10 | 2,750,184 | 1 |
| write | 1 | 0.56 | 177 | 0 | 0 | 0 |

CPU and wall time agree closely in the dominant phase. The curve is CPU work,
with no wait-shaped gap.

### `setup_call_cleanup/3`: selected for every boundary

Each measured goal runs inside `setup_call_cleanup/3`. The protected call
catches success, ordinary failure, and exception into an outcome term. Cleanup
always takes the ending snapshot and writes the JSON line. After cleanup, the
wrapper returns, fails, or rethrows to preserve the original control result.

Exception receipt:

```text
input:  rel broken(
exit:   2
line:   {"phase":"parse",...,"status":"error","error":"Unknown message: dl_parse_error(...)"}
```

The complete JSON line was flushed and the log stream was closed before the
exception left the process.

### Execution profiler: selected as a separate attribution pass

This SWI installation is 10.0.2. `library(prof)` does not exist. SWI reports
`profile/1` and `profile/2` under `library(prolog_profile)`, which is the
installed execution profiler. The bench uses `profile/2` for explicit options:
wall sampling, 1,000 samples per second, all ports, top 30 rows, and
non-cumulative display. It first compiles the same input once in the same
process to settle autoloading and file caches.

The report columns mean:

| output | attribution |
|---|---|
| `Calls + Redos` | entry and alternative-resumption port counts |
| `Exits + Fails` | successful and failed completion port counts |
| `Time:Self` | samples while the named predicate itself was executing |
| `Time:Children` | samples in descendants called below that predicate |

This is a sampling profiler. Its measured total includes profiler overhead, so
the external and `statistics/2` clocks supply elapsed-time claims. The
per-predicate percentages and call counts supply attribution.

The warm 60-statement report had a 4.575 second sampled total:

| predicate | calls | self | children |
|---|---:|---:|---:|
| `clock_check:clock_scc/3` | 32 | 0.00 s, 0.0% | 4.55 s, 99.5% |
| `clock_check:graph_reachable/4` | 45,632 | 0.63 s, 13.8% | 3.91 s, 85.4% |
| `clock_check:causal_dependency/4` | 39,046,545 | 1.82 s, 39.7% | 0.04 s, 1.0% |
| `lists:member_/3` | 1,309,064 | 1.80 s, 39.4% | 0.00 s, 0.0% |

The named inner hot predicate is `clock_check:graph_reachable/4`.
`clock_scc/3` is its cumulative caller.

### `prolog_trace_interception/4`: rejected for performance measurement

A noninteractive hook that returned `continue` at every visible trace port was
measured on the 20-statement chain. The ordinary compile took 0.09 seconds.
The traced compile took 0.48 seconds, a 5.3x ratio at the smallest benchmark
size. The hook provides frames, ports, and debugger actions. It does not
provide phase totals, and its per-port dispatch changes the cost being
measured. It remains usable for bounded debugger investigations.

### `debug/3` topics: rejected for performance measurement

The compiler has zero existing `debug/3` sites or declared debug topics.
Disabled `debug/3` calls are cheap but nonzero: 1,000,000 disabled calls took
0.089782 CPU seconds in this SWI build. Topics carry formatted diagnostic
messages through SWI's debug hook and destinations. They do not supply CPU,
inference, GC, or wall counters. Adding topic calls to the ordinary compiler
would also violate the exact off-path requirement. The separate profiled door
already has phase boundaries and structured output.

### Tabling statistics: selected where SWI tables

The plan and clock-check code declares no tabled predicate. The emit phase
calls `re_replace/4`; SWI's `library(pcre)` declares
`pcre:compile_replacement/2` as a shared variant table. `library(tableutil)`
snapshots therefore add stable per-phase deltas for:

```text
tables, answers, complete_call, space, compiled_space
```

For the 60-statement compile, parse, plan, lower, boot, and write recorded zero
for all five fields. Emit recorded 4 tables, 4 answers, 2,580 completed-table
reuses, 1,968 answer-table bytes, and 640 compiled-table bytes. The table is a
small emitter regex cache and does not participate in the plan-phase curve.

## Shell benchmark

Run from the repository root:

```sh
v6/prolog/compile/scripts/0_profile_compile_curve.sh
```

The generator emits exactly `N` statements:

```text
ceil(N/2) relation declarations
floor(N/2) rules in one dependent chain
```

This shape reproduces the observed slowdown while making statement, relation,
and rule counts explicit. The default sizes are 20, 40, 60, 80, 100, and 117.
`DL_PROFILE_SIZES`, `DL_PROFILE_REPEATS`, `DL_PROFILE_WARM`, and
`DL_PROFILE_EXEC_N` are optional controls.

The script selects `hyperfine` when present. It was absent on this machine, so
this receipt used `/usr/bin/time -p` with two repeats. One untimed compile ran
before measurement to populate file caches. Every timed compile used a fresh
SWI process.

After the table, the script calculates the final-interval exponent and prints
the answer directly:

```text
dominant_phase: plan
controlled_shape: plan scales as rules^<measured exponent>
general_driver: repeated simple-path enumeration in clock_check:graph_reachable/4
dense_graph_shape: simple-path count can be exponential in dependency-graph connectivity
```

The script prints its environment. This run reported:

```text
SWI-Prolog version 10.0.2 for arm64-darwin
Darwin 23.6.0 arm64, Apple T6020
runner: plain-time
repeats: 2
warm: yes; one untimed compile populated file caches
SPREFA_CONFIG=/nonexistent/x.toml
DL_NO_DAEMON=1
DL_DB_PATH=<mktemp>/scratch.sqlite
XDG_STATE_HOME=<mktemp>/state
```

It never invokes the daemon or the Rust/TS application. The scratch DB and
state root are inside its `mktemp` directory and removed by its exit trap.

### Measured curve

Values are means of two runs. All times are milliseconds.

| N | rels | rules | external wall | parse | plan | lower | boot | emit | write |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | 10 | 10 | 110 | 2.0 | 26.0 | 2.0 | 0.0 | 8.0 | 0.5 |
| 40 | 20 | 20 | 635 | 2.0 | 542.0 | 3.0 | 0.0 | 10.5 | 1.0 |
| 60 | 30 | 30 | 3,715 | 2.5 | 3,618.0 | 5.0 | 0.0 | 14.0 | 1.0 |
| 80 | 40 | 40 | 14,300 | 3.0 | 14,199.5 | 6.0 | 0.0 | 16.5 | 1.0 |
| 100 | 50 | 50 | 42,035 | 4.0 | 41,923.0 | 8.5 | 0.0 | 22.5 | 1.0 |
| 117 | 59 | 58 | 86,865 | 4.0 | 86,742.5 | 9.0 | 0.0 | 24.0 | 1.0 |

At 117 statements, plan is 99.86% of external wall time. The five other
phases total 38 milliseconds. The gap between the six phase totals and
external wall is SWI startup, module loading, the shell, and phase-record
writes.

## Growth answer

The chain fixture has approximately fifth-power growth in rule/relation count.
The chain has one simple route between any reachable pair, so this controlled
curve does not exercise path-count growth from a dense graph.

Adjacent local exponents, calculated as
`log(T2/T1) / log(R2/R1)` with `R = rule count`, converge toward 5:

| rules | local exponent from prior row | plan ms / rules^5 |
|---:|---:|---:|
| 10 | | 0.000260000 |
| 20 | 4.382 | 0.000169375 |
| 30 | 4.682 | 0.000148889 |
| 40 | 4.753 | 0.000138667 |
| 50 | 4.852 | 0.000134154 |
| 58 | 4.899 | 0.000132158 |

A fourth-power normalization rises from 0.0026 at 10 rules to 0.007665 at 58
rules. The fifth-power normalization approaches a constant.

The call shape in `3_clock_check.pl` accounts for the measured exponent:

1. `check_clock_program/1` asks `clock_violation/2`.
2. The path-conflict clause enumerates `recurrence_free_clock/4` paths.
3. Each path excludes productive cycles by calling `clock_scc/3`.
4. `clock_scc/3` considers node and peer pairs.
5. Each pair invokes recursive `graph_reachable/4`, which repeatedly scans
   the dependency list through `member/2` and `causal_dependency/4`.

On an acyclic dependent chain, the repeated SCC scan inside path enumeration
produces the observed near-quintic count. The profiler's 39,046,545 calls to
`causal_dependency/4` at 30 rules are the direct count receipt.

`graph_reachable/4` enumerates simple paths through recursive backtracking. A
dense acyclic dependency graph can have exponentially many simple paths.
Repeated path enumeration therefore gives the clock check a topology-sensitive
worst case beyond the chain's fifth-power shape.

### Repository real-program receipt

`v6/dl/fixtures/flagship-flow.dl6` parses to 125 statements: 94 declarations,
27 rules, and 4 queries. Its clock graph contains 42 nodes and 64 dependencies.
A hermetic profiled compile produced:

| external wall | parse | plan | lower | boot | emit | write |
|---:|---:|---:|---:|---:|---:|---:|
| 255,490 ms | 11 ms | 255,333 ms | 23 ms | 0 ms | 34 ms | 3 ms |

Plan used 252,962.94 CPU ms and 6,011,087,004 inferences. It accounts for
99.94% of external wall time. The other five phases total 71 milliseconds.
This reproduces the reported 117-statement, 232-second magnitude on the nearby
125-statement repository program.

Graph shape changes the cost materially. The 30-rule chain used 3,618 ms in
plan, while this 27-rule real program used 255,333 ms. A second repository
program, `golden-flex.dl6`, parses to 94 statements and used 888 ms in plan.
Statement and rule counts alone do not determine compile time.

Declarations without dependent rules measured 0.07 to 0.10 seconds from 20
through 117 statements.

## Output identity and off-path receipt

A generated 60-statement input was compiled three ways:

1. the direct pre-instrumentation goal,
   `swipl -q -l compile.pl -g "compile_dl6(Input, Output)" -g halt`;
2. `compile_dl6.sh` with `DL_PERF_LOG` unset;
3. `compile_dl6.sh` with `DL_PERF_LOG` set.

`cmp` succeeded for control versus off and control versus on. All outputs had
this SHA-256:

```text
e96206944ffa5545a0c9b46e4ccbfbe00550890d1d4da1e3293b3cc810270fc7
```

The profiled compile wrote exactly six JSONL lines.

The unset shell branch loads `compile.pl` and calls the exact original goal.
It does not load `6_profile.pl`, query `DL_PERF_LOG` from Prolog, take a
snapshot, or install a hook. Twenty alternating 20-statement measurements
compared an exact copy of the former shell body with the new unset branch:

```text
former door mean: 100.0 ms
new unset mean:   100.0 ms
all 40 samples:   0.10 s at /usr/bin/time -p resolution
```

No off-path cost was measurable at the available clock resolution.

## Verification

```text
git rev-parse HEAD
  a4629623ff484eeb460487fbda96506980a091a6

shellcheck 6_profile bench and compile door
  passed

git diff --check
  passed

compile plunit suite
  200/200 passed

profile exception receipt
  exit 2, complete parse JSON line with status=error

60-statement output cmp and SHA-256
  control = profiling off = profiling on
```

No full conformance sweep was needed for additive shell dispatch and a
separate module. One full compiler plunit run was used from the four-run
budget.

## Scope held

No changes were made to `parse_dl.pl`, `lower.pl`, `0_program_check.pl`,
`0_body_walk.pl`, or the hot clock-check predicates. No optimizer change was
attempted. The execution profiler was kept out of normal compilation because
its sampling and port bookkeeping alter runtime. The tracing hook and debug
topics were evaluated and left out for the measured reasons above.
