# Report: failure-ledger entries for two unfiled defects

Worktree: `/Users/chrishafley/projects/sprefa-lanes/t4-ledger/flash`.
No commits. Changes left uncommitted.

## What I wrote, and where

Both entries added to `docs/failure-modes.md`, numbered the next two free
classes after 38: **class 39** (Entry A) and **class 40** (Entry B), inserted
between class 38's body and the `## Rail gap table`, with two matching rows
added to the gap table. Prose was matched to the doc's existing format
(`- WHAT IT LOOKS LIKE / - HOW IT BIT US / - LAW / - RAIL / - SAY THIS TO AN
AGENT`) and its em-dash (—) convention. Entry numbering is continuous from 38;
no renumbering of existing classes was needed. Mirroring the pre-existing gap
table (which omits classes 25/33/34/37), only my two new rows were appended.

### Entry A — class 39, `Run-capped outer kill orphans a backgrounded served engine`

> ## 39. Run-capped outer kill orphans a backgrounded served engine
>
> - WHAT IT LOOKS LIKE: a receipt script backgrounds a served node engine
>   (`node --experimental-transform-types "$SERVE_MAIN" ... &`, `SERVER_PID=$!`),
>   the outer command is killed by a run cap, and the SERVER survives as an
>   orphan still holding its port; the next run dies `EADDRINUSE` on that port.
>   The cleanup trap (`trap stop_server EXIT`) never ran because the cap killed
>   the shell it was attached to. A listener nobody owns is class 35's squatter,
>   and the port-collision family v6/tsv2's serve tests document is the same
>   spine playing out one port at a time.
> - HOW IT BIT US (2026-07-31, atlas arc): a receipt script started a served node
>   engine in the background; the outer command was killed by a 60-second run
>   cap; the server survived as an orphan holding its port; the next run died
>   EADDRINUSE on port 17811. The existing timeout-gun receipts do not cover this
>   path: class 38 proves the process-group leg on a direct-child backgrounded
>   server (`cap_self` on files.sh -> zero surviving `serve/main.ts`; the command
>   form on a shell that backgrounds a child -> 124 with the backgrounded
>   grandchild dead), but that coverage holds only while the server stays in the
>   script's process group. FACTS UNVERIFIED FROM THE REPO: the port 17811
>   appears nowhere here (the closest documented port-collision is 17611, in
>   v6/tsv2/tests/serveHelpers.ts:135 and serveLifecycle.test.ts:19-49), and the
>   atlas receipt script itself is gone, because the dataflow atlas was scraped
>   2026-07-31 (commit 2c08ea62 removes atlas.sh) — so whether the server
>   escaped the group via a nohup/setsid detach, or the outer cap was a bare
>   `perl -e 'alarm N; exec'` timing orphaning one-liner (class 38's residual)
>   rather than `cap_self`, cannot be confirmed from the repo. Both are real
>   escape routes for the same class.
> - THE LAW: a served engine's lifetime is enforced from inside the process that
>   would dangle (class 35's law), and a process-group kill covers only the
>   processes still in the group. A server launched through a detaching wrapper
>   (nohup/setsid) or reaped by a cap that signals only the outer process
>   survives every mechanism the repo's own receipts prove — so a receipt that
>   backgrounds a server must either keep it a direct child of the script's
>   group, or own its port at boot.
> - THE RAIL: MISSING. Proposed, the two halves the incident splits into:
>   (a) a PORT-FINGERPRINT boot check in receipt scripts — before starting, ask
>   `lsof -i :$PORT` (or probe `/ticks` and `kill -0` the answering pid) and fail
>   loudly if the port is already held by a pid this script did not spawn,
>   instead of discovering it at the first POST with EADDRINUSE; and (b) keep
>   the process-group kill on cap, enforced by launching the server as a direct
>   child in the script's group the way files.sh and extraction-live.sh already
>   do (files.sh:70-75, extraction-live.sh:84-89: `stop_server` +
>   `trap stop_server EXIT` + `cap_self`) — the cleanest existing lifecycle
>   pattern is the rail candidate, and the detaching forms behind it are
>   forbidden. A fail-pre-fix test is owed per the pipeline for both halves
>   before either counts as enforced.
> - SAY THIS TO AN AGENT: before you boot a served engine in a receipt, check
>   who already owns the port (`lsof -i :$PORT`) and fail loudly; then keep the
>   server a direct child of the script's process group (files.sh /
>   extraction-live.sh `stop_server` + `trap` + `cap_self`), never behind
>   nohup/setsid. A server that survives an outer kill is a squatter that
>   EADDRINUSE-complains later, and it is class 35's orphan wearing a port.

### Entry B — class 40, `Coalesce empty-group idiom: no fixture, no documented spelling`

> ## 40. Coalesce empty-group idiom: no fixture, no documented spelling
>
> - WHAT IT LOOKS LIKE: `coalesce` over an aggregate that derives ZERO rows for
>   a group. An aggregate keyed on a group rel emits no row for a group whose
>   members produce no aggregate input, so any formula where an empty group
>   still owes a term drops that term silently. The number stays plausible and
>   bends in the safe-looking direction — a modularity sum whose only missing
>   terms are the negative ones reads as a near-optimal score — so nothing at
>   the surface flags it.
> - HOW IT BIT US (2026-07-31, auto-factorization lab,
>   plans/2026-07-31-auto-factorization-verdict.md): the draft read
>   `und_internal_total` directly and the file axis returned 0.0 against
>   networkx's -0.0278. Every group with zero internal edges produced NO ROW
>   from the `count`/`sum` aggregate, so its term left the sum — and on the
>   file axis that is EVERY group (verdict:249-265). It is the language design
>   review's finding A11 ("count never 0") biting a real program, and the
>   verdict carries it twice as finding 9: "the `und_internal_filled` shape from
>   `numbering.dl6` as the worked example of the empty-group idiom (finding 9),
>   which today exists nowhere" (verdict:1032-1034); "coalesce against the group
>   rel is the empty-group idiom, and nothing says so. Finding 9 cost this lab a
>   plausible wrong number" (verdict:1054-1057). Fix shape:
>   `und_internal_filled(axis, grp, edges) <- axis_group(axis, grp),
>   coalesce(und_internal_total(axis, grp, edges), 0)` (verdict:256-260).
>   CLASS: missing coverage, not a crash — the aggregate was correct; the
>   caller forgot the empty group owed a term.
> - THE LAW: an aggregate over a group rel derives zero rows for an empty group;
>   any expression that still owes a term per group must put the zero back by
>   hand with `coalesce(..., 0)`. And an idiom that grading cannot see gets a
>   fixture: a measured, silent, wrong-in-the-safe-looking-direction answer that
>   only a referee catches is exactly the class the fixture corpus pins.
> - THE RAIL: MISSING. Proposed, per the verdict's own recommendation
>   (verdict:1054-1057): the cheapest fix is documentation plus a fixture; the
>   honest fix is a check that an aggregate feeding an arithmetic expression
>   over a group rel has a filled source. The fixture would be
>   `coalesce-empty-group.dl6` in v6/dl/fixtures/, following that directory's
>   kebab-case descriptive `.dl6` convention (beside extraction-live.dl6,
>   files-hosts.dl6, served-endurance.dl6): a program where an aggregate over a
>   group rel derives zero rows for at least one group, and the
>   `coalesce(..., 0)` floor recovers the empty group's row, with the reference
>   output graded the way the other conformance fixtures are. As written today
>   the idiom survives only in the lab, which dies on landing (recoverable via
>   `git show 7e4a8c62:v6/prolog/labs/auto_factorization/numbering.dl6`,
>   verdict:1017-1023, 1076).
> - SAY THIS TO AN AGENT: `coalesce` over an aggregate that derives zero rows
>   for a group is the empty-group idiom — an aggregate emits no row for an
>   empty group, so put the zero back by hand (`coalesce(..., 0)`) in any
>   expression that owes a term per group. It has no fixture yet (proposal:
>   `v6/dl/fixtures/coalesce-empty-group.dl6`), so do not invent a different
>   spelling.

## Facts I could not verify from the repo (marked in the entries, need coordinator confirmation)

1. **Port 17811 (Entry A).** The number 17811 appears nowhere in this
   worktree. The closest repository evidence is a distinctly-numbered
   EADDRINUSE port-collision family documented in `v6/tsv2/tests/`
   (`serveHelpers.ts:135` cites 17611; `serveLifecycle.test.ts:19-49`), which
   I cited as the adjacent documented class. The exact `:17811` from the
   incident report needs the coordinator's confirmation.
2. **The atlas receipt script's backgrounding mechanism (Entry A).** The
   receipt script that hit the 60s cap cannot be inspected, because the dataflow
   atlas was scraped (commit `2c08ea62` removes `atlas.sh`). Whether the server
   escaped the process group via `nohup`/`setsid` detach, or the 60s outer cap
   was a bare `perl -e 'alarm N; exec'` orphaning one-liner rather than
   `run-capped.sh`'s `cap_self`, is unverified. I recorded both as live escape
   routes and flagged the fact as unverified. Confirmation needed on which.
3. **`cap_self`'s process-group kill vs this incident.** The repo's own
   receipts prove `cap_self` kills a direct-child backgrounded server on
   `files.sh` (class 38), so the entry frames the incident as an uncovered
   path (detached server / non-process-group cap), not as a failure of
   `cap_self` as written. If the coordinator knows the incident was `cap_self`
   against a server that was a direct child, that would change the RCA and the
   entry's mechanism paragraph.
4. **Fixture name (Entry B).** `coalesce-empty-group.dl6` is my proposed
   name, inferred from the kebab-case `.dl6` convention of
   `v6/dl/fixtures/` (extraction-live.dl6, files-hosts.dl6,
   served-endurance.dl6). The verdict recommends promoting the `und_internal_filled`
   shape "to conformance fixtures" without naming it. Confirm the name or
   supply the intended one.

## Notes

- No command in this run exceeded 10s.
- No commits made; working tree changes are limited to `docs/failure-modes.md`
  and this file.
