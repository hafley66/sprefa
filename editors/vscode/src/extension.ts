import { workspace, ExtensionContext } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  const config = workspace.getConfiguration("sprf");
  const serverPath = config.get<string>("serverPath") || "sprefa-server";

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ["--lsp-stdio"],
  };

  // documentSelector is wide so requests (hover, codeAction, codeLens)
  // route to the server for any open file. Sync notifications are
  // narrowed by middleware below: only .sprf files push didOpen /
  // didChange / didClose / didSave into the server's buffer overlay.
  // sprf decides what other files to read via its own fs ops; it does
  // not need the editor's in-memory buffer for non-sprf files.
  const isSprf = (uri: { fsPath?: string; path: string }) =>
    (uri.fsPath ?? uri.path).endsWith(".sprf");

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file" }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.sprf"),
    },
    middleware: {
      didOpen:   (doc, next) => isSprf(doc.uri) ? next(doc) : Promise.resolve(),
      didChange: (e,   next) => isSprf(e.document.uri) ? next(e) : Promise.resolve(),
      didClose:  (doc, next) => isSprf(doc.uri) ? next(doc) : Promise.resolve(),
      didSave:   (doc, next) => isSprf(doc.uri) ? next(doc) : Promise.resolve(),
    },
  };

  client = new LanguageClient(
    "sprf-lsp",
    "sprf Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
