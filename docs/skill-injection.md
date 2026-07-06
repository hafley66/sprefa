# Harness hooks: `dl --hook` (force-load a skill, inject context, block)

Sibling to [rails.md](rails.md). A rail **blocks** via `--check` (exit 2 →
stderr → the model self-corrects). `--hook` is the other emit mode: dl reads a
coding-agent's hook event on stdin, ticks the rules, and **injects context** so a
skill's instructions load whether or not the model chooses to call the Skill
tool. dl is the binary the harness execs — no editor, no bash.

## The mechanism (and its one hard limit)

A hook **cannot invoke the Skill tool** and cannot make the model call it. So
`--hook` injects the skill's **body** as `additionalContext`: the instructions
are in context with no choice left to get wrong. Stronger than forcing a call.

LSP is not an alternative here. It only paints editor squiggles, has no
agent-tool-use event, and reaches Claude Code only through an open IDE. The hook
is the only editor-independent channel that fires on a Read/Edit and can inject.

## Emit relations

The program heads any of these (all single-column) over the agent built-ins
(`agent_touch`, `agent_changed`, `changed`, `module_edge`, ...):

| relation | effect |
|---|---|
| `inject(text)` | text appended to `additionalContext` |
| `inject_skill(name)` | resolve `.claude/skills/<name>/SKILL.md`, inject its body |
| `block(reason)` | emit `{"decision":"block","reason":…}` (short-circuits inject) |

The condition is the rule body. That is the whole feature: programmable hook
conditions in dl.

## Load once (declarative)

The guard is a negated atom, not a flag. `skill_loaded(harness, session, name)`
is a built-in relation derived from the transcript — explicit `Skill` tool calls
plus dl's own prior `dl --hook` injections (the `additionalContext` marker is
recorded verbatim). Negate it and the rule is idempotent:

```
inject_skill("testing") <- agent_touch(_, s, p), <test predicate>, !skill_loaded(_, s, "testing").
```

No state files; the dedup is a fact in the engine, refreshed each tick alongside
`agent_touch`.

## Setup

Install dl on PATH (`cargo install --path . --bin dl`), drop your rule at
`<repo>/.dl/` (or pass a path), and register the hook in
`<repo>/.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Read|Edit|Write|MultiEdit",
        "hooks": [ { "type": "command", "command": "dl --hook" } ]
      }
    ]
  }
}
```

That JSON is registration, not glue — zero logic. `dl --hook` reads the event
(`session_id`, `tool_name`, `tool_input.file_path`), ticks, emits the hook JSON.
With no program argument it discovers `<root>/.dl/*.dl`, same as `--check`.

## Example

[`examples/hook-skill-on-test.dl`](../examples/hook-skill-on-test.dl): inject the
`testing` skill when the agent touches a test file this turn.

```
rel inject_skill(name: text).

inject_skill("testing") <-
    agent_touch(_, _, p),
    p =~ /(_test\.|\.test\.|\.spec\.|test_|\/tests\/)/.
```

Swap `agent_touch` for `agent_changed`, join `changed`/`module_edge`/`type_edge`
— the condition is yours.

## Exit codes

| code | meaning |
|---|---|
| 0 | normal: emitted inject/block JSON, or silent (condition didn't fire). `block` rides the JSON, not the exit code |
| 1 | the rule program itself is broken (parse/type error) — surfaced to the user via stderr, never fed to the agent |

## Other harnesses

The stdin-JSON → stdout-JSON contract is Claude Code's native shape, so `dl
--hook` drops in with no glue. opencode registers hooks as a TS plugin, so it
needs a ~5-line plugin that shells `dl --hook` and maps the result — still zero
logic. A second harness = a second render arm in `src/hook.rs`, never a change to
any dl program.

## Implementation

All in [`src/hook.rs`](../src/hook.rs) (the event parse, skill resolution,
load-once dedup, emit), plus `agent::cc_skill_loads` for the transcript read.
Tests: `tests/it/hook_inject.rs`.
