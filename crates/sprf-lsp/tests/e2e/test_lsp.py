#!/usr/bin/env python3
"""E2E tests for sprf-lsp."""

import sys
import tempfile
import os
from pathlib import Path

# Add parent dir to path for importing lsp_client
sys.path.insert(0, str(Path(__file__).parent))
from lsp_client import LspClient

# Colors
GREEN = '\033[0;32m'
RED = '\033[0;31m'
YELLOW = '\033[1;33m'
NC = '\033[0m'

def find_lsp_binary() -> str:
    """Find sprf-lsp binary."""
    # Get project root (4 levels up from this file: e2e -> sprf-lsp -> crates -> sprefa)
    project_root = Path(__file__).parent.parent.parent.parent.parent
    
    candidates = [
        project_root / "target/debug/sprf-lsp",
        project_root / "target/release/sprf-lsp",
        Path.cwd() / "target/debug/sprf-lsp",
        Path.cwd() / "../../target/debug/sprf-lsp",
    ]
    for c in candidates:
        if c.exists():
            return str(c.absolute())
    raise FileNotFoundError(f"sprf-lsp binary not found. Looked in: {candidates}")

def test_initialize():
    """Test LSP initializes with completion capability."""
    print(f"{YELLOW}TEST: Initialize{NC}")
    
    lsp_path = find_lsp_binary()
    
    with tempfile.TemporaryDirectory() as tmpdir:
        client = LspClient(lsp_path, cwd=tmpdir)
        try:
            caps = client.initialize(f"file://{tmpdir}")
            
            assert "completionProvider" in caps, "Missing completion capability"
            assert "hoverProvider" in caps, "Missing hover capability"
            
            print(f"{GREEN}  ✓ Initialize with completion + hover{NC}")
            return True
        finally:
            client.shutdown()

def test_fs_completion():
    """Test fs(**/car completes to Cargo.toml."""
    print(f"{YELLOW}TEST: fs() completion{NC}")
    
    lsp_path = find_lsp_binary()
    
    with tempfile.TemporaryDirectory() as tmpdir:
        # Create test files
        Path(tmpdir, "Cargo.toml").touch()
        Path(tmpdir, "package.json").touch()
        os.makedirs(Path(tmpdir, "src"), exist_ok=True)
        Path(tmpdir, "src/main.rs").touch()
        
        client = LspClient(lsp_path, cwd=tmpdir)
        try:
            client.initialize(f"file://{tmpdir}")
            
            uri = f"file://{tmpdir}/test.sprf"
            client.open_doc(uri, "rule(test) { fs(**/car")
            
            items = client.complete(uri, line=0, character=20)
            
            labels = [i.get("label", "") for i in items]
            assert "Cargo.toml" in labels, f"Cargo.toml not in: {labels}"
            
            print(f"{GREEN}  ✓ Found Cargo.toml in completions{NC}")
            print(f"    Items: {labels[:3]}")
            return True
        finally:
            client.shutdown()

def test_repo_completion():
    """Test repo() completion from sprefa.toml."""
    print(f"{YELLOW}TEST: repo() completion{NC}")
    
    lsp_path = find_lsp_binary()
    
    with tempfile.TemporaryDirectory() as tmpdir:
        # Create sprefa.toml with repos
        with open(Path(tmpdir, "sprefa.toml"), "w") as f:
            f.write("""
[db]
path = "/tmp/test.db"

[[repos]]
name = "frontend"
path = "/tmp/frontend"
revs = ["main"]

[[repos]]
name = "backend"
path = "/tmp/backend"
revs = ["main"]
""")
        
        client = LspClient(lsp_path, cwd=tmpdir)
        try:
            client.initialize(f"file://{tmpdir}")
            
            uri = f"file://{tmpdir}/test.sprf"
            client.open_doc(uri, 'rule(test) { repo(front')
            
            items = client.complete(uri, line=0, character=25)
            
            labels = [i.get("label", "") for i in items]
            assert "frontend" in labels, f"'frontend' not in: {labels}"
            
            print(f"{GREEN}  ✓ Found 'frontend' repo{NC}")
            return True
        finally:
            client.shutdown()

def test_tag_completion():
    """Test tag completion (fs, json, repo, etc)."""
    print(f"{YELLOW}TEST: Tag completion{NC}")
    
    lsp_path = find_lsp_binary()
    
    with tempfile.TemporaryDirectory() as tmpdir:
        client = LspClient(lsp_path, cwd=tmpdir)
        try:
            client.initialize(f"file://{tmpdir}")
            
            uri = f"file://{tmpdir}/test.sprf"
            client.open_doc(uri, "rule(test) { js")  # incomplete 'json'
            
            items = client.complete(uri, line=0, character=16)
            
            labels = [i.get("label", "") for i in items]
            assert "json" in labels, f"'json' not in: {labels}"
            assert "fs" in labels, f"'fs' not in: {labels}"
            
            print(f"{GREEN}  ✓ Found json and fs tags{NC}")
            return True
        finally:
            client.shutdown()

def main():
    """Run all tests."""
    print("═══════════════════════════════════════════════════════")
    print("sprf-lsp E2E Tests")
    print("═══════════════════════════════════════════════════════")
    
    tests = [
        test_initialize,
        test_fs_completion,
        test_repo_completion,
        test_tag_completion,
    ]
    
    passed = 0
    failed = 0
    
    for test in tests:
        print("")
        try:
            if test():
                passed += 1
            else:
                failed += 1
        except Exception as e:
            print(f"{RED}  ✗ {e}{NC}")
            failed += 1
    
    print("")
    print("═══════════════════════════════════════════════════════")
    print(f"Results: {GREEN}{passed} passed{NC}, {RED}{failed} failed{NC}")
    
    return failed == 0

if __name__ == "__main__":
    sys.exit(0 if main() else 1)
