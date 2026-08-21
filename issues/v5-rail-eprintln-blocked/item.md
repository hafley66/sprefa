---
created: 2026-08-21
updated: 2026-08-21
type: bug
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

`.dl/no-new-eprintln.dl` is named in CLAUDE.md as a live rail. It cannot be
ported to dl6 today. Blocked by @dl6-no-text-extraction-door.

## What the rail asks, and where each half stops

| v5 line | construct | v6 stop |
|---|---|---|
| `.dl/no-new-eprintln.dl:25` | `match_line(f, rev, /eprintln!/, line)` | no text plane on the wire |
| `.dl/no-new-eprintln.dl:32` | `comment(f, rev, /@eprintln-ok:/, line)` | comment TEXT is not on the wire |
| `.dl/no-new-eprintln.dl:42` | `match_line(f, rev, /\/\/.*@eprintln-ok:/, line)` | no text plane |

## Probe, 2026-08-21

A rust file holding `eprintln!("x")` and `println!("y")`, through the release
extractor:

| family | what came back |
|---|---|
| `call` | two `node` records for the enclosing fns. **No `site` record**: the rust front-end does not project macro invocations as call sites |
| `cst` | `{"kind":"macro_invocation","name":null}` and `{"kind":"identifier","name":null}` — the span is there, the identifier TEXT is not |
| `df` | zero rows |
| `data` | zero rows |
| `cfg` | entry/exit/stmt nodes, all `name:null` |

So no v6 record carries the fact "this line calls `eprintln!`".

## Two ways to unblock it

1. **@dl6-no-text-extraction-door arm A.** Link `--ast-pattern` in-process. The
   rail then reads `record=capture` from an `eprintln!($$$ARGS)` pattern, and
   the waiver from a second pattern over comment text.
2. **Project macro invocations as call sites** in `RustSource`. `eprintln` would
   arrive as `record=site callee="eprintln"` and the rail's first half becomes
   ordinary. This is a bigger semantic call (is a macro a call?) and belongs
   with Chris, not a lane.

Arm 1 answers the waiver half too and needs no semantic decision.

## Meanwhile, the number the rail exists to keep at zero

Measured 2026-08-21 by applying v5's own waiver rule (`@eprintln-ok` on the hit
line or the line above) with a script instead of the rail:

| | count |
|---|---|
| `eprintln!` sites in `v6/*/src/**/*.rs` | 17 |
| waived by `@eprintln-ok` | 12 |
| **unwaived** | **5** |

The five are carded at @v6-eprintln-ratchet-five.
