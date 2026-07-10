---
name: reference-vscode-samever-install-trap
description: code --install-extension with an unchanged version silently no-ops/stages into .obsolete while VS Code runs; always hash-verify the installed media file
metadata: 
  node_type: memory
  type: reference
  originSessionId: 4e6d11b9-5d44-4352-bd43-76b4f9c29ee7
---

Reinstalling a vsix with the SAME version (e.g. dl-lsp 0.4.1 over 0.4.1) while
VS Code is running can silently skip or stage the update: the extensions dir
keeps the OLD files, `.obsolete` may mark the new dirs, and `extensions.json`
can re-register a stale version. Bit three times on 2026-07-03 (dl flow-panel
fixes "installed" but the user kept running the first build of the day).

**How to apply:** for dev iteration on editors/vscode-dl, install with the full
dance and VERIFY:
1. `code --uninstall-extension sprefa.dl-lsp`
2. `rm -rf ~/.vscode/extensions/sprefa.dl-lsp-*`
3. `code --install-extension <vsix>`
4. `md5 -q <worktree>/media/flow-panel.html ~/.vscode/extensions/sprefa.dl-lsp-<ver>/media/flow-panel.html` — hashes must match
Then the user reloads the window (webview panels also cache HTML via
retainContextWhenHidden — reopen the panel). Bumping the version instead also
works and forces a fresh dir. Related: [[feedback-sonnet5-for-coding]].
