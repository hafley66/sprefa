#!/usr/bin/env bash
# ghcacher_vs_dl.sh — a repeatable head-to-head: the dl ghcacher port
# (examples/gh-cache.dl shape) vs the real ghcacher service, both caching the
# SAME GitHub org at its real repo cardinality. SERIAL by design (dl fully, then
# ghcacher) so neither the machine nor GitHub is hammered in parallel.
#
# What it measures: requests issued per refresh and the conditional-cache (304)
# discipline, read from each tool's own logs. NOTE: in a proxied environment
# `gh api` may not decrement the core rate limit (used=0 / rest_remaining=null) —
# so request COUNT + 304 ratio is the honest currency, not a rate-limit delta.
#
# Usage:
#   bench/ghcacher_vs_dl.sh                       # default: kubernetes org, all repos
#   ORG=apache bench/ghcacher_vs_dl.sh            # another org
#   MAX_REPOS=3 bench/ghcacher_vs_dl.sh           # smoke test the dl path (caps dl only;
#                                                 # ghcacher is org-level and still syncs ALL repos,
#                                                 # so apples-to-apples needs MAX_REPOS=0)
#   CLOCK_SECS=30 RUN_SECS=80 bench/ghcacher_vs_dl.sh
#   GHCACHE=~/projects/ghcacher bench/ghcacher_vs_dl.sh   # ghcacher checkout (else dl-only)
set -euo pipefail

ORG="${ORG:-kubernetes}"
MAX_REPOS="${MAX_REPOS:-0}"          # 0 = all of the org's public repos
CLOCK_SECS="${CLOCK_SECS:-30}"        # the dl re-poll cadence (one conditional req / endpoint / N s)
RUN_SECS="${RUN_SECS:-80}"            # how long to run the dl daemon (spans a few clock buckets)
POLL_SECS="${POLL_SECS:-3}"           # dl daemon tick cadence (drives the effect drain)
GHCACHE="${GHCACHE:-$HOME/projects/ghcacher}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"      # v5/
DL="${DL:-$HERE/target/debug/dl}"

WORK="$(mktemp -d "/tmp/ghc-vs-dl.${ORG}.XXXX")"
ROOT="$WORK/root"; mkdir -p "$ROOT"; git -C "$ROOT" init -q 2>/dev/null || true
DLDB="$WORK/dl.db"
DAEMON_PID=""
cleanup() { [ -n "$DAEMON_PID" ] && kill "$DAEMON_PID" 2>/dev/null || true; "$DL" --stop --root "$ROOT" 2>/dev/null || true; }
trap cleanup EXIT

hr() { printf '\n\033[1;34m══ %s\033[0m\n' "$*"; }
sq() { sqlite3 "$1" "$2" 2>/dev/null; }

[ -x "$DL" ] || { echo "build dl first: (cd $HERE && cargo build --bin dl)"; exit 1; }

# ── 1. Enumerate the org's repos into a watch set ───────────────────────────
hr "Enumerate $ORG"
gh api "orgs/$ORG/repos?per_page=100&type=public" --jq '.[].full_name' | sort > "$WORK/repos.txt"
[ "$MAX_REPOS" -gt 0 ] && { head -n "$MAX_REPOS" "$WORK/repos.txt" > "$WORK/repos.cap"; mv "$WORK/repos.cap" "$WORK/repos.txt"; }
N=$(wc -l < "$WORK/repos.txt" | tr -d ' ')
echo "watch set: $N repos (repos/$ORG/*)"

# ── 2. Generate the dl program — the FULL gh-cache-full.dl shape per repo:
#      a conditional GET (repo metadata, 304-cached) AND a paginated PR list,
#      so the comparison matches ghcacher's sync_prs+sync_branches sweep.
{
  echo 'rel watch(ep: text).'
  echo 'rel watch_list(ep: text, kind: text).'
  while read -r r; do
    echo "watch(\"repos/$r\")."
    echo "watch_list(\"repos/$r/pulls\", \"pull_request\")."
  done < "$WORK/repos.txt"
  cat <<DL
# --- conditional single-GET (repo metadata): etag/304 cache ---
rel etag(ep: text, tag: text).
rel etag_next(ep: text, tag: text).
rel poll(ep: text, prev: text, bucket: int).
poll(ep, prev, b) <- watch(ep), etag(ep, prev), clock($CLOCK_SECS, b).
poll(ep, "",   b) <- watch(ep), !etag(ep, _),   clock($CLOCK_SECS, b).
sh fetch(ep, prev) -> (status: int, tag: text, body: text) =
  \`R=\$(gh api {ep} -i -H "If-None-Match: \$prev" 2>/dev/null)
   C=\$(printf '%s' "\$R" | head -1 | grep -oE '[0-9]{3}' | head -1)
   E=\$(printf '%s' "\$R" | grep -iE '^etag:' | head -1 | sed -E 's/^[Ee]tag:[[:space:]]*//; s/\r\$//')
   B=\$(printf '%s' "\$R" | awk 'f{print} /^\r?\$/{f=1}' | tr -d '\n')
   printf '%s\n%s\n%s' "\$C" "\$E" "\$B"\`.
rel resp(ep: text, status: int, tag: text, body: text).
resp(ep, status, tag, body) <- @async poll(ep, prev, bucket), fetch(ep, prev) -> (status, tag, body).
etag_next(ep, tag) <- resp(ep, 200, tag, _).
etag_next(ep, old) <- resp(ep, 304, _, _), etag(ep, old).
etag(ep, tag) <- @next etag_next(ep, tag).
rel stars(ep: text, n: text).
stars(ep, n) <- resp(ep, 200, _, body), jsonp(body, "stargazers_count", n).
# --- paginated PR list (gap C): gh follows Link rel="next"; jq merges pages ---
rel list_poll(ep: text, kind: text, bucket: int).
list_poll(ep, kind, b) <- watch_list(ep, kind), clock($CLOCK_SECS, b).
sh list_fetch(ep) -> (body: text) =
  \`gh api --paginate {ep} 2>/dev/null | jq -s 'add // .' 2>/dev/null\`.
rel list_resp(ep: text, kind: text, tx: int, body: text).
list_resp(ep, kind, bucket, body) <- @async list_poll(ep, kind, bucket), list_fetch(ep) -> (body).
# --- normalize + upsert (gap B): one brace pattern over the merged array,
#     latest-wins = max(tx) per number joined back ---
rel pr_obs(ep: text, num: text, title: text, state: text, tx: int).
pr_obs(ep, num, title, state, tx) <-
    list_resp(ep, "pull_request", tx, body),
    json(body, q:[... { number: \$num, title: \$title, state: \$state } ]).
rel pr_latest(ep: text, num: text, tx: int).
pr_latest(ep, num, max(tx)) <- pr_obs(ep, num, _, _, tx).
rel pull_request(ep: text, num: text, title: text, state: text).
pull_request(ep, num, title, state) <-
    pr_latest(ep, num, tx), pr_obs(ep, num, title, state, tx).
? stars(ep, n).
? pull_request(ep, num, title, state).
DL
} > "$WORK/cache.dl"
"$DL" --check "$WORK/cache.dl" >/dev/null 2>&1 || { echo "generated dl program failed --check"; "$DL" --check "$WORK/cache.dl"; exit 1; }

# ── 3. Run the dl daemon: cold start + warm re-polls across clock buckets ────
hr "dl port: daemon ${RUN_SECS}s (clock=${CLOCK_SECS}s, tick=${POLL_SECS}s)"
DL_POLL_SECS="$POLL_SECS" "$DL" --daemon --root "$ROOT" --db "$DLDB" "$WORK/cache.dl" >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep "$RUN_SECS"
"$DL" --stop --root "$ROOT" 2>/dev/null || kill "$DAEMON_PID" 2>/dev/null || true
DAEMON_PID=""
sleep 1

DL_CACHED=$(sq "$DLDB" "SELECT count(*) FROM rel_stars;")
DL_PRS=$(sq "$DLDB" "SELECT count(*) FROM rel_pull_request;")
DL_REQS=$(sq "$DLDB" "SELECT count(*) FROM pending_effect;")
DL_200=$(sq "$DLDB" "SELECT count(*) FROM rel_resp WHERE status=200;")
DL_304=$(sq "$DLDB" "SELECT count(*) FROM rel_resp WHERE status=304;")
echo "repos cached: ${DL_CACHED}/${N} | PRs normalized: ${DL_PRS} | distinct requests: ${DL_REQS} | resp 200: ${DL_200} | resp 304: ${DL_304}"
echo "requests per clock bucket (request rate tracks cardinality, not tick rate):"
sq "$DLDB" "SELECT json_extract(args_json,'\$.bucket') AS bucket, count(*) AS reqs FROM pending_effect GROUP BY bucket ORDER BY bucket;"

# ── 4. Run ghcacher (serially, after dl) on the SAME org, two syncs ──────────
GHC_BIN="$GHCACHE/target/debug/ghcache"
if [ -d "$GHCACHE" ]; then
  hr "ghcacher: build + two syncs on $ORG (serial, after dl)"
  (cd "$GHCACHE" && cargo build -q 2>/dev/null) || true
  if [ -x "$GHC_BIN" ]; then
    GD="$WORK/ghc"; mkdir -p "$GD/staging"
    cat > "$GD/config.toml" <<TOML
[global]
db_path               = "$GD/gh.db"
staging_folder        = "$GD/staging"
poll_interval_seconds = $CLOCK_SECS
log_level             = "warn"
gh_binary             = "gh"

[[org]]
owner         = "$ORG"
sync_prs      = true
sync_events   = true
sync_branches = ["main"]
TOML
    "$GHC_BIN" --config "$GD/config.toml" sync >/dev/null 2>&1 || true
    GHC_S1=$(sq "$GD/gh.db" "SELECT count(*) FROM call_log;")
    "$GHC_BIN" --config "$GD/config.toml" sync >/dev/null 2>&1 || true
    GHC_TOT=$(sq "$GD/gh.db" "SELECT count(*) FROM call_log;")
    GHC_S2=$((GHC_TOT - GHC_S1))
    GHC_REPOS=$(sq "$GD/gh.db" "SELECT count(*) FROM repo;")
    GHC_PRS=$(sq "$GD/gh.db" "SELECT count(*) FROM pull_request;" 2>/dev/null || echo "?")
    GHC_ETAGS=$(sq "$GD/gh.db" "SELECT count(*) FROM poll_state WHERE etag IS NOT NULL;")
    GHC_S2_304=$(sq "$GD/gh.db" "SELECT count(*) FROM (SELECT status_code FROM call_log ORDER BY id DESC LIMIT $GHC_S2) WHERE status_code=304;")
    echo "repos: ${GHC_REPOS} | PRs: ${GHC_PRS} | sync#1 calls: ${GHC_S1} | sync#2 calls: ${GHC_S2} (304s: ${GHC_S2_304}) | etags stored: ${GHC_ETAGS}"
  else
    echo "ghcache build missing; skipping"
  fi
else
  echo "(no ghcacher checkout at $GHCACHE; set GHCACHE=... for the head-to-head; dl-only run)"
fi

# ── 5. Verdict table ────────────────────────────────────────────────────────
hr "Summary ($ORG, $N repos, clock=${CLOCK_SECS}s — MATCHED feature set: repo + PRs)"
echo "dl port    : ${DL_CACHED} repos + ${DL_PRS:-?} PRs cached | repo re-poll = 304 (conditional, every endpoint) | paginated PR list = 200 | ~req/bucket, decoupled from tick rate"
if [ -x "$GHC_BIN" ]; then
echo "ghcacher   : ${GHC_REPOS:-?} repos + ${GHC_PRS:-?} PRs | branch/PR/event re-fetch = 200 each sync (conditional cache only on its events/notifications path)"
fi
echo "rate-limit consumed: unobservable here (gh proxy: used=0 / rest_remaining=null for BOTH) — compare request count + 304 ratio."
echo "artifacts: $WORK"
trap - EXIT   # keep artifacts for inspection
