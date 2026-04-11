#!/usr/bin/env python3
"""LSP JSON-RPC client for e2e testing."""

import json
import subprocess
import time
from pathlib import Path

class LspClient:
    def __init__(self, lsp_path: str, cwd: str = None):
        self.proc = subprocess.Popen(
            [lsp_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=cwd,
        )
        self.msg_id = 0
        
    def send(self, method: str, params: dict, msg_id: int = None) -> None:
        """Send JSON-RPC request."""
        if msg_id is None:
            self.msg_id += 1
            msg_id = self.msg_id
            
        msg = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "method": method,
            "params": params
        }
        body = json.dumps(msg)
        header = f"Content-Length: {len(body.encode('utf-8'))}\r\n\r\n"
        data = (header + body).encode('utf-8')
        self.proc.stdin.write(data)
        self.proc.stdin.flush()
        
    def notify(self, method: str, params: dict) -> None:
        """Send JSON-RPC notification (no response expected)."""
        msg = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }
        body = json.dumps(msg)
        header = f"Content-Length: {len(body.encode('utf-8'))}\r\n\r\n"
        data = (header + body).encode('utf-8')
        self.proc.stdin.write(data)
        self.proc.stdin.flush()
        
    def recv(self, timeout: float = 5.0, expect_id: int = None) -> dict:
        """Receive JSON-RPC response."""
        start = time.time()
        
        while time.time() - start < timeout:
            # Read header byte by byte
            header = b""
            while True:
                char = self.proc.stdout.read(1)
                if not char:
                    time.sleep(0.01)
                    if time.time() - start > timeout:
                        break
                    continue
                header += char
                if header.endswith(b"\r\n\r\n"):
                    break
                    
            if not header:
                continue
                
            # Parse Content-Length
            content_length = 0
            for line in header.decode('utf-8', errors='replace').split("\r\n"):
                if line.startswith("Content-Length:"):
                    try:
                        content_length = int(line.split(":")[1].strip())
                    except:
                        pass
                    break
                    
            if content_length == 0:
                continue
                
            # Read body
            body = b""
            while len(body) < content_length:
                chunk = self.proc.stdout.read(content_length - len(body))
                if not chunk:
                    time.sleep(0.01)
                    continue
                body += chunk
                
            try:
                msg = json.loads(body.decode('utf-8'))
                # If expect_id specified, only return matching response
                if expect_id is not None and msg.get("id") != expect_id:
                    continue
                return msg
            except json.JSONDecodeError:
                continue
                
        raise TimeoutError(f"No response received within {timeout}s")
        
    def initialize(self, root_uri: str) -> dict:
        """Send initialize and return capabilities."""
        self.send("initialize", {
            "processId": 123,
            "rootUri": root_uri,
            "capabilities": {}
        }, msg_id=1)
        resp = self.recv(expect_id=1)
        
        # Send initialized notification
        self.notify("initialized", {})
        time.sleep(0.1)
        
        return resp.get("result", {}).get("capabilities", {})
        
    def open_doc(self, uri: str, text: str, language_id: str = "sprf") -> None:
        """Open document."""
        self.notify("textDocument/didOpen", {
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": 1,
                "text": text
            }
        })
        time.sleep(0.2)
        
    def complete(self, uri: str, line: int, character: int) -> list:
        """Request completions."""
        self.send("textDocument/completion", {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character}
        }, msg_id=2)
        resp = self.recv(expect_id=2)
        result = resp.get("result", [])
        # Handle both array and CompletionList
        if isinstance(result, list):
            return result
        return result.get("items", [])
        
    def hover(self, uri: str, line: int, character: int) -> dict:
        """Request hover."""
        self.send("textDocument/hover", {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character}
        }, msg_id=3)
        resp = self.recv(expect_id=3)
        return resp.get("result")
        
    def shutdown(self) -> None:
        """Shutdown LSP."""
        try:
            self.send("shutdown", {}, msg_id=99)
            try:
                self.recv(timeout=1.0, expect_id=99)
            except TimeoutError:
                pass
            self.notify("exit", {})
        except:
            pass
        finally:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=2.0)
            except:
                self.proc.kill()
