#!/usr/bin/env bash
# getting-started.sh -- v6/GETTING-STARTED.md is EXECUTED, not described.
#
# The doc teaches by transcript, and a transcript that nobody replays rots
# silently: that is the gen_staleness_gate class (checked-in artifacts whose
# source moved under them, caught three separate times in this repo by a gate
# that did not exist yet). This script is that gate for the page. It reads the
# markdown, replays every marked block IN ONE PERSISTENT SHELL, and diffs the
# captured output against the text the page prints.
#
# WHY ONE PERSISTENT SHELL rather than a fresh `bash -c` per block: the page is
# a copy-paste transcript, and a reader's terminal keeps `export`s, the working
# directory, and the backgrounded `bop serve` job across commands. Replaying
# each block in its own process would silently diverge from what a reader
# experiences (`kill %1` in section 4 refers to the job section 3 started), and
# a receipt that does not run what the reader runs proves nothing. The shell is
# driven over a pipe with a sentinel line after each command, which is also how
# the exit status of every single command gets checked rather than only the
# block's last one.
#
# BLOCK PROTOCOL. An HTML comment immediately above a fenced block marks it:
#
#   <!-- gs:write <relpath> -->     write the block body to that path in the
#                                   work dir (parents created). This is the
#                                   page's "now edit a file" step, and it is
#                                   what drives the watcher in section 3.
#   <!-- gs:run [nodiff] [norm=bytes] -->
#                                   a ```console block. Lines starting with
#                                   "$ " are commands, run in order; every
#                                   other line is expected output. The whole
#                                   block's combined stdout+stderr is compared
#                                   against those lines after normalization.
#                                   `nodiff` runs the commands and checks only
#                                   their exit status (version banners and
#                                   `pnpm install` chatter are machine state,
#                                   not engine behaviour). `norm=bytes`
#                                   additionally collapses EVERY number to <n>,
#                                   which turns the `bop stats` block into an
#                                   assertion about the payload's SHAPE (which
#                                   keys, which nesting) rather than about an
#                                   RSS figure or a page count that no page
#                                   should be pinning.
#
# An unmarked fenced block is prose furniture and is neither run nor diffed.
#
# NORMALIZATION is deliberately narrow -- the temp work dir, the clone path,
# 64-hex content digests, 32-hex program hashes, and 10-digit epoch buckets.
# Everything else on the page is compared literally, including every row of
# every relation, so a delta that changes shape fails here.
#
# EXIT STATUS is checked per command and is expected to be 0. A block that
# wants a nonzero code writes it into the transcript the way a reader would
# (`cmd; echo "exit $?"`), which is also how the page documents the 0/1/2
# contract in section 5 without a second mechanism.
#
# SABOTAGE RECEIPT (run 2026-07-31, both reverted; tree clean after):
#
#   1. changed the expected `beats` count on section 2's tick-3 line from
#      `[[3]]` to `[[4]]` -- block 6 RED, unified diff naming that one line,
#      exit 1.
#   2. emptied the `del` half of section 3's first `.deltas.todo` line
#      (`{"add":[],"del":[[...,3],[...,4]]}` -> `{"add":[],"del":[]}`) --
#      block 15 RED the same way.
#
# The second one is the discriminating case: the retraction is what the section
# is about, and a gate that compared only row COUNTS or only the final `bop q`
# answer would have passed it, since the end state after that edit is identical
# either way.
# BUDGET (timeout-gun lane, 2026-07-31). Measured wall: 16s for 24 replayed
# blocks. Default 600s is ~37x that. Whole-script cap, because the replay runs
# a BACKGROUNDED `bop serve` inside a persistent shell that outlives every
# individual block -- there is no single command to cap, and a doc block that
# starts a server and never reaches its own teardown block is precisely the
# leak this catches. Override with GETTING_STARTED_BUDGET_S.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$V6/.." && pwd)"

. "$V6/tools/run-capped.sh"
cap_self "${GETTING_STARTED_BUDGET_S:-600}" getting_started "$@"
DOC="$V6/GETTING-STARTED.md"

[ -f "$DOC" ] || { printf 'FAIL  no doc at %s\n' "$DOC"; exit 1; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/getting-started.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

SPREFA="$REPO" python3 - "$DOC" "$WORK" "$REPO" <<'PY'
import os, re, subprocess, sys, difflib

doc_path, work, repo = sys.argv[1], sys.argv[2], sys.argv[3]
doc = open(doc_path, encoding="utf-8").read().splitlines()

# ── block extraction ────────────────────────────────────────────────────────
DIRECTIVE = re.compile(r"^<!--\s*gs:(\S+)\s*(.*?)\s*-->$")
blocks, index, pending = [], 0, None
while index < len(doc):
    line = doc[index]
    hit = DIRECTIVE.match(line.strip())
    if hit:
        pending = (hit.group(1), hit.group(2).split())
        index += 1
        continue
    if line.startswith("```"):
        body, index = [], index + 1
        while index < len(doc) and not doc[index].startswith("```"):
            body.append(doc[index])
            index += 1
        if pending is not None:
            blocks.append((pending[0], pending[1], body))
            pending = None
    index += 1

if not blocks:
    print("FAIL  no gs: blocks found in the doc")
    sys.exit(1)

# ── the persistent shell ────────────────────────────────────────────────────
SENTINEL = "__GS_DONE__"
shell = subprocess.Popen(
    ["bash"], cwd=work, text=True, bufsize=1,
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    env={**os.environ, "SPREFA": repo, "PS1": "", "PS2": ""},
)

def send(command):
    """Run one command; return (output_lines, exit_status)."""
    shell.stdin.write(command + "\n")
    shell.stdin.write(f'printf "%s %s\\n" "{SENTINEL}" "$?"\n')
    shell.stdin.flush()
    out = []
    while True:
        line = shell.stdout.readline()
        if line == "":
            return out, 127
        if line.startswith(SENTINEL):
            return out, int(line.split()[1])
        out.append(line.rstrip("\n"))

# ── normalization ───────────────────────────────────────────────────────────
# The realpath pass is not cosmetic on macOS: mktemp -d hands back
# /var/folders/..., which is a symlink to /private/var/folders/..., and any
# command that RESOLVES a path before printing it (node's resolve() inside
# `bop check`, for one) reports the /private form. Replacing the longer
# spelling first keeps a nested match from stranding a `/private` prefix.
WORK_SPELLINGS = sorted({work, os.path.realpath(work)}, key=len, reverse=True)
REPO_SPELLINGS = sorted({repo, os.path.realpath(repo)}, key=len, reverse=True)

def normalize(lines, bytes_too):
    text = "\n".join(lines)
    for spelling in WORK_SPELLINGS:
        text = text.replace(spelling, "<workdir>")
    for spelling in REPO_SPELLINGS:
        text = text.replace(spelling, "<sprefa>")
    text = re.sub(r"\b[0-9a-f]{64}\b", "<digest>", text)
    text = re.sub(r"\b[0-9a-f]{32}\b", "<program>", text)
    text = re.sub(r"\b\d{10}\b", "<epoch>", text)
    # COMPILE-TRACE (compile.pl:388) carries per-run wall/inference pairs; only
    # its shape is documentable. Scoped to that line because every gs:run command
    # ends in `echo "exit $?"` and so always exits 0, which makes the PRINTED
    # exit code the only assertion of it.
    text = re.sub(
        r"^(COMPILE-TRACE .*)$",
        lambda match: re.sub(r"=\d+/\d+", "=<n>/<n>", match.group(1)),
        text,
        flags=re.M,
    )
    if bytes_too:
        text = re.sub(r"\d+", "<n>", text)
    out = [line.rstrip() for line in text.split("\n")]
    while out and out[-1] == "":
        out.pop()
    return out

# ── replay ──────────────────────────────────────────────────────────────────
failures = 0
for number, (kind, flags, body) in enumerate(blocks, start=1):
    if kind == "write":
        target = os.path.join(work, flags[0])
        os.makedirs(os.path.dirname(target) or work, exist_ok=True)
        with open(target, "w", encoding="utf-8") as handle:
            handle.write("\n".join(body) + "\n")
        print(f"PASS  block {number}: wrote {flags[0]} ({len(body)} lines)")
        continue

    if kind != "run":
        print(f"FAIL  block {number}: unknown directive gs:{kind}")
        failures += 1
        continue

    commands = [line[2:] for line in body if line.startswith("$ ")]
    expected = [line for line in body if not line.startswith("$ ")]
    if not commands:
        print(f"FAIL  block {number}: a gs:run block with no '$ ' command")
        failures += 1
        continue

    actual, bad = [], None
    for command in commands:
        out, status = send(command)
        actual.extend(out)
        if status != 0 and bad is None:
            bad = (command, status)

    if bad is not None:
        print(f"FAIL  block {number}: `{bad[0]}` exited {bad[1]}")
        for line in actual[-12:]:
            print(f"      | {line}")
        failures += 1
        continue

    if "nodiff" in flags:
        print(f"PASS  block {number}: {len(commands)} command(s), exit 0 (nodiff)")
        continue

    want = normalize(expected, "norm=bytes" in flags)
    got = normalize(actual, "norm=bytes" in flags)
    if want == got:
        print(f"PASS  block {number}: {len(commands)} command(s), {len(got)} output line(s) match")
        continue

    print(f"FAIL  block {number}: output does not match the doc")
    for line in difflib.unified_diff(want, got, "doc", "actual", lineterm="", n=2):
        print(f"      {line}")
    failures += 1

shell.stdin.close()
shell.wait(timeout=30)

print()
if failures:
    print(f"GETTING STARTED STALE: {failures} of {len(blocks)} block(s) disagree with v6/GETTING-STARTED.md")
    sys.exit(1)
print(f"GETTING STARTED HOLDS: {len(blocks)} blocks replayed, every diffed output identical to the doc")
PY
status=$?

# The page backgrounds a `bop serve` on a pinned port and kills it in section 4;
# this is the belt-and-braces sweep for a run that failed before reaching that
# block, so a red receipt does not leave a listener behind for the next one.
pkill -f 'cli/bop.ts serve --port 17593' 2>/dev/null
exit "$status"
