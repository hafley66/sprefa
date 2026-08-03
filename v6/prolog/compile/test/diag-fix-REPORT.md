# dl6 diag channel: audit fix REPORT

## Base proof

`git merge --ff-only 8907a040` from the worktree root printed:

    Already up to date.

## What changed, per defect

### Defect 1 (wrong position; the important one)

`compile/parse_dl.pl` resolved a refusal to the FIRST recorded statement whose
term contained the refusing relation as a sub_term (`statement_reference/3`,
a `sub_term` scan). When an earlier valid rule merely mentioned the relation,
the diagnostic pointed at that rule instead of the one that defines it.

The resolution is now head-first (`statement_location_for_reference/4`'s new
first clause + `statement_head_reference/2`): a refusal names the OFFENDING
RULE, and that rule is the one that defines the reference, so the resolver
looks up the recorded statement by its HEAD identity before it falls back to
the sub-term scan (the fallback still covers a body-named reference that no
rule heads). The refusal already carried that identity (its head ref) through
`throw_text_door_error`; the resolver had not been told to use it.

No parsed term shape changes. No `lower.pl` or `emit_ts.pl` edits. The change
sits on the refusal path only (`statement_location_for_reason`, used by
`diag.pl` and by `throw_text_door_error`), so a successful compile pays
nothing.

Fail-first receipt, added as a decoy test
(`test(refusal_resolves_offending_rule_not_earlier_mention)`): the program

    rel counter(name: text, total: int) key(1).
    rel tick(name: text).
    rel mirror(name: text, total: int).

    mirror(Name, Total) <- counter(Name, Total).
    counter(Name, Total) <- tick(Name), Total = latest(1).

`counter/2` is named by the valid `mirror` rule (line 5) and by the offending
`counter` rule (line 6). The diagnostic must land on line 6. The prior test
passed only because its program had a single statement.

RED, on the pre-fix code:

    % [1/6] diag_channel:one_based_to_zero_based ........ passed (0.005 sec)
    % [2-1/6] diag_channel:json..e_equals_human_line ... passed (20.746 sec)
    % [3/6] diag_channel:reco..round_trips_as_json ...... passed (0.365 sec)
    % [4/6] diag_channel:refu..not_earlier_mention .... **FAILED (0.000 sec)
    ERROR: [Thread main] /Users/.../diag.test.pl:72:
    ERROR: [Thread main]     test diag_channel:refusal_resolves_offending_rule_not_earlier_mention: failed
    % [5/6] diag_channel:refu.._statement_position ...... passed (0.000 sec)
    % [6/6] diag_channel:pars.._is_exact_in_record ...... passed (0.000 sec)
    ERROR: [Thread main] 1 test failed

GREEN, after the resolver fix (this run is at the test's new home, and also
carries the Defect-3 uri test; `[4/7]` is the decoy now passing):

    % [1/7] diag_channel:one_based_to_zero_based ........ passed (0.004 sec)
    % [2-1/7] diag_channel:json..e_equals_human_line ... passed (20.499 sec)
    % [3/7] diag_channel:reco..round_trips_as_json ...... passed (0.350 sec)
    % [4/7] diag_channel:refu..not_earlier_mention ...... passed (0.001 sec)
    % [5/7] diag_channel:refu.._statement_position ...... passed (0.000 sec)
    % [6/7] diag_channel:pars.._is_exact_in_record ...... passed (0.000 sec)
    % [7/7] diag_channel:uri_..encoded_file_scheme ...... passed (0.000 sec)

End-to-end confirm on a real compile, both the human line and the JSON record
now name line 6 (the JSON `line: 5` is the zero-based form of 1-based 6):

    ERROR: ...:6: unsupported_construct: compiler refused rule 'keyed_level_head' for rel 'counter/2'
    {"code":"keyed_level_head/1",...,"range": {"start": {"character":0,"line":5},...},"uri":"file:///tmp/my%20file/repro.dl6"}

### Defect 2 (lab coupling)

`diag.pl` was a compiler module parked under `labs/diag_channel/`, imported by
`compile.pl:35` as `use_module('labs/diag_channel/diag', ...)`. Under the lab
protocol the directory dies on landing, which would have stopped the compiler
from loading.

- Moved `v6/prolog/labs/diag_channel/diag.pl` to `v6/prolog/diag.pl`, beside
  `0_refusal_messages.pl`. Its internal imports changed from
  `'../../0_refusal_messages'` / `'../../compile/parse_dl'` to
  `'0_refusal_messages'` / `'compile/parse_dl'`.
- Moved its test to `v6/prolog/compile/test/diag.test.pl`, beside the other
  compiler tests, and re-pointed `plunit_tests.pl`'s `ensure_loaded` to
  `'diag.test.pl'` (same directory).
- Updated every reference: `compile.pl` (`'diag'`), `parse_dl.pl`'s export
  comment, and `diag.test.pl`'s own imports (`'../../diag'`, `'../parse_dl'`).

Moved only those two files; the remaining `labs/diag_channel/CONTRACT.md` was
left in place and my scratch repro file was removed.

### Defect 3 (bare-path `uri`)

`diag_uri/2` emitted the raw filesystem path. It now percent-encodes through
SWI's `library(uri):uri_encoded(path, ..., EncodedPath)` and prepends
`file://`, so spaces and non-ASCII are escaped rather than concatenated raw
(also `library(uri)` added to the module's imports). Covered by
`test(uri_is_percent_encoded_file_scheme)`:

    diag_uri(at('/tmp/my résumé/notes file.dl6', 3, _), Uri) == "file:///tmp/my%20r%C3%A9sum%C3%A9/notes%20file.dl6"

Verified end to end above: `"uri":"file:///tmp/my%20file/repro.dl6"`.

## Gate output

- `cd v6/prolog && swipl -g go -t halt ARCH.pl` → `PASS` (exit 0).
- `cd v6/prolog && just plunit` → `283/283` passed (baseline 281; the +2 are
  the new decoy and uri tests; all diag_channel tests ran inside the gate).
- `cd v6/prolog && just text-door` → `TEXT_DOOR compiled=196 byte_identical=196 failures=0` (exit 0).
- `cd v6/prolog && just conformance` → `281` PASS / `0` fail (exit 0).
- `cd v6/prolog && just compile-speed` → `COMPILE_SPEED programs=4 phases=24 regressions=0 improvements=0 OK` (exit 0).

`just green-all` was not run: it needs node modules a fresh worktree lacks,
which is a known environment artifact and not a defect.

## What I could not do

- The file the task pointed at for the audit's own account,
  `v6/prolog/labs/diag_channel/AUDIT/REPORT.md`, does not exist in this
  worktree. I worked from the defect descriptions in the task itself; the
  `labs/diag_channel/` directory on landing contained only `CONTRACT.md`,
  `diag.pl`, and `diag.test.pl`, with no `AUDIT/` subdirectory.
- I did NOT carry the offending rule as a changed refusal-term argument (e.g.
  threading the rule term through the thrown `unsupported_construct(...)`
  payload). Those payloads (`keyed_level_head/1`, `latest_in_level_rule/1`,
  and the rest) are pinned byte-for-byte by conformance fixtures and plunit
  `throws/1` matches, and the gate rule forbids fixing a moved gate by
  adjusting a test. Instead the resolver uses the rule identity the refusal
  already names (the relation it refers to) to select the defining statement,
  falling back to the sub-term scan when no rule heads the reference. For a
  body-named reference that happens to also be some other rule's head, this
  could still pick the defining rule over the body mention; that residual is
  the same classifier the pre-existing single-statement tests exercise and I
  did not add per-refusal-type position logic.
- I changed no parsed term shapes, no `lower.pl`, and no `emit_ts.pl`; byte
  identity is preserved (`byte_identical=196`).
