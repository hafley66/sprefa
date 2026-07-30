#!/usr/bin/env python3
"""parity-grep.py -- the ONE generic line-regex extractor the two spelunk
programs call. It is the `sh` host's executable, exactly as `$DL_EXTRACT_BIN` is
the flagship rail's; nothing about it is engine machinery.

CONTRACT, so the .dl6 side can be read without reading this file:

    parity-grep.py <rev> <git-pathspec> <python-regex>   ->  JSONL on stdout

  rev
      `WORK` reads the WORKING TREE (v5's own `scan` default rev word, reused
      deliberately so the two engines' rev vocabulary matches). Any other value
      is a git revision: the file set comes from `git ls-files --with-tree=<rev>`
      and each file's BYTES come from `git show <rev>:<path>`, so nothing on
      disk is read and the answer is immutable for that rev.

      THE REV IS WHY THIS ARGUMENT EXISTS AT ALL. An `sh` host's witness is
      content-addressed over its INPUT COLUMNS, so a worktree-only host caches
      its first answer for the life of the db (enumerate-hosts.dl6 states this
      about its own `enumerate`). A rail that must show a diagnostic APPEARING
      and RETRACTING across commits therefore cannot ask a worktree host twice
      -- it asks a rev-pinned host once per rev, and each commit is a new
      witness by construction.

  git-pathspec
      passed straight to git. `*` crosses `/` in a pathspec; `:(glob)` magic
      makes it stop at `/`. Both spellings select different sets and the
      programs use both ON PURPOSE (see the scan() reconciliation in
      plans/2026-07-30-v5-parity-spelunk.md).

  python-regex
      applied to every line with `re.finditer`, so a line with N matches emits
      N rows. A per-line "first match only" reading would undercount call
      sites, which is exactly the number these programs grade.

OUTPUT: one JSON object per match, ALWAYS carrying every declared column:

    {"path": ..., "line": <1-based int>, "g1": ..., "g2": ..., "g3": ...}

A capture group the pattern does not have, or that did not participate,
renders as "" -- never null, because serve/1_hosts.ts drops any object missing
a declared output or carrying it as null, and a dropped row is a silently lost
fact.

LINE NUMBERS ARE 1-BASED HERE and v5's `diag` schema is 0-BASED. The conversion
is a `:=` bind in the .dl6 rule, in the open, not a hidden adjustment here.

WHY A SCRIPT AND NOT AN INLINE TEMPLATE: `1_host_expand.pl:validate_template/3`
throws `template_mismatch(unknown_column(N))` for any `{identifier}` in a host
template that is not a declared input column, so awk/python program text cannot
live in a template -- `{print}` alone is a refusal.

REGEX DIALECT IS PYTHON `re`, stated rather than implied: it is NOT v5's regex
(the rust `regex` crate, via SQLite REGEXP) and NOT globset. Where a pattern
would read differently under those, that is a real difference and the receipt
says so instead of assuming the dialects agree.

Undecodable bytes use errors="replace" rather than dropping the file, so a
stray non-UTF8 byte cannot silently delete a whole file's rows.
"""
import json
import re
import subprocess
import sys

WORKTREE = "WORK"


def run_git(args):
    return subprocess.run(["git"] + args, capture_output=True, check=False)


def file_set(rev, pathspec):
    args = ["ls-files", "--", pathspec] if rev == WORKTREE else [
        "ls-files", "--with-tree=" + rev, "--", pathspec
    ]
    listing = run_git(args)
    if listing.returncode != 0:
        sys.stderr.write(
            "parity-grep: git %s failed: %s\n"
            % (" ".join(args), listing.stderr.decode("utf-8", "replace").strip())
        )
        return []
    return [line for line in listing.stdout.decode("utf-8", "replace").splitlines() if line]


def file_text(rev, path):
    if rev == WORKTREE:
        try:
            with open(path, "rb") as handle:
                return handle.read().decode("utf-8", "replace")
        except OSError:
            return None
    blob = run_git(["show", "%s:%s" % (rev, path)])
    # `ls-files --with-tree` unions the tree with the index, so a path staged
    # but absent at that rev has no blob. Absence is row absence, never a row
    # with empty text.
    if blob.returncode != 0:
        return None
    return blob.stdout.decode("utf-8", "replace")


def main():
    if len(sys.argv) != 4:
        sys.stderr.write("usage: parity-grep.py <rev|WORK> <git-pathspec> <python-regex>\n")
        return 2
    rev, pathspec, pattern = sys.argv[1], sys.argv[2], sys.argv[3]
    try:
        rx = re.compile(pattern)
    except re.error as bad:
        sys.stderr.write("parity-grep: bad regex %r: %s\n" % (pattern, bad))
        return 2
    out = sys.stdout
    for path in file_set(rev, pathspec):
        text = file_text(rev, path)
        if text is None:
            continue
        for number, line in enumerate(text.split("\n"), start=1):
            for hit in rx.finditer(line):
                groups = hit.groups()
                out.write(
                    json.dumps(
                        {
                            "path": path,
                            "line": number,
                            "g1": groups[0] if len(groups) > 0 and groups[0] is not None else "",
                            "g2": groups[1] if len(groups) > 1 and groups[1] is not None else "",
                            "g3": groups[2] if len(groups) > 2 and groups[2] is not None else "",
                        }
                    )
                    + "\n"
                )
    return 0


if __name__ == "__main__":
    sys.exit(main())
