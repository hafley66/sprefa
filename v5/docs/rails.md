# Agent rails: dl as a guardrail loop for coding agents

A rail is a `diag` rule scoped to the worktree diff. The agent edits, a hook
runs `dl --check`, violations come back as text, the agent fixes them. The
repo's pre-existing debt never fires because every rail joins `changed(p)`.

## Setup

1. Put rails in `<repo>/.dl/*.dl` (any file count; they merge in filename
   order, shared `rel` declarations dedupe). Start from `examples/rails.dl`.
2. No positional argument means discovery: `dl --check --root <repo>` finds
   `.dl/*.dl`, caches in `.dl/cache.db` (gitignored automatically), and ticks
   incrementally on repeat runs.

## Exit-code contract

| code | meaning | who sees stderr |
|---|---|---|
| 0 | clean (warn-severity rows do not fail) | nobody |
| 2 | error-severity diag rows | the agent (Claude Code blocking-hook code) |
| 1 | broken rails program (parse/type/decl error) | the user |

The 1/2 split is the point: a bug in the rails reads as "fix the rails", never
as feedback the agent should act on.

## Claude Code hook

`.claude/settings.json` in the repo:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "dl --check --root \"$CLAUDE_PROJECT_DIR\""
          }
        ]
      }
    ]
  }
}
```

`dl` must be on PATH (or use an absolute path to the binary). On exit 2,
Claude Code feeds stderr back to the model and it self-corrects; exit 0 is
silent; exit 1 surfaces to you only.

The same command works as a pre-commit hook or CI step unchanged.

## Writing a rail

Source rules (`scan` + `match`/`ast`/`sg`) extract per file and cannot join
relations, so a rail is two rules: extract, then join `changed`:

```
rel diag(path: text, line: int, severity: text, code: text, msg: text).

rel todo_hit(p: file, l: int).
todo_hit(p, l) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /TODO/, l).

diag(p, l, "error", "no-todo", "TODO in a changed file") <-
    todo_hit(p, l), changed(p).
```

`changed(p)` holds every path `git status --porcelain -uall` reports against
HEAD (modified, added, renamed, untracked), re-anchored to `--root`. Outside a
git repo it is empty, so rails degrade to silent. Committing clears it: rails
go quiet on the next tick.

Severity is convention: `"error"` blocks (exit 2), anything else reports
without failing. Exemptions are joins too — see the fenced `fs:` literal in
`examples/rails.dl`, which makes a typo'd exemption path a check error instead
of a silently never-matching string.

## Known grain limits

- `changed` is file-grain: a rail counting hits in a changed file counts the
  whole file, not just added lines.
- Multi-file type diags attribute to `.dl/*.dl`, not the specific file.
