// sprefa v4 VS Code extension entry point.
//
// Spawns `sprefa-lsp` (path configurable via `sprefa-v4.serverPath`) and
// hands it stdio. Document selector matches `language: sprf` so the
// declarative `.sprf` registration in package.json triggers activation.
//
// Diagnostics-only first slice. Hover / completion / semantic tokens
// arrive in later lanes.

import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(_context: vscode.ExtensionContext): void {
    const config = vscode.workspace.getConfiguration('sprefa-v4');
    const command = config.get<string>('serverPath') || 'sprefa-lsp';
    const args = config.get<string[]>('serverArgs') || [];

    const serverOptions: ServerOptions = {
        run:   { command, args, transport: TransportKind.stdio },
        debug: { command, args, transport: TransportKind.stdio },
    };

    // documentSelector is wide so inbound `publishDiagnostics` for ANY
    // file URI (e.g. .rs / .ts / .py raised by sprf lint rules) is accepted
    // by VS Code's DiagnosticCollection and surfaced through
    // `mcp__ide__getDiagnostics`. Sync notifications are narrowed by
    // middleware below: only .sprf files push didOpen / didChange /
    // didClose / didSave into the server's buffer overlay.
    const isSprf = (uri: vscode.Uri): boolean =>
        (uri.fsPath ?? uri.path).endsWith('.sprf');

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.sprf'),
        },
        middleware: {
            didOpen:   (doc, next) => isSprf(doc.uri)        ? next(doc) : Promise.resolve(),
            didChange: (e,   next) => isSprf(e.document.uri) ? next(e)   : Promise.resolve(),
            didClose:  (doc, next) => isSprf(doc.uri)        ? next(doc) : Promise.resolve(),
            didSave:   (doc, next) => isSprf(doc.uri)        ? next(doc) : Promise.resolve(),
        },
    };

    client = new LanguageClient(
        'sprefa-v4',
        'sprefa v4',
        serverOptions,
        clientOptions,
    );
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
