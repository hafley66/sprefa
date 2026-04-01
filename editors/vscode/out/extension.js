"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    const config = vscode_1.workspace.getConfiguration("sprf");
    const serverPath = config.get("serverPath") || "sprf-lsp";
    const serverOptions = {
        command: serverPath,
        args: [],
    };
    const clientOptions = {
        documentSelector: [{ scheme: "file", language: "sprf" }],
    };
    client = new node_1.LanguageClient("sprf-lsp", "sprf Language Server", serverOptions, clientOptions);
    client.start();
}
function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
//# sourceMappingURL=extension.js.map