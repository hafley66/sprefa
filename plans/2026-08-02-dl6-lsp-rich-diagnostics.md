# dl6 LSP rich diagnostics: can messages be rich, and the shortest path to them on screen

Status: research lane (dl6-vscode worktree, branch lane/dl6-vscode), 2026-08-02.
Question: can dl6 diagnostics carry markdown or HTML into VS Code, and what is
the shortest path to getting them on screen. Both parts answered with running
code, not documentation quotes.

Method: a standalone proof extension under `proofs/dl6-rich-diag/` was run in a
real VS Code Extension Development Host (VS Code 1.120.0, Electron 39,
macOS arm64) and driven over the Chrome DevTools Protocol. The DOM captures of
the Problems panel, the squiggle overlay hover, and an explicit HoverProvider are
the evidence. PNGs `shot-1-problems.png`, `shot-2-hover.png`, `shot-3-webview.png`
in that directory; the innerHTML/innerText dumps are quoted inline below.
Anyone can rerun: `cd proofs/dl6-rich-diag && npm install && npx tsc -p ./` then
`code --extensionDevelopmentPath=$PWD /tmp/dl6ws`.

The proof extension does not touch `editors/vscode-dl/`. It is its own extension.

## Ground truth: what the LSP wires actually render

The v5 extension has already encoded one fact the coordinator's grep missed:
`editors/vscode-dl/src/extension.ts:256-260` ships a HoverProvider whose comment
states Diagnostic.message is plain-text-only, which is why v5 re-renders dl
diagnostics as a MarkdownString on hover. That is the same mechanism the proof
this lane built, and the capture below proves the why.

The capture target on line 3 of `scratch.dl6`:
`parse error here **bold** `mono` [linked](#target) <b>html-bold</b> <span style="color:red">html-red</span>`

Diagnostic overlay hover, innerHTML (the `<b>` and `<span>` are escaped to
`&lt;b&gt;`, `&lt;span`; nothing executes, nothing is bold/red):

```
<span style="white-space: pre-wrap;">parse error here **bold** `mono` [linked](#target) &lt;b&gt;html-bold&lt;/b&gt; &lt;span style="color:red"&gt;html-red&lt;/span&gt;</span>
```

The proof HoverProvider row (MarkdownString, isTrusted=true, supportHtml=true),
innerHTML:

```
<p><strong>Hover heading</strong> <code>mono</code> <a ... data-href="command:dl6RichDiag.openWebview">doc link</a></p>
<p><b>block html bold</b><br><span>html red span here</span></p>
```

In that second row `**Hover heading**` rendered to `<strong>` (markdown works),
the safe tag `<b>` survived (that line renders bold), and the inline style
`color:rgb(255,0,0)` was stripped (the `<span>` carries no style attribute). The
`command:` URI became a working button because `isTrusted=true`.

## PART 1: can messages be rich? priced per route

### a. Diagnostic.message with markdown-ish / HTML text
What renders: the literal string. Both the Problems list and the squiggle overlay
render `**bold**`, `` `mono` ``, `[linked](#target)`, `<b>`, `<span>` as the
characters they are; in the overlay the HTML is additionally escaped (`&lt;b&gt;`).
What is sanitized: the whole field is treated as untrusted plain text; nothing is
interpreted. Does HTML survive: no, it is escaped. Cost: zero to adopt (it is the
default), but it buys nothing: this field cannot carry markdown or HTML. This is
the field the question started on and it is a dead end for rich content.
Evidence: extension.ts test string; overlay DOM dump above; `shot-1-problems.png`.

### b. Diagnostic.code (object form `{ value, target: Uri }`) / codeDescription.href
What renders: a single clickable link in the overlay and Problems entry. The
capture shows `<a class="code-link" href="file:///.../scratch.dl6">dl6/surface_findings:3</a>`.
Clicking opens the target URI. What is sanitized: only a URL; the value text is
plain. Does HTML survive: no, it is a location, not markup. Cost: one field; no
provider. Buy: one clickable "go somewhere" affordance per diagnostic. This is
the recommended way to attach an external doc link or a jump to the offender.
Evidence: hover HTML dump shows the `code-link` anchor; `shot-2-hover.png`.

### c. Diagnostic.relatedInformation
What renders: each item is a clickable location that jumps to that file:line. The
capture shows `<a style="cursor:pointer">scratch.dl6(1, 1): </a>` followed by its
message text, and the same for `(2, 1)`. What is sanitized: the per-item message
is plain text (same rule as the main message). Does HTML survive: no. Cost: one
array field; no provider. Buy: attach "see also" jumps to other decls, callers,
or the exact identifier, without any rich markup. Evidence: hover HTML dump.

### d. HoverProvider returning MarkdownString / MarkupContent kind=markdown
What renders: full markdown on hover at the cursor. `**bold**` -> `<strong>`,
`` `mono` `` -> `<code>`, `[x](#)` and `[x](https://...)` -> `<a>`. With
`isTrusted=true` a `command:` URI becomes a clickable button. What is sanitized:
with `supportHtml=true` the sanitizer keeps safe tags (`<b>`, `<br>`) but strips
inline `style` attributes, event handlers, and scripts (proved: the `<span
style="color:rgb(255,0,0)">` came out style-less). HTML survives only as safe
tags, never styles or scripts. Cost: a HoverProvider plus a message
re-render, and it is a hover, not the diagnostics list. This is the v5 way
(`extension.ts:261-286`), so it is established practice and the cheapest route
that makes a diagnostic's message visually rich where the user already looks (at
the squiggle). Evidence: the proof HoverProvider innerHTML dump; `shot-2-hover.png`.

### e. Custom LSP method feeding a webview panel (the dl/refs pattern)
What renders: arbitrary HTML, CSS, and JS in a dedicated panel; buttons, colored
spans, real `style` attributes all work because it is a document you author, not
a sanitized string. What is sanitized: nothing you wrote (the panel CSP and the
VS Code webview security model apply, but your own markup is not mangled). Does
HTML survive: fully, including inline styles, the one route where they do. Cost:
the most. It needs a panel, a message channel from the server, and a viewer; the
repo already builds exactly this for the flow panel (`extension.ts:619-762` +
`media/flow-panel.html`). Buy: the only route that renders true rich content
(tables, buttons, colored/diagnostic layouts). Use it for a diagnostic DETAIL
VIEW (click a squiggle -> side panel), not to replace the in-line squiggle.
Evidence: `shot-3-webview.png`; the frame is a `vscode-webview://` document.

### f. CodeLens and its tooltip
What renders: the lens title on the code line ("run dl6 proof" was visible in the
editor DOM capture). What is sanitized: title is plain/code. Does HTML survive:
no. The command `tooltip` string did not surface as a `.monaco-tooltip` in
capture at this VS Code version, so treat the tooltip as unreliable. Cost: a
CodeLensProvider (cheap), but the payoff is a clickable action button on a line,
not a rich message. Evidence: editor DOM capture (lens title present); tooltip
capture returned no tooltip node.

### Part 1 verdict
Diagnostic.* fields are plain text (a) plus two clickable affordances (b, c).
The only places a message becomes visually rich without building a full panel are
(d) the hover re-render, which supports markdown and safe-tag HTML (no inline
styles), and (e) the webview, which supports everything at the highest wiring
cost. Recommendation: (d) as the primary (markdown on hover at the squiggle, the
v5-established pattern), layered on (b)+(c) on the diagnostic itself for the
jumps; go to (e) only for a detail view that genuinely needs full HTML.

## PART 2: shortest path to diagnostics on screen, from a non-server compiler

Context the pricing must respect: the dl6 compiler is Prolog (swipl 10.0.2 here),
invoked per-file via `compile_dl6/2` (`v6/prolog/compile.pl:197`). On a broken
file it currently throws to stderr a plain-text refusal like
`parse error at line 3, column 12: statement` (reproduced in this lane). The
LSP-shaped JSONL emitter the coordinator cited (`v6/prolog/labs/diag_channel/diag.pl`)
is NOT in this worktree: it is on a blocked, unmerged lane (commit log findings
`diag_lab_coupling`, `diag_wrong_position`, `diag_bare_uri`). It must merge and be
fixed before any consuming route sees real records. Plan/status reference:
`chat_log/20260802.2.opus-flash-fleet-haskell-prolog-dl6-diag.pl` (open: fix_diag_wrong_position, move_diag_out_of_labs, file_scheme_uri).

Three routes:

a. Extension watches the JSONL file and publishes diagnostics itself.
   The extension already has the exact watching pattern (`.dl/marks.dl` watcher
   at `extension.ts:358-362`) and owns the VS Code diagnostics API. Work: a
   `FileSystemWatcher` over `**/*.dl6.jsonl` (or a per-file target), a line
   parser for the LSP-shaped records the emitter publishes, and a
   `createDiagnosticCollection().set()` mapping each record's range/code/related.
   No server, no subprocess, no crate. Cost: small; the compiler stays a batch
   tool that writes a state file. Buy: matches the user's stated intent verbatim
   (a diagnostic state somewhere someone reads), and it is the thinnest seam the
   coordinator's own directive named (`diag_state_is_a_seam`: "file or stream,
   do NOT wire the whole language server in Prolog").
   Dependencies blocking it: the emitter lane merge + the wrong-position fix.

b. A small TypeScript language server beside v6/tsv2 that runs the compiler.
   `v6/tsv2` is a TS compiler port, not a language server; a new server means an
   LSP lifecycle (initialize/shutdown, didOpen/didSave with debounce), spawning
   swipl or an in-TS compile per change, position mapping back onto the buffer,
   and a `publishDiagnostics` pull. It duplicates what `vscode-languageclient`
   already gives an extension for free and what route (a) never needs. Against
   the repo's "infra is bought, never built" law, a hand-rolled server that only
   echoes compiler output is the build-first option with the least existing
   surface to lean on. Cost: high, no advantage over (a) for a save-time
   compiler. Buy: only real if dl6 later needs live/incremental features that a
   watching extension cannot express (idle recompile, file-less diagnostics).

c. Extend the Rust src/lsp.rs to shell out to the dl6 compiler.
   The Rust server is the v5 .dl server (handles .dl only). Adding dl6 means:
   spawning the Prolog compiler as a subprocess on didOpen/didSave, capturing
   its JSONL (or parsing its stderr), mapping positions, and publishing through
   the existing server/client diagnostics path. Cost: medium-high; it couples a
   long-running Rust binary to a swipl subprocess lifetime and to the (currently
   absent) emitter, and it drags dl6 into a server built for v5. Against the law:
   it rebuilds inside the server what a watcher does outside it.

### Part 2 verdict
Recommend route (a): the extension watches the compiler's JSONL and sets
diagnostics itself. It is the shortest path, it matches the user's stated intent
(a readable diagnostic state), it reuses the extension's existing watcher and
diagnostics plumbing, and it keeps the Prolog compiler a batch tool that writes a
state file, which is exactly the seam the coordinator already ruled
(`diag_state_is_a_seam`). It is also the least-machinery option under
"infra is bought, never built": it buys VS Code's diagnostics API rather than
building a server. (b) and (c) are only justified if dl6 later needs live
incremental diagnostics neither a watcher nor a save-run can express; neither is
justified for the current batch compiler. The precondition for (a) is landing the
blocked diag-channel lane (its wrong-position and bare-uri defects) so the JSONL
carries true offender positions and file:// URIs.

## What I could NOT do
- I could not read the captured PNGs (this model has no image input), so the
  pixels are unverified by me; the DOM innerText/innerHTML dumps above are the
  verified record and stand alone. A human should eyeball the three PNGs.
- I could not extract the webview's inner document text via CDP (the nested
  `vscode-webview://` content frame did not yield body text reliably in this
  capture), and the CodeLens command tooltip did not surface as a tooltip node.
  Both are secondary routes; the webview is already proven capable by the repo's
  own flow panel, and the CodeLens title demonstrably rendered on the line.
- The dl6 JSONL emitter (the subject of route a) is absent from this worktree,
  so route (a) was priced, not scripted end to end. It cannot be scripted until
  the blocked diag-channel lane merges.
