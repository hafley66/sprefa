import * as vscode from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions, TransportKind, State } from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(ctx: vscode.ExtensionContext): void {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders) return;

  const cfg = vscode.workspace.getConfiguration("dl");
  const binary = cfg.get<string>("binaryPath", "dl");
  const program = cfg.get<string>("program", "");
  const root = cfg.get<string>("root", "") || folders[0].uri.fsPath;

  const args = program ? [program, "--root", root, "--lsp"] : ["--root", root, "--lsp"];

  const serverOptions: ServerOptions = {
    run: { command: binary, args, transport: TransportKind.stdio },
    debug: { command: binary, args, transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "dl" },
      { scheme: "file", language: "rust" },
      { scheme: "file", language: "typescript" },
      { scheme: "file", language: "typescriptreact" },
      { scheme: "file", language: "javascript" },
      { scheme: "file", language: "javascriptreact" },
      { scheme: "file", language: "python" },
      { scheme: "file", language: "go" },
      { scheme: "file", language: "kotlin" },
      { scheme: "file", language: "json" },
      { scheme: "file", language: "yaml" },
      { scheme: "file", language: "toml" },
      { scheme: "file", language: "shell" },
    ],
    synchronize: { fileEvents: vscode.workspace.createFileSystemWatcher("**/*") },
  };

  const bar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
  bar.text = "dl: starting";
  bar.show();
  ctx.subscriptions.push(bar);

  client = new LanguageClient("dlLSP", "dl LSP", serverOptions, clientOptions);
  client.onDidChangeState((e) => {
    bar.text = e.newState === State.Running ? "dl: ready" : "dl: stopped";
    bar.backgroundColor = e.newState === State.Running
      ? undefined
      : new vscode.ThemeColor("statusBarItem.warningBackground");
  });
  ctx.subscriptions.push({ dispose: () => { void client?.stop(); } });
  void client.start();
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
