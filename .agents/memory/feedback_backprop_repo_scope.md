---
name: feedback_backprop_repo_scope
description: backprop must not write repo-specific learnings into the global skills dir — repo knowledge stays in the repo
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1e06c249-1ca4-4665-98b0-d38b5054124a
---

Chris, 2026-07-02, after a backprop run appended sprefa perf-arc learnings to
~/projects/claude-research/skills/sprefa-dl and sprefa-v5-new-builtin-rel:
"backprop needs to chill, if its repo specific then it should stay in the repo."
The appends were reverted the same day.

**Why:** repo-specific facts already have in-repo homes (CLAUDE.md ledger,
CHANGELOG, source doc comments, example headers, docs/reference regen,
chat_log) and duplicating them in a global skills dir creates a second,
drifting copy outside the repo's review/versioning.

**How to apply:** before launching backprop, split the learnings: only
cross-repo / tool-general knowledge goes to global skills; anything scoped to
one repo lands in that repo's own files (which the normal commit already
covers). When in doubt, skip the backprop and say so.

Follow-through (same day, sprefa 17e4959): the sprefa-* skills themselves
moved INTO the repo — source of truth is `assets/*.skill.md` (tracked; the
sprefa-dl one carries the gen-skill-ref op-quickref rail), exposed via
gitignored `.claude/skills/<name>/SKILL.md` symlinks. The claude-research
copies are deleted (d0e7479 there). Editing a sprefa skill = edit
`assets/<name>.skill.md` in the sprefa repo, never a global skills dir.
Residual: a fresh clone lacks the symlinks (`dl setup` doesn't create the
three maintainer ones yet).
