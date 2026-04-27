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

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "sprf" }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.sprf"),
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
