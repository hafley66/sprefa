# DCG stack report

Winner baseline: 26486 non-comment, non-whitespace characters.

| Move | Source branch | Verdict |
|---|---|---|
| Fuse spec names and `record_cols/2` | `refactor/dcg-flash-a` (`e4b7be58`) | MOOT winner-already-has: `record_spec_names/2` |
| Unify declaration and host column readers | `refactor/dcg-flash-a` (`e4b7be58`) | REJECTED: `decl_b_column_type//3` has a cut and `host_col_type//3` does not; merging changes the accepted language |
| Fuse spec names and `record_cols/2` | `refactor/dcg-flash-b` (`8c60c63c`) | MOOT winner-already-has: `record_spec_names/2` |
| Unify column-type wrapper readers | `refactor/dcg-flash-b` (`8c60c63c`) | REJECTED: the declaration and host readers differ by a cut; merging changes the accepted language |
| Merge the two `sh_decl_stmt//1` clauses | `refactor/dcg-flash-b` (`8c60c63c`) | REJECTED: one-clause form loses the duplicate `column_type_wrapper` record |
| Remove anonymous slot filling | `refactor/dcg-flash-b` (`8c60c63c`) | MOOT winner-already-has: winner consolidated omitted-slot handling in `finish_omitted_slots/4` while retaining fresh output slots |
| Fold unary surface wrappers | `refactor/dcg-flash-b` (`8c60c63c`) | REAPPLIED -13 chars in `fe523a39` |
| Reuse computed CST input names | `refactor/dcg-flash-b` (`0a50ff5d`) | MOOT winner-already-has: `cst_body_variable_names/4` receives `InNames` directly |
| Share dotted-path term construction | `refactor/dcg-flash-b` (`bfcda556`) | MOOT winner-already-has: `path_atom/4` serves head and body atoms |
| Inline free-slot filling | `refactor/dcg-flash-b` (`9b5a8b94`) | MOOT winner rewrite: `fill_free_slots/3` serves both filled and omitted slots |
| Merge host normalization identity guards | `refactor/dcg-flash-b` (`9b5a8b94`) | MOOT winner rewrite: `map_tree/4` delegates leaf handling to `normalize_host_leaf/3` |
| Merge declaration and host column types | `refactor/dcg-flash-c` (`0a261531`) | REJECTED: the declaration and host readers differ by a cut; merging changes the accepted language |
| Share filled and anonymous slot handling | `refactor/dcg-flash-c` (`0a261531`) | MOOT winner-already-has: `fill_free_slots/3` handles both calls |
| Merge escape mappings | `refactor/dcg-flash-c` (`0a261531`) | MOOT winner-already-has: one `memberchk/2` table covers five recognized escapes |
| Share shell declaration head | `refactor/dcg-flash-c` (`0a261531`) | MOOT winner-already-has: `sh_head//2` |
| Reuse dollar-variable parser for brace keys | `refactor/dcg-flash-c` (`0a261531`) | MOOT winner rewrite: global variable state removed the threaded arguments that made this fusion useful |
| Share bind and comparison infix parsing | `refactor/dcg-flash-c` (`047c3ade`) | MOOT winner-already-has: `infix_item//2` |

## Reapplied commit scoreboard

| Commit | Characters | Text door | Parse parity | Conformance |
|---|---:|---|---|---|
| `fe523a39` | 26473, down 13 | compiled=266 byte_identical=266 failures=0 | total=677 parity=677 skips=0 diffs=0 | PASS, 0.52s wall |

The first text-door invocation inherited unavailable `C.UTF-8` and reported one character-encoding failure. Re-running with installed `en_US.UTF-8` produced the result above.
