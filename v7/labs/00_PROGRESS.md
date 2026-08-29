# V7 Common Lisp logic lab progress

Updated: 2026-08-29 11:47 EDT

## Current state

- Native-logic shootout: SBCL 2.6.7 host data structures, SWI-Prolog 10.0.2
  tabling, and Racket CS 9.3 `datalog` evaluation on deterministic chain and
  ring transitive closures. `N=48` completed one warmup and five measured
  repetitions in 40 seconds with exact closure counts of 1,128 and 2,304.
- Shared skill commit: `932abe9` in `claude-research`.
- Lab scaffold commit: `98f991dbd` in `sprefa`.
- Installed runtime: SBCL 2.6.7.
- GLM shared worktree: `.boop-worktrees/chore/v7-cl-logic-glm`.
- Terra shared worktree: `.boop-worktrees/chore/v7-cl-logic-terra`.
- Completed lab reports: 12 (`1_inventory`, `2_cl_gambol`, `3_paiprolog`,
  `4_cl_datalog`, `5_cl_grph`, `6_screamer`, `7_reazon_cl`, `8_cl_kanren`,
  `9_vivace_graph`, `10_wamcompiler`, `11_cl_prolog2`,
  `12_handwritten_logic`).
- Active lab workers: 2 (`13_racket_crosswalk`, `14_binary_packaging`) using
  native Terra-high workers in the shared lab worktree.

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
- `reazon-cl` commit on main: `40a13c320`. Native behavior covers first-order
  unification with a default occurs check, productive stream interleaving,
  bidirectional append, and ordered answers. Cyclic paths, facts, updates, and
  bounded negation use adapters.
- `reazon-cl` image: 44,309,032 bytes, SHA-256
  `438681574716742538ee8ae756a439d23f992c6a03e470c09ccd1ee803a8ed6f`.
  Its emitted probe SHA matches the committed `2_PROBE.lisp`. The runtime
  verifies the clean upstream pin and the Trivia archive and ASDF source.
- `cl-kanren` commit on main: `de6ff57cd`. Native behavior covers generic
  first-order unification with an occurs check and lazy `mplus` interleaving.
  The cyclic fixture needs an answer cap or wall bound; Datalog saturation,
  dynamic facts, and negation are adapters.
- `cl-kanren` image: 42,932,568 bytes, SHA-256
  `b6d24a321b3ccba51ee74373f7bac46ca0970ddeb8187c31437cec8d16e71aea`.
  Its toplevel starts 13 external SBCL child sections and propagates child
  failure as exit code 1.
- Two Luna-high review passes reconciled provenance, dependency hashes,
  fairness receipts, truth labels, upstream history, child counts, external
  runtime files, and final image receipts for labs 7 and 8.
- `vivace-graph` commit on main: `20b870074`. Native behavior covers compiled
  first-order unification with an occurs check, graph transactions, durable
  indexes, duplicate Prolog proofs, persistence, and retraction. Cyclic
  closure uses a persistent-node-ID visited-set adapter.
- `vivace-graph` image: 65,873,672 bytes, SHA-256
  `a21045518382ad210bd48e76761ad0a08b3edc427b677ce8b348d5e0beadf7b6`.
  Runtime requires the pinned checkout for commit verification and a fresh
  graph directory; Quicklisp is not loaded at image startup.
- `WAMCompiler` commit on main: `800a4284c`. Native behavior covers WAM
  unification, trail-based depth-first backtracking, cut, lists, negation,
  clause addition, and first-argument dispatch. It has no occurs check,
  tabling, finite cyclic closure, or retraction API.
- `WAMCompiler` image: 40,638,456 bytes, SHA-256
  `b9c1e669c87f3288010de75f07ddd07960619272bcd33058c4eea48675629df4`.
  Each isolated image probe section runs in a child copy of the saved image.
- Luna-high review found 12 concrete issues across labs 9 and 10. Final fixes
  reconciled saved-image execution, runtime provenance wording, persistent
  visited keys, dynamic clause receipts, transcript labels, and measurements.
- `cl-prolog2` commit on main: `f8c1d18f2`. It emits Prolog source to a
  temporary file and launches `swipl`; SWI supplies unification, tabling,
  cyclic completion, negation, and retraction. The bridge returns one stdout
  string and has no typed query lifecycle or in-process engine.
- `cl-prolog2` image: 45,554,408 bytes, SHA-256
  `8d4af37c3a891b73ee969a81de8613917d9a058c24e05a46c93700eed2dac53e`.
  Full fixture startup samples are 0.11 seconds; peak RSS is 52,690,944 bytes.
- The handwritten kernel commit on main is `e7cc3723c`. Its 158
  nonblank/noncomment lines implement persistent substitutions, an occurs
  check, lazy fair disjunction and conjunction, bounded reification, facts,
  and Horn-style rules.
- Handwritten-kernel image: 42,080,472 bytes, SHA-256
  `be4fc038ee5f3af2e476684d491b717401ed6d550fff69e54acfe923b23c661c`.
  Startup samples are 0.01 seconds; peak RSS is 46,743,552 bytes.
- Luna-high reviews reconciled the lab 11 deterministic answer order, r2
  traces, pin checks, benchmark command, external dependencies, and source
  versus image receipts. Lab 12 required one source/runtime dependency wording
  correction.

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
registered as `v7-reazon-glm` and `v7-cl-kanren-glm`. Five stale ACPX queue
owners and the old cl-datalog coordinator were terminated after they blocked
the next pair. Managed approval rejected OpenRouter transmission of repository
contents, so labs 9 and 10 use native Terra-high workers instead.

## Next execution sequence

1. Run `13_racket_crosswalk` and `14_binary_packaging` in parallel.
2. Review, measure, and commit the accepted pair on the shared branch.
3. Cherry-pick the pair to main and update this progress log.
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
