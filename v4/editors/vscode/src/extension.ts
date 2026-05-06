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

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'sprf' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.sprf'),
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
