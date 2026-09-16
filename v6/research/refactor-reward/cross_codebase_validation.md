# Cross-codebase validation — rust-analyzer fixture

Date: 2026-06-27
Method: generated SCIP index for rust-analyzer fixture, ran all 9 kernels
on 5 hand-written files, read every top consensus range to verify.

## Ground truth

| File | Range | Kernels | What's there | Real? |
|------|-------|---------|-------------|-------|
| hir/lib.rs | L4188-4267 | 9 | `sources()` vs `primary_source()` — identical match/store dance, `.iter()` vs `.first()` | YES |
| hir/lib.rs | L6574-6600 | 8 | `could_unify_with` vs `could_unify_with_deeply` — one word difference | YES |
| hir/lib.rs | L5081-5108 | 9 | `display_place` projection walking — two methods share structure | YES |
| goto_def.rs | L170-184 | 7 | `?`-operator type check — repeated adt+args+check pattern | YES |
| goto_def.rs | L570-585 | 7 | `T![match]` / `T![=>]` arms — same find_branch_root->filter_map | YES |
| config.rs | L4287-4335 | 7 | 3 test fns (`proc_macro_srv_null/_abs/_rel`) — same setup | YES |
| goto_def.rs | L90-96 | 7 | dispatch chain (`if let Some(n) = find_def_X`) | MARGINAL |
| extract_fn.rs | L2233-2919 | 6 | test fixtures (`check_assist` x hundreds) | NO |
| goto_def.rs | L1598-1888 | 4 | test fixtures (`check(r#"..."#)`) | NO |
| render.rs | L3975-4079 | 6 | test fixtures (`check_relevance(r#"..."#)`) | NO |

6/7 high-consensus proposals (7-9 kernels) are genuine dups.
All false positives are test-fixture structural similarity.

## Precision signal

Proposals backed by >=1 structural kernel (tree/cfg/cgraph/ddg) are real.
Proposals backed only by recall kernels (ast/ngram/symbol/verbatim) are
likely test noise. The structural kernels see through test wrappers
(check_assist/check_relevance calls) to the different test bodies inside
and correctly don't match.

## SCIP impact

Symbol kernel shrinks 15-40% vs ast and drops >10param to 0 on every file.
Call-seq produces 1-10 blocks per file (was 0 without SCIP).
Consensus gets stronger: top ranges promote from 8->9 kernel agreement.
