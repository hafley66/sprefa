# dl6 rich diag proof

Standalone VS Code extension proving what VS Code actually renders for markdown
and HTML in each rich-content route, run in a real Extension Development Host
(VS Code 1.120.0 / Electron 39) captured over the Chrome DevTools Protocol.

It does not touch `editors/vscode-dl/`. It is its own extension with its own
extension id (`dl6-rich-diag-proof`) and contributes its own `dl6` language id
(extension-host-only; it is never released alongside the real dl extension).
On activation it opens `scratch.dl6` and publishes one diagnostic; a human or a
driver only needs to look.

## What it registers (all against language `dl6`, on `scratch.dl6`)

- `diags` collection: one `Error` diagnostic on line 3 whose `message` contains
  markdown-ish text and raw HTML, whose `code` is `{ value, target: Uri }`, and
  with two `relatedInformation` clickable locations (extension.ts).
- A `HoverProvider` re-rendering the diagnostic as a `MarkdownString` with
  `isTrusted = true` and `supportHtml = true`, including raw HTML with an inline
  style and a `command:` URI (extension.ts).
- A `CodeLensProvider` with a command title + tooltip (extension.ts).
- A `WebviewPanel` command (the dl/refs style custom surface).

## Commands

- `dl6RichDiag.publishDiagnostics` - publish after edits
- `dl6RichDiag.openWebview` - open the full-HTML panel
- `dl6RichDiag.reportState` - write `proof-state.json`
- `dl6RichDiag.showHover` triggers `editor.action.showHover`

## Build

```
npm install
npx tsc -p ./
```

## What the capture proved (VS Code 1.120.0)

Captured over CDP from a real Extension Development Host; the DOM dumps are the
record (the PNGs are visual backups).

1. Diagnostic.message (Problems panel and the squiggle overlay): PLAIN TEXT.
   `**bold**`, `` `mono` ``, `[linked](#target)`, `<b>`, `<span>` all appear
   literally. In the overlay DOM the HTML tags are escaped
   (`&lt;b&gt;html-bold&lt;/b&gt;`), so nothing executes and nothing renders
   rich. This is the field the question started on: it cannot carry markdown or
   HTML.
2. Diagnostic.code = `{ value, target }`: renders as a clickable
   `<a class="code-link" href="file://...">` link in the overlay. Clickable out.
3. Diagnostic.relatedInformation: each renders as a clickable
   `<a>scratch.dl6(1, 1): ...</a>` that jumps to that location. Clickable.
4. HoverProvider returning `MarkdownString(supportHtml=true, isTrusted=true)`:
   markdown fully renders (`**bold**` -> `<strong>`, `` `mono` `` -> `<code>`,
   links -> `<a>`, and `command:` URIs become working buttons because
   `isTrusted=true`). With `supportHtml=true` the safe HTML tag `<b>` SURVIVES
   and renders bold, but the inline `style="color:..."` attribute is STRIPPED by
   the sanitizer (the `<span>` keeps no style). So markdown is fully supported;
   HTML is only supported as sanitized safe tags, never scripts or inline styles.
5. CodeLens: the lens title renders on the code line. The command `tooltip` did
   not surface as a `.monaco-tooltip` in capture.
6. Webview: opens a real `vscode-webview://` document serving arbitrary HTML
   (full HTML/CSS/JS, command buttons) - the only route where HTML is not
   sanitized. The repo's own flow panel (`editors/vscode-dl/media/flow-panel.html`)
   is the production proof of this route.

## How a human confirms (the render is already on screen)

1. `code --extensionDevelopmentPath=<this dir> /tmp/dl6ws` (or F5 from this dir).
2. `scratch.dl6` opens with a red squiggle on line 3.
3. Hover the squiggle: you see the plain-text message, the clickable
   `dl6/surface_findings:3` code link, and the two clickable "related" links.
4. Hover the `**Hover heading**` block: markdown renders bold/mono/links, the
   "block html bold" line renders bold (the `<b>` survived), and "html red
   span here" shows WITHOUT the red color (the style attribute was stripped).
5. Run `dl6RichDiag.openWebview`: full bold + a button render in the panel.
