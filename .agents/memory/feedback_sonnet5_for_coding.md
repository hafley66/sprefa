---
name: feedback-sonnet5-for-coding
description: "Delegate coding tasks to well-advised Sonnet 5 subagents; the main session orchestrates, specs, and verifies"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 328dc2c5-5921-4237-9f45-23b13b1a7072
---

Chris: "use well advised sonnet5 for coding tasks" (2026-07-02).

**Why:** cost/speed; the main session's value is the spec and the verification, not typing the edit.

**How to apply:** for implementation edits, launch an Agent with `model: "sonnet"` and a prompt that carries exact file paths + line numbers, the precise change, repo style rules that apply, and the verification commands to run. Verify the result yourself after (build/tests/smoke). Trivial one-line edits in already-open files can stay inline.

**Partitioning (Chris, 2026-07-03, after a 37-min/360k-token sweep agent edited ZERO files):** "chill on ur tokens... use haiku or sonnet like a samurai sword". Rules: (1) cap a sweep agent at ~8-11 files with a DISJOINT file set; run partitions in parallel. (2) EDIT-FIRST — never prescribe per-file before/after engine runs or a baseline full-suite inside the agent; the agent that died ran the whole it-suite as a "before" baseline. Verification budget goes IN the prompt: "verify at most N light files, skip anything embedding-flavored"; the coordinator runs the full suite ONCE after all partitions land. (3) mechanical single-pattern buckets go to Haiku with exact before→after shapes inline; judgment buckets go to Sonnet. (4) tell agents which files NOT to read (no README surveys) — context bloat is the cost driver, not the edits. (5) GIT SAFETY (2026-07-03 incident): every agent prompt must forbid `git restore`/`checkout`/`stash`/`clean` outside the agent's named target files — a port-graph agent "cleaned up" the working tree and git-restored ~40 files of another arc's uncommitted sweep (unrecoverable via git; recovered via main-checkout copies + transcript replay + re-run). Also: in this repo a full `dl` one-shot run triggers doc-gen splices into README/docs — agents verify with `--check` only unless the task needs a live run, and then the prompt must say which dirt the run creates.
