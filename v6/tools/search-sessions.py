#!/usr/bin/env python3
"""Search YOUR messages across sprefa's Claude Code session transcripts.

Each session is a .jsonl under ~/.claude/projects/<project-slug>/ (one JSON
object per line: type, message.role, message.content). This tool extracts YOUR
typed turns (user-role text, skipping tool_result / tool_use / image noise and
the harness-injected <command-*> / <local-command-caveat> wrappers) and prints
the ones matching a regex — so the analogies and notes you wrote in past
sessions are findable instead of buried in ~500 MB of transcript.

Usage:
  search-sessions.py [-p PATH] [-n N] [--full] [--context] [--all] PATTERN

  PATTERN        regex, matched case-insensitively against message text
  -p, --path     session dir (default: the sprefa project slug below)
  -n, --limit    max matches to print (default 20)
  --full         print whole messages (default truncates to 600 chars)
  --context      also print the following assistant turn (the reconciled reply)
  --all          search assistant turns too (default: your messages only)

Stopgap: the long-term plan is for sprefa itself to query this filesystem space
as builtins (the scan/extract ops over files). dl (v5) is skipped for now until
the core is fixed; this script is the hand version of that capability.
"""
import argparse
import glob
import json
import os
import re
import sys

DEFAULT = os.path.expanduser("~/.claude/projects/-Users-chrishafley-projects-sprefa")

# Harness-injected wrapper noise to strip from displayed snippets (search still
# sees the raw text; this only cleans what is printed).
WRAPPER_RE = re.compile(
    r"<(?:command-(?:message|name)|local-command-(?:caveat|message)|system-reminder)>"
    r".*?</(?:command-(?:message|name)|local-command-(?:caveat|message)|system-reminder)>",
    re.DOTALL,
)


def role_text(obj, want_role):
    """Extract typed text from a message of the wanted role, or None."""
    msg = obj.get("message") or {}
    if msg.get("role") != want_role:
        return None
    content = msg.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = [
            b.get("text", "")
            for b in content
            if isinstance(b, dict) and b.get("type") == "text"
        ]
        joined = "\n".join(p for p in parts if p)
        return joined or None
    return None


def clean(text):
    return WRAPPER_RE.sub("", text).strip()


def show(text, full, width=600):
    text = clean(text)
    return text if (full or len(text) <= width) else text[:width] + f"  …[+{len(text) - width} chars]"


def main():
    ap = argparse.ArgumentParser(description="Search your sprefa session messages.")
    ap.add_argument("pattern")
    ap.add_argument("-p", "--path", default=DEFAULT)
    ap.add_argument("-n", "--limit", type=int, default=20)
    ap.add_argument("--full", action="store_true")
    ap.add_argument("--context", action="store_true", help="also print the following assistant turn")
    ap.add_argument("--all", action="store_true", help="search assistant turns too")
    args = ap.parse_args()

    rx = re.compile(args.pattern, re.IGNORECASE)
    files = sorted(glob.glob(os.path.join(args.path, "*.jsonl")))
    if not files:
        sys.exit(f"no .jsonl under {args.path}")

    printed = 0
    for f in files:
        if printed >= args.limit:
            break
        try:
            with open(f, encoding="utf-8") as fh:
                rows = [json.loads(line) for line in fh if line.strip()]
        except Exception:
            continue
        for i, obj in enumerate(rows):
            if printed >= args.limit:
                break
            txt = role_text(obj, "user")
            role = "user"
            if txt is None and args.all:
                txt = role_text(obj, "assistant")
                role = "assistant"
            if txt is None or not rx.search(txt):
                continue
            printed += 1
            print(f"\n## {os.path.basename(f)} · {role} #{i}")
            print(show(txt, args.full))
            if args.context and role == "user":
                for j in range(i + 1, min(i + 4, len(rows))):
                    at = role_text(rows[j], "assistant")
                    if at:
                        indented = show(at, args.full, 600).replace("\n", "\n    ")
                        print(f"  ↳ assistant:\n    {indented}")
                        break
    print(f"\n[{printed} match(es) across {len(files)} session(s)]", file=sys.stderr)


if __name__ == "__main__":
    main()
