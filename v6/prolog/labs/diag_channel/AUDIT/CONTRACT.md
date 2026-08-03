# Audit lane: the Prolog work

## What this is

Two lanes produced Prolog this session: the dl6 diagnostic channel (in this
worktree, commit 8907a040) and a Tarjan extraction from clpfd (at
`/Users/chrishafley/projects/sprefa-lanes/swi-scc`, commit a315deba, read-only to
you). Their reports say the work is good and the coordinator verified a lot of it.

Assume more is wrong. Find it by RUNNING things. You are auditing, not fixing.

## Base

First action, from the worktree root:

    git merge --ff-only 8907a040

Expected: `Already up to date.` Anything else: STOP, write REPORT.md, do not work
around it.

## What you own

`v6/prolog/labs/diag_channel/AUDIT/**` and `REPORT.md` at the worktree root.

You may READ anything. You will temporarily mutate files for sabotage tests and
you MUST restore them byte-for-byte. Back up with a recorded checksum before every
mutation, restore, re-check the checksum, and finish with `git status --short`
showing nothing dirty but your own files. A sabotage you forget to revert is the
worst outcome of this lane, worse than finding nothing.

Do NOT edit anything in the swi-scc worktree. Read and run it in place.

## Already-confirmed findings, seeded so you do not spend time rediscovering them

Verify each is still true, then go past them:

1. **`compile.pl` hard-depends on a lab directory.** It carries
   `:- use_module('labs/diag_channel/diag', [emit_diag_file/2])`. The repo's lab
   protocol (CLAUDE.md, "Labs die on landing") deletes lab files on landing, which
   would make the compiler fail to load. Confirm the breakage concretely: move the
   lab directory aside, show the compiler failing, move it back.
2. **The `uri` field is a bare filesystem path**, not a `file://` scheme URI as LSP
   requires. Confirm, and check whether anything else in the record shape departs
   from the LSP `Diagnostic` interface.

## The audit protocol

### 1. Sabotage every test

For each plunit suite in scope (`labs/diag_channel/diag.test.pl`, and
`v6/prolog/labs/scc_extract/scc.test.pl` in the other worktree), introduce ONE
deliberate defect in the code under test, run, confirm red AND nonzero exit,
restore. Table it: suite, defect, tests red, exit code.

Known and to be re-verified: removing the outer `sort` in `scc.pl` turns 3 tests
red with exit 1.

Specifically try to break the diag channel in ways its own tests might not catch:
- make `lsp_position/4` off by one in the other direction
- make the JSON `message` field differ from the human line
- make a resolvable position silently fall back

If any of those stays green, that is the headline finding.

### 2. Attack the one-source-two-renderers claim

The lane claims human text and JSON `message` cannot diverge because both come
from `message_to_string/2` on the same term, checked across the whole inventory by
`json_message_equals_human_line`.

Try to break that property. Can a diagnostic reach the channel without going
through the umbrella renderer? Is there any path where the channel constructs
text itself? Does the inventory walk actually cover every signature that can be
emitted, or only those `refusal_inventory/1` happens to have loaded?

### 3. Check what a SUCCESSFUL compile pays

The lane claims "a successful compile never reaches" the emitter. Verify by
measurement, not by reading: compare the compile-speed numbers and the parse-phase
inference count on a clean program with and without the change. `just
compile-speed` from `v6/prolog` is the existing rail; the expected line is
`programs=4 phases=24 regressions=0 improvements=0 OK`.

### 4. Check the position claim honestly

The report says 58 of 73 signatures resolve a real position BY MECHANISM, with
only five plus the parse-error class runtime-confirmed. Trigger more of them.
Write `.dl6` programs that provoke refusals from the resolvable set and check
whether the emitted line and column actually point at the offending statement,
not merely at some statement. Report how many you triggered and how many pointed
correctly.

A position that resolves to the WRONG statement is far worse than one that falls
back, because a fallback is visibly a fallback and a wrong position is a lie the
editor will draw on screen. Hunt specifically for that.

### 5. Couplings and layering

Beyond the seeded finding: does `diag.pl` reach into modules it should not?
Does the wiring in `compile.pl` change control flow on any path other than the
refusal path? Does the channel hold state across compiles (an open stream, a
sticky file handle, an assert) that would leak between runs or between threads?
Check what happens when `DL6_DIAG_JSONL` names an unwritable path.

### 6. The scc_extract lane

Verify independently: the extraction really is verbatim against
`/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/library/clp/clpfd.pl` lines
5892-5962; the 11-shape agreement with `graph_components/2`; and the claimed
360-graph fuzz. Re-run the fuzz yourself with a different seed and report whether
any disagreement appears.

Its verdict is that the extraction is 21x slower and not worth using. Check
whether its stated cause (an O(V^2) identity scan in the wrapper) is really the
cause, by measuring at two sizes.

## What you must NOT do

- Do not fix anything. Report.
- Do not run `just green-all`; it needs node modules a fresh worktree lacks. The
  Prolog legs (`just conformance`, `just plunit`, `just text-door`,
  `just compile-speed`, `just arch` from `v6/prolog`) are what you use.
- Do not spawn subagents, create worktrees, commit, or push.

## Style laws

- No em dashes anywhere.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base layer, critical, mode.
- Tables and file:line over prose. No claim without a command behind it.

## REPORT.md format

    # prolog audit: REPORT
    ## Base proof
    ## Restore proof
    ## 1. Sabotage table
    ## 2. The two-renderer property under attack
    ## 3. Cost on the success path
    ## 4. Position correctness
    <how many triggered, how many pointed at the right statement, any WRONG ones>
    ## 5. Couplings, state, and failure modes
    ## 6. scc_extract independent verification
    ## Ranked findings
    ## What I could not check
