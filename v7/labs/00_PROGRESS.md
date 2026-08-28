# V7 Common Lisp logic lab progress

Updated: 2026-08-28 16:22 EDT

## Current state

- Shared skill commit: `932abe9` in `claude-research`.
- Lab scaffold commit: `98f991dbd` in `sprefa`.
- Installed runtime: SBCL 2.6.7.
- GLM shared worktree: `.boop-worktrees/chore/v7-cl-logic-glm`.
- Terra shared worktree: `.boop-worktrees/chore/v7-cl-logic-terra`.
- Completed lab reports: 6 (`1_inventory`, `2_cl_gambol`, `3_paiprolog`,
  `4_cl_datalog`, `5_cl_grph`, `6_screamer`).
- Active lab workers: 2 (`7_reazon_cl`, `8_cl_kanren`).

## Completed labs

- Inventory commits on main: `171b3922c`, `50fc699a1`.
- Inventory result: 17 repositories, 14 families, 12 runnable systems.
- Inventory added `16_logadat` and `17_si_kanren`.
- `cl-gambol` probe covers nested unification, missing occurs check, DFS answer
  order, DFS starvation, cyclic recursion, fact updates, an external fixpoint
  sketch, and standalone-image measurement.
- `cl-gambol` image: 40,179,640 bytes. Generated executable remains outside
  Git.
- Luna review blocked the first draft. The corrected probe prints both
  unification bindings, caps PATH at exactly 100 answers, preserves ORDER,
  demonstrates starvation, and prints the required BINARY record.
- `cl-datalog` commit on main: `2f32e7fe9`. The pinned upstream is a package
  stub with zero authored functions or macros and no evaluator. Its SBCL image
  measures 42,342,656 bytes.
- `paiprolog` commit on main: `a56f980ad`. The probe covers the interpreter
  and compiled unifiers, DFS order and starvation, cyclic closure with a depth
  adapter, cut spelling, clause replacement and retraction, raw unification
  trace, and standalone-image measurement.
- `paiprolog` image: 40,769,552 bytes, SHA-256
  `3b60739f1ca822c7f97ded738c3f2943e7be33b935e55a91f64ae74e2f0525ab`.
  The retained image is outside Git at
  `/private/tmp/sprefa-v7-paiprolog-lab-012d6bb-20260828`.
- The final Paiprolog Luna verification returned PASS after the commit pin,
  cyclic harness, trace provenance, capability claims, and measurements were
  made executable and internally consistent.
- `cl-grph` commit on main: `8df12abea`. Its restricted linear-rule fixpoint
  terminates on the cyclic graph and emits 12 closure pairs. Compiler symbols
  require an integer ID dictionary because vertices are signed 32-bit values.
- `cl-grph` image: 55,582,944 bytes, SHA-256
  `c3693384ea6456f0df133bed6010ffb9df091aa7506322d6ba8fa0194025a676`.
  The retained image is outside Git at
  `/private/tmp/sprefa-v7-cl-grph-lab-d9d5edd-20260828-r3`.
- Screamer commit on main: `2990c8ff7`. Native behavior covers ordered
  nondeterminism and finite-domain constraints. First-order unification,
  Horn facts, bounded cyclic paths, and finite closed-world negation are lab
  adapters. Its starvation probe confirms depth-first search.
- Screamer image: 43,653,576 bytes, SHA-256
  `b33aab147a35c01424cea8fb248eff37eb9125ecc2e2deecc94e51d8cecad5e1`.
  The retained image is outside Git at
  `/private/tmp/sprefa-v7-screamer-lab-ce50614-20260828-r4`.
- Final Luna reviews returned PASS for both labs after package provenance,
  external build paths, raw receipts, append, negation, and capability
  vocabulary were reconciled.

## Coordination receipt

The first two GLM 5.3 Flash coordinators initially hit ACPX status 5 because
the coordinator command omitted an explicit writable non-interactive policy.
The Boop fix landed in `hafley-rs` as `ca26b2b` and the installed binary reports
`boop 0.0.9 (ca26b2b-dirty)`.

The first coordinators produced both lab folders but emitted no Boop result
hail and no projected assistant transcript. Filesystem artifacts were reviewed
directly before commit. The second pair used the corrected ACPX policy and
registered as `v7-paiprolog-glm` and `v7-cl-datalog-glm`. The third pair is
registered as `v7-cl-grph-glm` and `v7-screamer-glm`. Those persistent sessions
were closed after one queued correction wrote late into the shared worktree.
Stable files were then re-run and reviewed before commit. The fourth pair is
registered as `v7-reazon-glm` and `v7-cl-kanren-glm`.

## Next execution sequence

1. Review `7_reazon_cl` and `8_cl_kanren` when their completion hails arrive.
2. Commit the accepted pair on the GLM branch and cherry-pick it to main.
3. Run bounded Terra review on the accepted pair.
4. Repeat in pairs through the runnable library labs before starting binary
   packaging.

## Shared-worktree laws

- One worker owns one numbered lab folder.
- Workers do not commit.
- The coordinator reviews and commits accepted folders in bounded pairs.
- Inventory alone may add a new numbered candidate folder and update
  `0_INDEX.md`.
- Downloaded dependencies and project-local Quicklisp state do not enter Git.
- Every recursive probe has a finite domain, answer limit, timeout, or a
  combination of those bounds.
