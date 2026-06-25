import { workspace, ExtensionContext } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

export function activate(_context: ExtensionContext) {
  const config = workspace.getConfiguration("sprefa-dl");
  const serverPath = config.get<string>("serverPath") || "dl";
  const root =
    config.get<string>("root") ||
    workspace.workspaceFolders?.[0]?.uri.fsPath ||
    ".";

  // v5 LSP surface: `dl --lsp --root <dir>`. No program positional => discovery
  // mode (the server merges every .dl in the program dir). The program's `diag`
  // relation becomes editor diagnostics; lint fires on open/save (disk-truth).
  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ["--lsp", "--root", root],
  };

  // documentSelector is wide so requests (hover, codeAction, codeLens) route to
  // the server for any open file. Sync notifications are narrowed by middleware:
  // only .dl files push didOpen / didChange / didClose / didSave. The engine
  // reads every other file via its own fs ops; it does not need the editor's
  // in-memory buffer for non-.dl files.
  const isDl = (uri: { fsPath?: string; path: string }) =>
    (uri.fsPath ?? uri.path).endsWith(".dl");

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file" }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.dl"),
    },
    middleware: {
      didOpen:   (doc, next) => isDl(doc.uri) ? next(doc) : Promise.resolve(),
      didChange: (e,   next) => isDl(e.document.uri) ? next(e) : Promise.resolve(),
      didClose:  (doc, next) => isDl(doc.uri) ? next(doc) : Promise.resolve(),
      didSave:   (doc, next) => isDl(doc.uri) ? next(doc) : Promise.resolve(),
    },
  };

  client = new LanguageClient(
    "sprefa-dl",
    "sprefa-dl Language Server",
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
