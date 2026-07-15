#!/usr/bin/env python3
"""Generate a deterministic Rust corpus for the reactivity evidence harness."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from pathlib import Path

ALLOWED_SIZES = (10, 100, 1000)
DEFAULT_SEED = "sprefa-reactivity-v1"
SAFE_SEED = re.compile(r"[A-Za-z0-9_-]+")


def reject_symlink_components(path: Path) -> None:
    absolute = path.absolute()
    current = Path(absolute.anchor)
    for part in absolute.parts[1:]:
        current /= part
        if current.exists() and current.is_symlink():
            raise ValueError(f"symlinked path component is forbidden: {current}")


def stable_number(seed: str, index: int) -> int:
    digest = hashlib.sha256(f"{seed}:{index}".encode()).digest()
    return int.from_bytes(digest[:4], "big") % 100_000


def source_text(seed: str, index: int, edited: bool = False) -> str:
    value = stable_number(seed, index)
    suffix = "" if index == 0 else f"_{index:04d}"
    helper_a = f"helper_a{suffix}"
    helper_b = f"helper_b{suffix}"
    selected = helper_b if edited and index == 0 else helper_a
    return (
        f"// deterministic fixture {index:04d}; seed={seed}\n"
        f"pub fn {helper_a}(value: i64) -> i64 {{ value + {value} }}\n"
        f"pub fn {helper_b}(value: i64) -> i64 {{ value - {value} }}\n"
        f"pub fn entry_{index:04d}(value: i64) -> i64 {{ {selected}(value) }}\n"
    )


def write_exclusive(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8", newline="\n") as handle:
        handle.write(content)


def generate(root: Path, manifest_path: Path, files: int, seed: str) -> dict:
    if files not in ALLOWED_SIZES:
        raise ValueError(f"fixture size must be one of {ALLOWED_SIZES}, got {files}")
    if not SAFE_SEED.fullmatch(seed):
        raise ValueError("fixture seed must contain only ASCII letters, digits, '_' or '-'")
    reject_symlink_components(root)
    reject_symlink_components(manifest_path)
    if root.exists() and any(root.iterdir()):
        raise FileExistsError(f"fixture root is not empty: {root}")
    if manifest_path.exists():
        raise FileExistsError(f"manifest already exists: {manifest_path}")
    root.mkdir(parents=True, exist_ok=True)
    root = root.resolve(strict=True)

    digest = hashlib.sha256()
    entries = []
    total_bytes = 0
    for index in range(files):
        relative = Path("corpus") / f"file_{index:04d}.rs"
        content = source_text(seed, index)
        encoded = content.encode("utf-8")
        write_exclusive(root / relative, content)
        digest.update(relative.as_posix().encode("utf-8"))
        digest.update(b"\0")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
        total_bytes += len(encoded)
        entries.append(
            {
                "path": relative.as_posix(),
                "bytes": len(encoded),
                "sha256": hashlib.sha256(encoded).hexdigest(),
            }
        )

    edit_relative = Path("corpus/file_0000.rs")
    before = source_text(seed, 0)
    after = source_text(seed, 0, edited=True)
    manifest = {
        "schema_version": 1,
        "fixture_seed": seed,
        "fixture_files": files,
        "fixture_bytes": total_bytes,
        "fixture_digest": digest.hexdigest(),
        "files": entries,
        "edit": {
            "path": edit_relative.as_posix(),
            "before_sha256": hashlib.sha256(before.encode()).hexdigest(),
            "after_sha256": hashlib.sha256(after.encode()).hexdigest(),
            "before": before,
            "after": after,
        },
    }
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    with manifest_path.open("x", encoding="utf-8", newline="\n") as handle:
        json.dump(manifest, handle, sort_keys=True, indent=2)
        handle.write("\n")
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--files", required=True, type=int, choices=ALLOWED_SIZES)
    parser.add_argument("--seed", default=DEFAULT_SEED)
    args = parser.parse_args()
    if not SAFE_SEED.fullmatch(args.seed):
        parser.error("--seed must contain only ASCII letters, digits, '_' or '-'")
    generate(args.root, args.manifest, args.files, args.seed)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
