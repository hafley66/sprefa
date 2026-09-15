# msstyle x f41

Skill `msstyle`: Claude-ai-technical-writing-skill (Microsoft Manual of Style) entry `SKILL.md`, used through its missing `workflows/create.md` path. Model `f41`: `openrouter/deepseek/deepseek-v4.1-flash`.

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "One or two sentences: what this feature does and why someone would use it." (`reference/syntax.md`) | Opening sentence at `10_effects.md:3`, one sentence. |
| "Pattern to follow: Concept (what and why) → Task (how) → Reference" (`reference/syntax.md`) | Order `What`/`Why` (concept), `When to Use`/`Example` (task), `What Proves It` (reference). |
| "Use **Title Case**: capitalize all major words" (`reference/style-rules.md`) | Headings `When to Use` (`:49`), `What Proves It`; `What`, `Why`, `Example` already major-word. |
| "Do not skip levels" (`reference/style-rules.md`) | `# Effects` then `##` only; no `###`. |
| "Use active voice for explanations" (`reference/style-rules.md`) | Every sentence names its actor: "The runner resolves...", "An unknown name produces..." (`:21`). |
| "Short sentences. Plain words." (`reference/style-rules.md`) | Input's long parenthetical sentences split, e.g. `:21`, `:23`, `:25`, `:29`. |
| "Parallel structure throughout... Capitalize the first word of each item" (`reference/style-rules.md`) | Bullets capitalized and parallel at `:53-55`, `:59-61`. |
| "Introduce every table with a sentence before it" (`reference/style-rules.md`) | `:33` before the plan table; the sentence before the `What Proves It` table. |
| "No punctuation in column headers" (`reference/style-rules.md`) | `Plan \| Code \| Winner` (`:35`), `Claim \| Path \| Command`. |
| "Spell out zero through nine; numerals for 10 and above" (`reference/style-rules.md`) | "arity two" at `:29` and `:39`. |
| "cite every finding... Always give a line number" (`SKILL.md`) | Every claim keeps its `path:line`, inline at `:3`-`:61` and in both tables. |
| "Use **see**, not 'refer to'" (`reference/syntax.md`) | No "refer to"; links are `[Not built yet](16_not_built.md)`, `[Executors](11_executors.md)` (`:60-61`). |
| "Link text must describe the destination — never 'click here' or 'this page'" (`reference/syntax.md`) | Link text is the destination page title. |
| "Don't repeat the same cross-reference more than once per topic" (`reference/syntax.md`) | `Related Topics` section omitted; the two links already appear inline. |
| "Remove sections that don't apply — a short, accurate page beats a long, padded one." (`reference/syntax.md`) | No `Prerequisites`, `Troubleshooting`, or `Related Topics`; this page has none. |
| "Avoid time-stamped language outside release notes." (`reference/style-rules.md`) | Considered advisory; decision dates at `:45` kept as source attribution, not as "currently/new/now". |
| "Define acronyms on first use" (`reference/style-rules.md`) | Considered advisory for `JSON`; expanding it would not help this reader, so it stays as the established name. |
| "Stale UI labels... Trademark symbols on first mention" (`README.md`) | Not applicable; the page names no UI or trademarks. |

## Prev vs next

| aspect | prev (input page) | next (your page) |
|---|---|---|
| opening | `# Effects` then straight to `## What` and the diagram; no overview. | One-sentence overview at `:3` that names `--serve` and the `effect` row, then `## What`. |
| heading set | `What`, `Why`, `When to use`, `Example`, `What proves it`. | `What`, `Why`, `When to Use`, `Example`, `What Proves It`. Same five, Title Case. |
| diagram | `mermaid` flowchart under `## What`. | Identical mermaid, unchanged. |
| citations | inline `path:line` and two tables of citations. | Same inline citations and same ranges; no citation dropped or renumbered except the prose "arity 2" → "arity two". |
| sentence length | Multi-clause sentences with trailing parenthetical citations (`:20-21`). | Split into one-claim sentences with the citations kept (`:21`, `:23`, `:25`, `:29`). |
| tables | Two tables, neither introduced; lowercase headers. | Two tables, each introduced (`:33`); headers Title Case. |
| length in lines | 154. | 185. |

## Dropped or changed facts

- In-fence `; fixture: v8/fixtures/...` path comments removed from the three `dl7` blocks. Replaced by the prose line naming the file above each block, which the lane brief requires for a byte-for-byte `dl7` copy. The path is stated, so no fact is lost.
- The `3_loading` `dl7` block expanded from input lines 21-26 to the whole 26-line fixture, required by the byte-for-byte `dl7` rule. No new behavior claimed; the extra lines were already visible in the fixture.
- "arity 2" became "arity two" under the skill's number rule. Same value.
- The skill's `Related Topics` template section was omitted, because the only two cross-references already sit inline and the skill bars repeating a reference. Both links survive.
- No fact from the input page was dropped.

## Skill questions answered on the user's behalf

- Doc root: `v8/book/src`.
- Static site generator: mdBook (`v8/book/book.toml`). mdBook renders the body `# Effects` H1, so the skill's "H1 from front matter" does not apply; no front matter is added.
- UI config file: none. mdBook navigation is `SUMMARY.md`, and this output is not added to it.
- Feature map: none exists; not applicable to this page.
- `workflows/create.md` is absent from the archived skill (only `workflows/coverage.md` shipped). Followed `SKILL.md` plus the `reference/syntax.md` "New Page Template" instead.
- The skill's "Ask before you change anything" was not followed because the brief forbids asking; the skill's own recommendation is to report findings and keep the facts.

## Checkers run

The skill ships no checker script; it ships the grep patterns in `reference/ui-terms.md`. Run against the output:

```
$ f="v8/book/src/lab/docs-skill-bakeoff/msstyle/f41/10_effects.md"
$ for p in '(?i)\bclick on\b' '(?i)\bdeselect\b' '(?i)\bdropdown\b' '(?i)\blaunch\b' \
    '(?i)\bhighlight\b' '(?i)\binvoke\b' '(?i)\bthe user\b' '(?i)\b(we|our|us)\b' \
    '(?i)\brefer to\b' '(?i)\bclick here\b'; do
    echo "PATTERN $p ->"; rg -n --pcre2 "$p" "$f" || echo "  (no match)"
  done
PATTERN (?i)\bclick on\b -> (no match)
PATTERN (?i)\bdeselect\b -> (no match)
PATTERN (?i)\bdropdown\b -> (no match)
PATTERN (?i)\blaunch\b -> (no match)
PATTERN (?i)\bhighlight\b -> (no match)
PATTERN (?i)\binvoke\b -> (no match)
PATTERN (?i)\bthe user\b -> (no match)
PATTERN (?i)\b(we|our|us)\b -> (no match)
PATTERN (?i)\brefer to\b -> (no match)
PATTERN (?i)\bclick here\b -> (no match)
```

Byte-for-byte `dl7` check against `v8/fixtures/`:

```
$ python3 -c 'import re,pathlib; out=pathlib.Path(".../10_effects.md").read_text(); \
  blocks=re.findall(r"```dl7\n(.*?)```", out, re.S); \
  [print(f, "BYTE-MATCH" if b==pathlib.Path(f).read_text() else "MISMATCH") \
   for b,f in zip(blocks, ["v8/fixtures/host_effect/1_settled.dl7", \
   "v8/fixtures/host_effect/3_loading.dl7", "v8/fixtures/host_effect/2_source.dl7"])]'
v8/fixtures/host_effect/1_settled.dl7 BYTE-MATCH
v8/fixtures/host_effect/3_loading.dl7 BYTE-MATCH
v8/fixtures/host_effect/2_source.dl7 BYTE-MATCH
```

Build gate. The project has no lint command, so the skill's "Automated linting" phase is skipped:

```
$ cd v8/book && mdbook build 2>&1 | tail -5
 WARN Caused By: failed to read `.../v8/book/src/modules/../probes/21_partial_in_field.dl7`
 WARN Caused By: No such file or directory (os error 2)
Warning: The mdbook-mermaid preprocessor was built against version 0.5.0 of mdbook, but we're being called from version 0.5.4
 INFO Running the html backend
 INFO HTML book written to `.../v8/book/book`
```

Exit code 0. The `tail -5` shows no error. The full log carries three pre-existing `ERROR Error updating "{{#include ../probes/*.dl7}}"` lines from `modules/` pages whose probe files are absent from the tree; none names this page.
