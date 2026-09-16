# BRIEF: recon — what does `?-` actually do in v6, end to end

You are a read-only investigator. Work ONLY inside
/Users/chrishafley/projects/sprefa-recon-query. You may READ any file in
this worktree; the ONLY file you may WRITE is REPORT.md at the worktree
root. No git commands that mutate (status/log/grep/show are fine). No
subagents. If a question below cannot be answered from the code, write
"UNKNOWN: <why>" for it; never guess.

## The question being verified (context, do not treat as truth)
The user believes: a `?-` query written in a .dl6 file is a literal
standing question = semantically the subscribe operation = the only
demand root; compile is eager (all table defs up front) but nothing
"clocks" until a query demands it. Your job is to report what the system
AS BUILT actually does, with file:line receipts for every claim.

## Trace the whole pipeline for queries, with receipts
1. PARSE: where `?-` is parsed (v6/prolog/compile/parse_dl.pl or
   neighbors). What term does a query become? Can a file hold many?
2. EXPANSION/CHECKS: how query terms travel through 1_expansion.pl
   phases and any checks. Are queries typechecked like rule bodies?
3. LOWERING/EMIT: in the compiler (lower.pl / emit_ts.pl or the actual
   emitters), what TypeScript does a query become? Receipts from a real
   generated file (compile/dl_view builds, v6/tsv2, or compile a small
   fixture yourself with the in-worktree scripts if one exists). Is the
   emitted query: (a) a one-shot SELECT at boot, (b) a standing
   subscription re-evaluated per tick, (c) something else?
4. RUNTIME: in v6/tsv2 (runtime/serve/cli), when do query results
   compute? Is there any demand/laziness machinery today (rels computed
   only when queried), or does the engine evaluate every rule every tick
   regardless of queries? Look for: demand, refCount, subscribe,
   lazy, topo/stratum scheduling keyed off queries.
5. ORACLE: does the Prolog reference engine (v6/prolog/conformance/
   engine.pl, level_eval.pl) treat queries as demand roots or evaluate
   all rules and merely PRINT queried rels at the end?
6. DDL: are all table defs created up front regardless of queries
   (v6/sprefa-store or the emitted boot code)? Receipt.
7. SERVE: in v6/tsv2 serve, what re-runs queries when new rows arrive —
   push (subscription) or poll (re-select)?

## Verdict section (last, one paragraph)
State plainly: is the user's "?- is the subscribe, everything else is
lazy" TRUE today, PARTIALLY true (say which half), or FALSE (say what
actually drives evaluation). Then list, as bullet points, the smallest
set of changes that would make it true, each with the file it touches.
No design opinions beyond that list.

## Format
REPORT.md sections: parse / expansion / lowering+emit / runtime / oracle
/ ddl / serve / verdict. Every claim carries file:line. Quote at most 3
lines of code per receipt.
