#!/usr/bin/env bash
# install-dl.sh — turnkey: put `dl` on PATH and wire it into AI coding agents
# (Claude Code, opencode, anything that reads AGENTS.md / a skills dir).
#
# Usage:
#   install-dl.sh                 # install binary + wire global skill into detected agents
#   install-dl.sh --bin-only      # just `cargo install` the binary
#   install-dl.sh --project DIR   # bootstrap a repo: starter .dl + AGENTS.md/CLAUDE.md section
#   install-dl.sh --project .     # ...the current repo
#
# Env: SPREFA_SKILLS_DIR overrides skill destination.
set -euo pipefail

V5="$(cd "$(dirname "$0")" && pwd)"            # the repo root (crate root)
SKILL_SRC="$HOME/projects/claude-research/skills/sprefa-dl/SKILL.md"
say() { printf '\033[1;36m[install-dl]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[install-dl]\033[0m %s\n' "$*"; }

# ── binary ───────────────────────────────────────────────────────────────────
install_bin() {
  say "cargo install dl (--path $V5)"
  cargo install --path "$V5" --bin dl --force
  command -v dl >/dev/null && say "dl on PATH: $(command -v dl)" \
    || warn "dl not on PATH — add \$HOME/.cargo/bin to PATH"
}

# ── skill destination: opencode's first configured path > claude-research >
#    a dir dl owns. Both Claude Code and opencode are then pointed at it.
resolve_skills_dir() {
  if [ -n "${SPREFA_SKILLS_DIR:-}" ]; then echo "$SPREFA_SKILLS_DIR"; return; fi
  local oc="$HOME/.config/opencode/opencode.json"
  if [ -f "$oc" ] && command -v jq >/dev/null 2>&1; then
    local p; p=$(jq -r '.skills.paths[0] // empty' "$oc" 2>/dev/null)
    [ -n "$p" ] && [ -d "$p" ] && { echo "$p"; return; }
  fi
  for d in "$HOME/projects/claude-research/skills" "$HOME/.claude/skills"; do
    [ -d "$d" ] && { echo "$d"; return; }
  done
  echo "$HOME/.config/sprefa/skills"
}

wire_skill() {
  local dest; dest="$(resolve_skills_dir)"
  mkdir -p "$dest/sprefa-dl"
  if [ -f "$SKILL_SRC" ]; then cp -f "$SKILL_SRC" "$dest/sprefa-dl/SKILL.md"
  else warn "skill source missing at $SKILL_SRC (skipping skill copy)"; fi
  say "skill at $dest/sprefa-dl/SKILL.md"

  # Claude Code: symlink into ~/.claude/skills if that isn't already the dest.
  local cc="$HOME/.claude/skills"
  if [ -e "$cc" ] && [ "$(cd "$cc" && pwd)" != "$dest" ]; then
    ln -sfn "$dest/sprefa-dl" "$cc/sprefa-dl" 2>/dev/null \
      && say "linked into Claude Code skills ($cc/sprefa-dl)" || true
  fi
  # opencode: ensure the dir is in skills.paths.
  local oc="$HOME/.config/opencode/opencode.json"
  if [ -f "$oc" ]; then
    if grep -qF "$dest" "$oc"; then say "opencode already reads $dest"
    else warn "add to $oc:  \"skills\": { \"paths\": [\"$dest\"] }"; fi
  fi
  command -v claude   >/dev/null 2>&1 && say "detected: claude"   || true
  command -v opencode >/dev/null 2>&1 && say "detected: opencode" || true
}

# ── per-project bootstrap: agent-agnostic (AGENTS.md + CLAUDE.md both read by
#    opencode and Claude Code from the repo) + a runnable starter rail. ────────
bootstrap_project() {
  local dir; dir="$(cd "$1" && pwd)"
  say "bootstrap $dir"
  mkdir -p "$dir/.dl"
  local starter="$dir/.dl/dl-self-lint.dl"
  if [ ! -f "$starter" ]; then
    cat > "$starter" <<'DL'
# dl-self-lint.dl — discovery rail: dl validates the repo's own .dl files.
# `dl --check` (or the daemon) runs this; exit 2 on any broken/typed-wrong .dl.
# dl_diag runs the engine's own lex/parse/typecheck (like rust-analyzer for .dl).

# git-free "what did the agent change" — reads the harness store, keyed on --root.
rel agent_changed(path: file).
agent_changed(path) <- agent_touch(_, _, path).

# scan so `file` carries .dl sources for dl_diag to validate.
rel dl_file(path: file).
dl_file(path) <- scan("**/*.dl", path, rev).

# the rail: a parse/type error in a .dl file the agent just edited.
rel diag(path: file, line: int, col: int, end_line: int, end_col: int,
         severity: text, code: text, msg: text).
diag(p, l, c, el, ec, sev, code, msg) <-
    agent_changed(p), p =~ /\.dl$/, dl_diag(p, l, c, el, ec, sev, code, msg).
DL
    say "wrote $starter"
  fi
  append_section "$dir/AGENTS.md"
  append_section "$dir/CLAUDE.md"
  say "run it:  (cd $dir && dl --check)"
}

append_section() {
  local f="$1"
  [ -e "$f" ] && grep -qF 'BEGIN: sprefa-dl' "$f" && { say "section already in $(basename "$f")"; return; }
  cat >> "$f" <<'MD'

<!-- BEGIN: sprefa-dl -->
## Querying this codebase with `dl`

`dl` (sprefa v5) is datalog over code. Reach for it instead of grep when you need
structured facts: call graph, import/type graph, blast radius, lint rails, codemods.

- Run a program: `dl prog.dl --root .` (prints `?` rows). `--no-daemon` for ad-hoc.
- Discovery rail: `dl --check` runs every `.dl/*.dl`; exits 2 on a `diag` row.
- The `.dl/dl-self-lint.dl` rail makes a broken/mistyped `.dl` a `--check` failure
  (the engine lints `.dl` via the built-in `dl_diag` relation, like rust-analyzer).
- Surface reference: see the engine's generated `docs/reference/{relations,functions,syntax,examples}.md`.
- `agent_edit`/`agent_touch` are git-free (keyed on `--root` dir); `changed`/`created` need git.

See the `sprefa-dl` skill for the full surface and authoring gotchas.
<!-- END: sprefa-dl -->
MD
  say "appended dl section to $(basename "$f")"
}

# ── dispatch ──────────────────────────────────────────────────────────────────
case "${1:-all}" in
  --bin-only) install_bin ;;
  --project)  bootstrap_project "${2:?--project needs a DIR}" ;;
  all|"")     install_bin; wire_skill ;;
  *) echo "usage: install-dl.sh [--bin-only | --project DIR]"; exit 2 ;;
esac
say "done."
