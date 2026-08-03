# REPORT - dl6 LSP rich diagnostics lane

Branch lane/dl6-vscode, worktree root.

## Entry gate
`git merge --ff-only b7cdf014` -> `Already up to date.` Proceeded.

## What I own
- `plans/2026-08-02-dl6-lsp-rich-diagnostics.md` (price + recommendation + evidence)
- `proofs/dl6-rich-diag/` (the working proof + captured render evidence)
- this REPORT.md

## Answer, short

PART 1: LSP `Diagnostic.message` cannot carry markdown or HTML; VS Code renders it
as plain, HTML-escaped text (proved in a live Extension Development Host). The
supported rich routes, all proved by capture: `code` (object-with-Uri form) gives
a clickable link; `relatedInformation` gives clickable jump locations; a
HoverProvider returning a MarkdownString gives full markdown plus safe-tag HTML
with inline styles stripped; a webview gives full untruncated HTML at the highest
wiring cost; CodeLens gives a clickable line action whose tooltip did not surface.
InnerHTML capture, for the money rows:

Diagnostic overlay (HTML escaped to `&lt;b&gt;`, nothing renders):
`parse error here **bold** `mono` [linked](#target) &lt;b&gt;html-bold&lt;/b&gt; ...`

Proof HoverProvider (markdown rendered `<strong>`, `<b>` tag survived, inline
`style` attribute stripped):
`<p><strong>Hover heading</strong> <code>mono</code> <a ...>doc link</a></p><p><b>block html bold</b><br><span>html red span here</span></p>`

Recommended PART 1 route: (d) HoverProvider re-render of the diagnostic as
markdown, on top of (b)+(c) on the diagnostic itself. This is the v5 shipped
pattern (`editors/vscode-dl/src/extension.ts:261-286`); the proof reproduces it
with `supportHtml=true` and captures it working.

PART 2: recommend route (a) - the extension watches the dl6 compiler's JSONL and
sets diagnostics itself. No server, no subprocess in the Rust LSP, reuses the
extension's existing watcher + diagnostics API, matches the user's stated intent
(a diagnostic state someone reads), and keeps the Prolog compiler a batch writer.
Priced (b) TS language server and (c) Rust-shell-out as heavier with no current
payoff. The one precondition: the blocked diag-channel lane
(`chat_log/20260802.2...diag.pl`: findings `diag_wrong_position`,
`diag_lab_coupling`, `diag_bare_uri`) must merge with its defects fixed; the
emitter is not in this worktree and route (a) cannot be scripted until then.

## Evidence
The proof extension ran in a real VS Code 1.120.0 / Electron 39 Extension
Development Host driven over CDP. PNGs: `proofs/dl6-rich-diag/shot-1-problems.png`
(Diagnostic.message plain/escaped in the Problems panel),
`shot-2-hover.png` (markdown rendered in the HoverProvider row next to the
plain/escaped diagnostic row), `shot-3-webview.png` (webview panel). The DOM
dumps in the plan doc and proof README are the verified record.
Reproduce: `cd proofs/dl6-rich-diag && npm install && npx tsc -p ./ && time node drive-all.js`; or open the extension (F5) and hover the squiggle.

Receipts:
- compile: `npx tsc -p ./` -> CLEAN
- dl6 compiler exists and throws plain refusal: `swipl ... compile_dl6(...)` ->
  `parse error at line 3, column 12: statement` (no JSONL; emitter not merged)
- capture: Problems panel text = literal message + source + code + 2 related;
  hover DOM = escaped diagnostic row + rendered markdown HoverProvider row.

## Caveats / what I could not do
- I cannot read the PNGs (no image input in this model); the DOM text/HTML
  captures are the verified record. A human should eyeball the three PNGs.
- The webview inner document text and the CodeLens tooltip did not surface in
  CDP capture; both are secondary (webview already proven by the repo's flow
  panel; the CodeLens title rendered on the code line).
- The dl6 JSONL emitter is absent from this worktree (blocked lane), so route (a)
  is priced, not scripted end to end.

Did not: commit, push, spawn subagents, create worktrees, touch `v6/prolog/`
(only read), or modify `editors/vscode-dl/` (the proof is a separate extension).
