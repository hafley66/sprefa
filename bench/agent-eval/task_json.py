#!/usr/bin/env python3
"""task_json.py — tiny JSON field helpers for mutate.sh (kept out of shell
quoting: an inline `python3 -c` embedding f-strings with escaped double
quotes inside a bash single-quoted heredoc is a quoting minefield; a real
file is not).

  task_json.py fields <task.json>
      Prints "<id>\\t<mutation_kind>\\t<site_path>\\t<site_line>".

  task_json.py write <task.json> <out.json> <worktree_root> <answer_file> <answer_line>
      Loads <task.json>, sets expected_json = {"file": answer_file,
      "line": int(answer_line)} and worktree_root, writes <out.json>.
"""
import json
import sys


def cmd_fields(task_path):
    t = json.load(open(task_path))
    print(f"{t['id']}\t{t['mutation_kind']}\t{t['site_path']}\t{t['site_line']}")


def cmd_write(task_path, out_path, worktree_root, answer_file, answer_line):
    task = json.load(open(task_path))
    task["expected_json"] = {"file": answer_file, "line": int(answer_line)}
    task["worktree_root"] = worktree_root
    json.dump(task, open(out_path, "w"), indent=2)


def main():
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    verb, rest = sys.argv[1], sys.argv[2:]
    if verb == "fields" and len(rest) == 1:
        cmd_fields(*rest)
    elif verb == "write" and len(rest) == 5:
        cmd_write(*rest)
    else:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)


if __name__ == "__main__":
    main()
