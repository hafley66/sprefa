---
created: 2026-08-23
updated: 2026-08-23
type: chore
reporter: hafley66
status: open
priority: normal
labels: [docs]
---

# ARCH.pl spring cleaning: history out to plans/, rows under 600 chars, rail against regrowth

_Source: v6/prolog/ARCH.pl_

## Description

# arch-spring-cleaning

`v6/prolog/ARCH.pl` is 1,020 lines, 263 `task/3` rows, 168 done; 104 rows are over 800 characters because every landing pasted its whole report into the row comment. CLAUDE.md: "narrative history: plans/, chat_log/, git. Never this file." Make ARCH.pl the roadmap again.

Base: `git merge --ff-only <sha the coordinator states>` first; fail = stop and hail. Branch `chore/arch-spring-cleaning`. PR to main.

## Rules, mechanical, no judgement calls
1. Before touching anything: `cd v6/prolog && swipl -g go -t halt ARCH.pl` must print 7 PASS. Record the count of `task(`, `fork(`, `check(`, `construct(`, `sugar(`, `algorithm(`, `graph(`, `covers(` rows. None of these counts changes. No status changes. No dependency-list changes. No row deleted, renamed, or reordered.
2. Every `task(Name, done, Deps). % <comment>` with a comment over 240 characters: the comment becomes ONE line, under 240 characters: `LANDED <date> (<PR or sha>): <one sentence of what it is now>`. The full original comment moves VERBATIM to `plans/2026-08-23-arch-history.md` under a `## <Name>` heading, in file order, with the line it came from. Nothing is summarized away; the history file is append-only from now on.
3. Rows with status unbuilt / labbed / labbing / active / parked / closed / superseded / canonical: keep the comment, but if it is over 600 characters move everything after the first two sentences to the history file the same way, and leave `(history: plans/2026-08-23-arch-history.md#<name>)` at the end of the row.
4. The header callouts (top of file, before the first fact) stay as they are unless a callout names a thing that no longer exists (grep each named file/predicate; list removals in the PR body with the grep that proved it). Do not rewrite prose you did not remove.
5. After: `swipl -g go -t halt ARCH.pl` 7 PASS, the row counts identical, `wc -l` and `wc -c` before and after in the PR body, the longest row length before and after, `awk 'length > 800' ARCH.pl | wc -l` = 0.
6. A plunit-style rail so it cannot regrow: add `check(task_rows_short, forall(task_row_comment(_, C), string_length(C) =< 600))` or the nearest shape the file's own `check/2` form supports; read `run(check)` first. If the file has no way to read its own comments (it is Prolog source; comments are invisible to it), do NOT invent a parser: add a line to `v6/justfile`'s `arch` recipe (or a `scripts/` one-liner called from it) that fails when any line exceeds 600 characters, and name it in the PR.

## You own
`v6/prolog/ARCH.pl`, `plans/2026-08-23-arch-history.md` (new), `v6/justfile` (one recipe line), `v6/prolog/conformance/rulings.pl` NOT (forbidden), `CLAUDE.md` NOT (forbidden). Nothing else.

## Gates
`cd v6/prolog && swipl -g go -t halt ARCH.pl` 7/0 before and after. `git diff --stat` in the PR body. Commit in two steps: (a) history file + trimmed done rows, (b) open rows + rail. PUSH before you report; a result with nothing pushed is not a result.

## Style laws
No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). The one-line summaries are plain; no "successfully", no adjectives.

Done: `boop beep hail sprefa-coordinator --from arch-spring-cleaning --body "PR #<n>: lines before->after, longest row before->after, go 7/0"`; if refused, message the sprefa-* session over the cross-session socket. Blocked: one line, stop.
