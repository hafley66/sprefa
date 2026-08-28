#!/usr/bin/env bash
# Preflight for a coordinator session or a lane dispatch. Prints PASS/WARN/FAIL
# rows with raw numbers; never deletes anything. Exit 1 on any FAIL.
set -u
repo=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "FAIL git: not in a repository"; exit 1; }
cd "$repo"
rc=0
row() { printf '%-4s %-10s %s\n' "$1" "$2" "$3"; [ "$1" = FAIL ] && rc=1; return 0; }

# 1. disk
avail_gb=$(df -Pk . | awk 'NR==2{printf "%d", $4/1048576}')
if [ "$avail_gb" -lt 15 ]; then
  row FAIL disk "${avail_gb}G free under $repo (floor 15G)"
  du -sg "$repo"/target "$repo"/v6/*/target 2>/dev/null | sort -rn | head -5 | sed 's/^/       /'
else
  row PASS disk "${avail_gb}G free"
fi

# 2. git divergence, main and every worktree
git fetch -q origin 2>/dev/null || row WARN fetch "origin unreachable, numbers below are stale"
while read -r path _ branch; do
  branch=${branch#[}; branch=${branch%]}
  [ -z "$branch" ] && continue
  ab=$(git -C "$path" rev-list --left-right --count "origin/main...HEAD" 2>/dev/null) || continue
  behind=${ab%%	*}; ahead=${ab##*	}
  dirty=$(git -C "$path" status --porcelain | wc -l | tr -d ' ')
  if [ "$branch" = main ] && [ "$ahead" -gt 0 ]; then
    row FAIL main "local main is $ahead ahead of origin/main ($behind behind); branching from HEAD pollutes PRs"
  elif [ "$dirty" -gt 0 ]; then
    row WARN wt "$path [$branch] ahead=$ahead behind=$behind dirty=$dirty"
  else
    row PASS wt "$path [$branch] ahead=$ahead behind=$behind"
  fi
done < <(git worktree list)

# 3. worktrees whose branch is already merged on origin/main
while read -r path _ branch; do
  branch=${branch#[}; branch=${branch%]}
  [ -z "$branch" ] || [ "$branch" = main ] && continue
  if git merge-base --is-ancestor "$branch" origin/main 2>/dev/null; then
    row WARN merged "$path [$branch] tip is on origin/main (merged or never started); git worktree remove $path"
  fi
done < <(git worktree list)

# 4. lane runners and tmux
if command -v boop >/dev/null; then
  lanes=$(boop beep lane list 2>/dev/null | tail -n +2 | wc -l | tr -d ' ')
  row PASS lanes "$lanes boop lanes listed"
fi
if command -v tmux >/dev/null; then
  old=$(tmux list-sessions -F '#{session_name} #{session_created}' 2>/dev/null | awk -v now="$(date +%s)" '$2 < now-6*3600 {print $1}' | tr '\n' ' ')
  [ -n "$old" ] && row WARN tmux "sessions older than 6h: $old" || row PASS tmux "no session older than 6h"
fi

# 5. processes
hogs=$(ps -Ao pid,rss,etime,comm | awk 'NR>1 && $2 > 2000000 {printf "%s(%dMB,%s) ", $4, $2/1024, $3}')
[ -n "$hogs" ] && row WARN procs "over 2GB RSS: $hogs" || row PASS procs "no process over 2GB RSS"

exit $rc
