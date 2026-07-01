# Piloting haiku from dl: the model as a cached per-file UDF

Date: 2026-06-11. Design only; nothing here is built. Companion to
plans/2026-06-11-anim-and-ports.md.

## The fit

`cmd(path, rev, "tool {file}", line, out)` already has exactly the contract a
model call needs:

- **per-file scope**: the model sees one file and one instruction, never the repo
- **cache key (file hash, rule text)**: a haiku call reruns only when the file
  content OR the prompt changes — billing-correct staleness for free, since the
  prompt is part of the rule text
- **one row per stdout line**: model output lands as relation rows, not prose
- **exit contract**: nonzero+stdout = findings, nonzero+empty = broken rail
- **`--check`/`--lsp` never run gen**, and cmd's cache means enforcement ticks
  never trigger uncached model calls if a normal tick ran first

So the shape is: dl decides WHERE to point the model (scan + joins + filters),
haiku does one narrow judgment per file, dl validates the output relationally
before anything acts on it.

## Patterns, cheapest first

### 1. Labeler (read-only)
    rel label(p: file, out: text).
    label(p, out) <- scan("WORK", "src/**/*.rs", p, rev),
      cmd(p, rev, "claude -p --model claude-haiku-4-5-20251001 'One word: is this file mostly PARSING, IO, or GLUE?' < {file}", line, out).

Rows join everything else: `gnarly(p) <- label(p, "PARSING"), fan_out(...)`.

### 2. Extractor with rails
Prompt demands TSV (`name<TAB>verdict`). Rails are plain rules:

    rel bad(p: file, out: text).
    bad(p, out) <- raw(p, out), out !~ "^[a-z_]+\t(ok|sus)$".
    diag("error", p, 0, "haiku emitted junk: ${out}") <- bad(p, out).

Malformed output → exit 2 → the tick refuses to act on it. The model is
allowed to be wrong; it is never allowed to be wrong silently.

### 3. Actuator loop (the blade)
- a derived rel picks targets (hot files, TODO regions via `comment`)
- gen file-form writes the per-target prompt/context file under the root
- next tick, cmd feeds that file to haiku; output rows pass rails
- a gen SPLICE writes accepted rows between markers in real files
- convergence: same content → no write → no re-extract → fixpoint

Every actuation is bracketed by markers, so `git diff` shows exactly the
machine-written lines, and deleting the region + rerunning regenerates it.

### 4. Steering
- aim: scan glob + joins choose targets; the model never picks its own scope
- throttle: thresholds (`n >= 10`) cap call count; count the rows first with a
  `?` query before enabling the cmd rule
- redirect: edit the prompt string → cache invalidates for all files → full
  re-judgment; edit nothing → zero calls
- kill: `--check` mode is always write-free and call-free-after-warm

## Gaps to close before this is pleasant

1. **Shell quoting**: prompts with quotes need a `{file}`-style placeholder
   escape (`{var:sh}`) or cmd should exec argv-style instead of `sh -c`.
2. **Multi-line model output as one value**: cmd is line-grained; a `cmd_all`
   variant binding whole stdout as one value would suit prose outputs.
3. **Timeout/cost ceiling per tick**: a `--cmd-budget N` rail (max uncached
   cmd executions per tick) so a glob typo can't fan out 2k API calls.
4. **stderr capture**: API errors currently vanish unless exit is nonzero.

## Why haiku specifically

The judgments are narrow (one file, one question, enumerable answers), the
volume is high (hundreds of files), and the rails catch junk. Cheap + fast +
relationally validated beats smart + slow here. Escalation path: a second cmd
rule with a bigger model that runs ONLY on rows the rails flagged — the
disagreement set is tiny by construction.
