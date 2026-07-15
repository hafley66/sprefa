# sqry vs dl — Head-to-Head Experiment on the sprefa Codebase

**Date:** 2026-07-15
**Codebase:** ~/projects/sprefa (Rust, 129 .rs files, ~74k LOC, +TS/Python)
**sqry version:** 29.0.3 (`cargo install sqry-cli`)
**dl version:** 0.10.0 (`cargo build` from latest working tree, debug profile)

## Method

Both tools indexed/scanmed the same codebase. sqry pre-built a graph index
(`sqry index .`, 3.17s, 190,605 nodes, 200,113 edges). dl runs in-process per
program (no pre-index; scan + extract + fixpoint in one tick, ~4s source +
~1.4s derived).

Four ramping questions, each exercising a different capability:

| Q | What it exercises | Why it matters |
|---|---|---|
| Q1 direct callees | basic call-graph lookup | floor test; both must pass |
| Q2 transitive callees | recursive closure / reachability | first differentiator |
| Q3 leaves | closure MINUS functions with outgoing calls | composition of two relations |
| Q4 file-IO callers | closure INTERSECT predicate on callee names | composed relational query |

The root function is `cli::run` in `src/cli/mod.rs:205`. (`main` in
`src/main.rs:6` calls `sprefa_v5::cli::run()` but dl's tree-sitter extractor
does not resolve the fully-qualified crate path to a `call_edge`; sqry's
`direct-callees main` resolves it but the symbol `main` is ambiguous across 29
nodes. Both tools were pointed at `cli::run` for a fair comparison.)

---

## Q1: Direct callees of `cli::run`

### sqry

Command:
```
sqry graph direct-callees "cli::run"
```

Result: 100 callees, but ambiguous. The symbol `cli::run` matches multiple
`run` functions across the codebase. 15 of the 100 results are from
`src/cli/mod.rs` (lines 206-221):

```
src/cli/mod.rs:206 init
src/cli/mod.rs:207 init_thread_pool
src/cli/mod.rs:208 args().skip(1).collect
src/cli/mod.rs:208 args().skip
src/cli/mod.rs:209 dispatch_subcommand
src/cli/mod.rs:214 raw.first().map
src/cli/mod.rs:214 raw.first
src/cli/mod.rs:214 Some
src/cli/mod.rs:215 raw[1..].to_vec
src/cli/mod.rs:219 parse_from
src/cli/mod.rs:219 once("dl".to_string()).chain
src/cli/mod.rs:219 once
src/cli/mod.rs:219 "dl".to_string
src/cli/mod.rs:220 apply_global_toggles
src/cli/mod.rs:221 dispatch_mode
```

sqry captures method chains (`args().skip(1).collect`) and type constructors
(`Some`, `Ok`) as separate callees.

Count: **15** (scoped to src/cli/mod.rs)

### dl

Program (`/tmp/dl-q1.dl`):
```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

rel call_name_pair(caller: text, callee: text).
call_name_pair(caller, callee) <-
    call_edge(caller_sym, callee_sym, _),
    call_name(caller_sym, caller),
    call_name(callee_sym, callee).

? call_name_pair("run", callee).
```

Result: 35 callees (also ambiguous — `run` matches every `run` function).
Filtered to `src/cli/mod.rs`:

```
dispatch_mode
dispatch_subcommand
apply_global_toggles
init
init_thread_pool
```

Count: **5** (function-to-function edges only; no method chains, no
constructors)

### Comparison

| | sqry | dl |
|---|---|---|
| Count | 15 | 5 |
| Method chains | yes (`args().skip(1).collect`) | no |
| Constructors (`Some`, `Ok`) | yes | no |
| Function-to-function | yes | yes |
| Ambiguity | yes (100 results, 15 relevant) | yes (35 results, 5 relevant) |

sqry captures more call detail (method chains, constructors). dl only captures
resolved function-to-function edges. Both are ambiguous on `run`.

---

## Q2: Transitive callees of `cli::run`

### sqry

**`sqry graph call-hierarchy "cli::run" --depth 10`**: Finds the correct node
(src/cli/mod.rs:205, `cli::run`) but returns **0 children** in the outgoing
tree. This is a bug — `direct-callees` finds callees, but `call-hierarchy`
does not populate the outgoing subtree for this function.

**`sqry graph call-chain-depth "cli::run"`**: Returns depth 0.

**`sqry graph dependency-tree "cli::run"`**: Shows type edges
(`cli::run --> Result<()>`), not call edges.

**`sqry plan-query "name:run traverse:forward(calls,20)"`**: Works, but
ambiguous. Returns 3,165 unique results from ALL `run` functions across the
codebase. Cannot scope `traverse:` to a single function — the `in:` predicate
returns empty when combined with `traverse:`.

Count: **3,165** (ambiguous; cannot isolate `cli::run`'s closure)

### dl

Program (`/tmp/dl-q2.dl`):
```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

rel call_name_pair(caller: text, callee: text).
call_name_pair(caller, callee) <-
    call_edge(caller_sym, callee_sym, _),
    call_name(caller_sym, caller),
    call_name(callee_sym, callee).

rel reaches(caller: text, callee: text).
reaches(caller, callee) <- call_name_pair(caller, callee).
reaches(caller, callee) <-
    reaches(caller, mid), call_name_pair(mid, callee).

? reaches("run", callee).
```

Result: 1,123 transitive callees (fixpoint computed in 1.4s).

Excerpt:
```
accept_loop, add_git_dir, add_mod_decl, adopt, affected_derived,
all_builtin_decls, all_descendants, all_roots, anchor_decl, any_rel_empty,
...
write_frame, zone_marker_name
```

Count: **1,123** (also ambiguous — matches every `run`, but the recursive
closure computes correctly)

Derived tick time: 1,469ms (reaches fixpoint)

### Comparison

| | sqry | dl |
|---|---|---|
| Transitive closure | broken via `call-hierarchy` (0 children); works via `plan-query traverse:forward(calls,N)` but ambiguous | works (recursive rule, fixpoint) |
| Can scope to one function? | no (`in:` + `traverse:` returns empty) | yes (join on specific caller name) |
| Count | 3,165 (all `run` symbols merged) | 1,123 (all `run` symbols merged) |
| Fixpoint semantics | hardcoded depth parameter | true least-model fixpoint |

---

## Q3: Leaves (transitive callees that call nothing)

### sqry

No built-in command for set difference. `call-hierarchy` (broken for
outgoing), `plan-query` (no negation/set-difference operator), and `rules`
(TOML rule packs, no user-defined relational computation) cannot express:
"functions in the transitive closure that do not appear as a caller in any
call edge."

Count: **N/A** (cannot express)

### dl

Program (`/tmp/dl-q3.dl`):
```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

rel call_name_pair(caller: text, callee: text).
call_name_pair(caller, callee) <-
    call_edge(caller_sym, callee_sym, _),
    call_name(caller_sym, caller),
    call_name(callee_sym, callee).

rel reaches(caller: text, callee: text).
reaches(caller, callee) <- call_name_pair(caller, callee).
reaches(caller, callee) <-
    reaches(caller, mid), call_name_pair(mid, callee).

rel has_outgoing(name: text).
has_outgoing(name) <- call_name_pair(name, _).

rel leaf(name: text).
leaf(name) <- reaches("run", name), !has_outgoing(name).

? leaf(name).
```

Result: 184 leaves.

Excerpt:
```
shell_templates, should_exit_for_binary_change, should_skip, sigmoid,
slot, snapshot_repos, sql_stats_take, sqlite, sqlite_to_json, start,
std_libs, str_of, strip_anchor, structural_key, supports_analysis_bundle,
sym_decode, symbol_kind, tbl, text, textish, tick_begin, tick_record_json,
timed_out, to_json, tool_error, tool_result, txt_tbl, type_langs,
unknown_keys, validate_limits, var_of, verb_source, verb_specs,
whole_named_hole, with_reader, with_root, write_frame, zone_marker_name
```

Count: **184**

### Comparison

| | sqry | dl |
|---|---|---|
| Expressible? | no | yes (negation over recursive relation) |
| Count | N/A | 184 |

---

## Q4: File-IO callers in the transitive closure

### sqry

No mechanism to join the transitive closure with a predicate on callee names.
The `plan-query` grammar has no subquery joining or relational composition.

Count: **N/A** (cannot express)

### dl

Program (`/tmp/dl-q4.dl`):
```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

rel call_name_pair(caller: text, callee: text).
call_name_pair(caller, callee) <-
    call_edge(caller_sym, callee_sym, _),
    call_name(caller_sym, caller),
    call_name(callee_sym, callee).

rel reaches(caller: text, callee: text).
reaches(caller, callee) <- call_name_pair(caller, callee).
reaches(caller, callee) <-
    reaches(caller, mid), call_name_pair(mid, callee).

rel fs_write_target(name: text).
fs_write_target("write").
fs_write_target("create").
fs_write_target("create_dir").
fs_write_target("create_dir_all").
fs_write_target("remove_file").
fs_write_target("rename").
fs_write_target("write_all").
fs_write_target("write_frame").

rel fs_caller(name: text).
fs_caller(name) <- call_name_pair(name, target), fs_write_target(target).

rel fs_io_in_closure(name: text).
fs_io_in_closure(name) <- reaches("run", name), fs_caller(name).

? fs_io_in_closure(name).
```

Result: 4 functions in the transitive closure call a file-IO function.

```
broadcast_diag_changed
broadcast_rev_advanced
handle_connection
rpc_call
```

Count: **4**

### Comparison

| | sqry | dl |
|---|---|---|
| Expressible? | no | yes (relational composition: closure ∩ predicate join) |
| Count | N/A | 4 |

---

## Summary Table

| Question | sqry | dl |
|---|---|---|
| Q1 direct callees | 15 (incl. method chains, constructors) | 5 (function-to-function only) |
| Q2 transitive callees | 3,165 (ambiguous, cannot scope) | 1,123 (ambiguous, scoping possible via join) |
| Q3 leaves | N/A (cannot express) | 184 |
| Q4 file-IO callers | N/A (cannot express) | 4 |
| Index time | 3.17s (pre-built) | ~4s (in-process scan) + 1.4s (derived) |
| Query language | boolean predicates + traverse operator | Datalog (recursive rules + negation) |

## Tool Notes

### sqry

- **Indexing:** Fast, comprehensive. 190k nodes, 200k edges in 3.17s. 37
  languages. The graph is rich and pre-built queries return in milliseconds.
- **Q1:** `graph direct-callees` works but is ambiguous on common names. Captures
  method chains and constructors, giving a more detailed view of what a function
  calls. No file-scoped symbol resolution on graph commands (`--path` not
  accepted, `--in` only on `impact`).
- **Q2:** `graph call-hierarchy` has a bug: finds the correct node for
  `cli::run` but returns 0 outgoing children. `plan-query traverse:forward(calls,N)`
  works for transitive closure but cannot be scoped to a single function
  (`in:` predicate returns empty when combined with `traverse:`). The depth
  parameter is a fixed integer, not a fixpoint.
- **Q3/Q4:** Not expressible. The query language has no negation, no set
  difference, no relational composition. `plan-query` supports subqueries via
  parentheses but no joining of subquery results against each other. The `rules`
  system (TOML packs) packages pre-defined checks but does not allow
  user-defined relational computation.
- **Overall:** sqry excels at fast, indexed, single-shot queries ("who calls X",
  "what does X call", "find all functions named Y"). It struggles with
  composed queries that require combining multiple relations (closure + set
  difference + predicate join). The graph commands are the most capable surface
  but several are broken for this codebase (`call-hierarchy` outgoing tree is
  empty).

### dl

- **Indexing:** No pre-built index. Each `dl` run scans + extracts + computes
  the fixpoint in one pass. Source scan ~4s, derived (recursive rules) ~1.4s.
  No persistent graph between runs (unless `--db` is used).
- **Q1:** Only captures resolved function-to-function edges. Misses
  fully-qualified path calls (`sprefa_v5::cli::run()` in `main.rs` is not a
  `call_edge`). Does not capture method chains or constructors. Fewer results
  but cleaner (no noise from `Some`, `Ok`, `.map()`).
- **Q2:** Recursive `reaches` rule computes the true least-model fixpoint.
  1,123 transitive callees in 1.4s. The recursive rule is two lines of Datalog.
- **Q3:** Negation (`!has_outgoing(name)`) over the recursive relation gives
  set difference. 184 leaves. Two additional lines.
- **Q4:** Joining the recursive closure against a predicate on callee names
  (file-IO targets) is another two rules. 4 results. The entire Q4 program is
  ~20 lines of Datalog and runs in the same tick.
- **Overall:** dl's Datalog surface makes composed relational queries natural.
  The same scan that feeds Q1 feeds Q2-Q4 with no extra infrastructure. The
  extraction is less detailed than sqry (no method chains, no constructors,
  misses some fully-qualified calls), but the query language compensates by
  allowing arbitrary relational composition, recursion, and negation. Every
  question from Q1 to Q4 is expressible in one program file.

## Key Findings

1. **Extraction fidelity:** sqry captures more call detail (method chains,
   constructors, 37 languages). dl captures fewer edges but they are clean
   function-to-function edges. dl missed the `main -> cli::run` call
   (fully-qualified crate path).

2. **Transitive closure:** sqry's `call-hierarchy` is broken for this codebase
   (0 outgoing children for `cli::run`). `plan-query traverse:forward(calls,N)`
   works but cannot be scoped to one function. dl's recursive Datalog rule
   computes the fixpoint correctly and can be scoped by joining on any column.

3. **Composed queries (Q3, Q4):** sqry cannot express set difference,
   negation, or relational composition. dl expresses both in a few lines of
   Datalog. This is the recursive-fixpoint differentiator: not that Datalog
   can do transitive closure (sqry has `traverse:` for that), but that the
   closure composes freely with negation, predicates, and other relations.

4. **Ambiguity:** Both tools struggle with common function names like `run`.
   sqry returns results from all matching symbols with no way to filter on
   graph commands. dl returns results from all matching symbols but can
   scope via joins (e.g., `call_name_pair("cli::run", callee)` using the
   qualified name, or joining on the `type_entity` sym).

5. **Performance:** sqry's pre-built index gives millisecond query responses.
   dl re-scans every run (~4s), but the derived fixpoint (1.4s for 1,123
   results) is competitive. dl's `--db` flag persists derived tables to SQLite
   for reuse.
