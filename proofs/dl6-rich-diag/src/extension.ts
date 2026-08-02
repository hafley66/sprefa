import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";

// Self-contained proof extension. Registers every rich-content route against
// the dl6 language id and lets a test/playwright drive it via commands.
// Writes a state JSON on demand so a headless driver can assert what got wired.

export function activate(ctx: vscode.ExtensionContext): void {
  const statePath = path.join(ctx.extensionPath, "proof-state.json");

  const diags = vscode.languages.createDiagnosticCollection("dl6rich");

  // A document with these markers, opened in the editor, is the target of
  // publishDiagnostics. Markers give stable positions.
  const MARKUP_DL6 = [
    "rel a(x: int).",
    "rel b(y: int).",
    "here_is_the_bad_line :-",
    "",
  ].join("\n");

  // Ensure a scratch .dl6 to publish against even if none is open.
  function ensureDoc(): Thenable<vscode.TextDocument> {
    const uri = vscode.Uri.joinPath(ctx.extensionUri, "scratch.dl6");
    return vscode.workspace.openTextDocument(uri).then(
      (d) => d,
      () => {
        fs.writeFileSync(uri.fsPath, MARKUP_DL6);
        return vscode.workspace.openTextDocument(uri);
      });
  }

  // ── route (a): Diagnostic.message with markdown-ish + HTML text ────────────
  // VS Code renders this field as PLAIN TEXT (v5 already established this and
  // works around it with a hover provider). The string below carries markdown
  // (**bold**, `code`, [link](#)) and an HTML bold tag + inline style, so a
  // human can eyeball whether ANYTHING renders rich.
  const MarkdownISH_MSG =
    "parse error here **bold** `mono` [linked](#target) <b>html-bold</b> <span style=\"color:red\">html-red</span>";

  // ── routes (b)+(c): code+codeDescription.href and relatedInformation ──────
  const CODE_VALUE = "dl6/surface_findings:3";

  function publishDiagnostics(): void {
    void ensureDoc().then((doc) => {
      const range = doc.lineAt(2).range; // the bad line, 0-based line 2
      const uri = doc.uri;
      const relatedInfo: vscode.DiagnosticRelatedInformation[] = [
        new vscode.DiagnosticRelatedInformation(
          new vscode.Location(uri, new vscode.Range(0, 0, 0, 8)),
          "related: rel a declared here (click me)"),
        new vscode.DiagnosticRelatedInformation(
          new vscode.Location(uri, new vscode.Range(1, 0, 1, 8)),
          "related: rel b declared here (click me)"),
      ];
      const d = new vscode.Diagnostic(
        range,
        MarkdownISH_MSG,
        vscode.DiagnosticSeverity.Error);
      d.code = { value: CODE_VALUE, target: uri };
      d.relatedInformation = relatedInfo;
      d.source = "dl6proof";
      diags.set(uri, [d]);
      vscode.window.showInformationMessage("dl6RichDiag: published 1 diagnostic to " + uri.path);
    });
  }

  // ── route (d): HoverProvider returning MarkdownString with supportHtml ─────
  // The v5 extension uses exactly this pattern (isTrusted=true, supportHtml=false).
  // This proof sets supportHtml=true and includes raw HTML to test whether HTML
  // actually survives when the flag is on.
  ctx.subscriptions.push(vscode.languages.registerHoverProvider(
    { scheme: "file", language: "dl6" },
    {
      provideHover(document, position) {
        if (position.line !== 2) return undefined;
        const md = new vscode.MarkdownString();
        md.isTrusted = true;
        md.supportHtml = true; // the tested knob
        md.appendMarkdown("**Hover heading** `mono` [doc link](command:dl6RichDiag.openWebview)\n\n");
        md.appendMarkdown("<b>block html bold</b><br/><span style=\"color:rgb(255,0,0)\">html red span here</span>\n\n");
        md.appendMarkdown("A command button: [Open Webview](command:dl6RichDiag.openWebview).\n\n");
        md.appendMarkdown("Raw message text that the diagnostics pane renders: `" + MarkdownISH_MSG + "`");
        return new vscode.Hover(md, new vscode.Range(position.line, 0, position.line, 80));
      },
    }));

  // ── route (f): CodeLens with a command tooltip ────────────────────────────
  ctx.subscriptions.push(vscode.languages.registerCodeLensProvider(
    { scheme: "file", language: "dl6" },
    {
      provideCodeLenses(document) {
        return [
          new vscode.CodeLens(
            new vscode.Range(2, 0, 2, 1),
            {
              title: "run dl6 proof",
              tooltip: "CodeLens tooltip with **markdown-ish** and <b>html</b> text",
              command: "dl6RichDiag.reportState",
            }),
        ];
      },
    }));

  // ── routes: custom protocol feeding a webview (the dl/refs pattern) ───────
  let panel: vscode.WebviewPanel | undefined;
  ctx.subscriptions.push(vscode.commands.registerCommand("dl6RichDiag.openWebview", () => {
    if (panel) { panel.reveal(); return panel; }
    panel = vscode.window.createWebviewPanel(
      "dl6RichDiagPanel", "dl6 rich diag", vscode.ViewColumn.Beside, {
        enableScripts: true,
      });
    panel.webview.html = `<!DOCTYPE html><html><body>
      <h1>dl6 rich diag webview</h1>
      <p>This is a full <b>HTML</b> surface: bullet commands, <a id="ihref" href="https://example.com">a plain link</a>.</p>
      <ul><li>rendered by the webview, not sanitized to plain text</li></ul>
      <button id="btn">a button reachable by a human</button>
      <script>document.getElementById('btn').onclick=()=>alert('webview button works');</script>
    </body></html>`;
    panel.onDidDispose(() => { panel = undefined; });
    return panel;
  }));

  // ── command: report what got wired, written to proof-state.json ────────────
  ctx.subscriptions.push(vscode.commands.registerCommand("dl6RichDiag.reportState", () => {
    const state = {
      message: MarkdownISH_MSG,
      codeValue: CODE_VALUE,
      codeTargetUri: ensureSyncDocUri(ctx).toString(),
      relatedCount: 2,
      hoverRegistered: true,
      hoverSupportHtml: true,
      codelensRegistered: true,
      webviewCommand: "dl6RichDiag.openWebview",
      timestamp: Date.now(),
    };
    fs.writeFileSync(statePath, JSON.stringify(state, null, 2));
    void vscode.window.showInformationMessage("dl6RichDiag: state written to " + statePath);
    return state;
  }));

  ctx.subscriptions.push(vscode.commands.registerCommand("dl6RichDiag.publishDiagnostics", publishDiagnostics));

  // Publish an informational diagnostic at startup so a headless driver can
  // see the problems panel even before clicking anything. Open the scratch
  // doc in an editor and publish the FULL error diagnostic so hover + squiggle
  // are live without any human click.
  ensureDoc().then((doc) => {
    const full = new vscode.Diagnostic(
      doc.lineAt(2).range,
      MarkdownISH_MSG,
      vscode.DiagnosticSeverity.Error);
    full.code = { value: CODE_VALUE, target: doc.uri };
    full.relatedInformation = [
      new vscode.DiagnosticRelatedInformation(
        new vscode.Location(doc.uri, new vscode.Range(0, 0, 0, 8)),
        "related: rel a declared here (click me)"),
      new vscode.DiagnosticRelatedInformation(
        new vscode.Location(doc.uri, new vscode.Range(1, 0, 1, 8)),
        "related: rel b declared here (click me)"),
    ];
    full.source = "dl6proof";
    diags.set(doc.uri, [full]);
    void vscode.window.showTextDocument(doc, { preview: true });
  });
}

export function deactivate(): void {
  // no-op
}

// The scratch .dl6 lives next to the extension; the code target points at it.
function ensureSyncDocUri(ctx: vscode.ExtensionContext): vscode.Uri {
  return vscode.Uri.joinPath(ctx.extensionUri, "scratch.dl6");
}
