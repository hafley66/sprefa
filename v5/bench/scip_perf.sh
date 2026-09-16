#!/usr/bin/env bash
# scip_perf.sh — measure SCIP cost across checkouts and validate the
# incrementality assumptions behind the OID-share / per-Document-ingest plan.
#
# Validates four claims, each as a numbered experiment:
#   1. GEN cost is per-rev full-pass (cold every run; no incremental for the CLI).
#   2. Two checkouts at the SAME commit produce indexes that differ ONLY by the
#      embedded absolute project root  => OID-shareable after path normalization.
#   3. A BODY edit leaves the symbol/moniker set unchanged (delta 0); an API edit
#      (rename) changes only that symbol + its referrers  => bounded blast radius.
#   4. dl INGEST cost << GEN cost  => re-ingest is cheap, re-GENERATION is the bill.
#
# Usage:
#   bench/scip_perf.sh ts   <target-dir>     # scip-typescript
#   bench/scip_perf.sh rust <crate-dir>      # rust-analyzer scip
#   bench/scip_perf.sh py   <target-dir>     # scip-python
#
# Env: DL = path to the dl binary (default target/debug/dl).
set -uo pipefail

KIND="${1:?ts|rust|py}"; TGT="${2:?target dir}"
DL="${DL:-$(cd "$(dirname "$0")/.." && pwd)/target/debug/dl}"
OUT="$(mktemp -d)"; echo "[scip_perf] kind=$KIND target=$TGT scratch=$OUT"

# indexer command + a printable-symbol extractor per ecosystem.
case "$KIND" in
  ts)   IDX=(scip-typescript index --output index.scip); GREP='scip-typescript' ;;
  py)   IDX=(scip-python index . --output index.scip);    GREP='scip-python' ;;
  rust) IDX=(rust-analyzer scip . --output index.scip);   GREP='rust-analyzer cargo' ;;
  *) echo "unknown kind $KIND"; exit 2 ;;
esac
gen()  { ( cd "$1" && /usr/bin/time -p "${IDX[@]}" >/dev/null 2>>"$OUT/gen.t" ); }
syms() { strings "$1/index.scip" | grep -F "$GREP" | sort; }

# --- 0: cold vs warm IN PLACE (same dir, no cp) -----------------------------
# The batch CLI does NOT reuse a salsa cache across launches, so a second pass
# in the same dir is ~same cost as the first. (Warm RA = the LSP SERVER, not
# this CLI.) Measured on v5: cold 11.37s, "warm" 2nd pass 10.59s, byte-identical.
W0="$OUT/W0"; cp -R "$TGT" "$W0"; : >"$OUT/cw.t"
gen "$W0"; cp -f "$W0/index.scip" "$OUT/c0.scip"
gen "$W0"
echo "== (0) cold vs warm, same dir (CLI has no warm path; warm RA = the server) =="; grep -E 'real|user' "$OUT/cw.t"
cmp -s "$OUT/c0.scip" "$W0/index.scip" && echo "  same-dir same-state: byte-identical" || echo "  same-dir same-state: differ (unexpected)"

# --- 1+2: two checkouts at the same state -----------------------------------
A="$OUT/A"; B="$OUT/B"; cp -R "$TGT" "$A"; cp -R "$TGT" "$B"
: >"$OUT/gen.t"; gen "$A"; gen "$B"
echo "== (1) GEN time (real/user/sys), each a full cold pass =="; grep -E 'real|user|sys' "$OUT/gen.t"
echo "== index size =="; ls -la "$A/index.scip" | awk '{print $5" bytes"}'
echo "== (2) cross-checkout determinism: non-path string diffs (expect 0) =="
# strip the embedded absolute roots, then compare symbol+string content.
sed "s#$A##g" <(syms "$A") > "$OUT/sa"; sed "s#$B##g" <(syms "$B") > "$OUT/sb"
N=$(diff "$OUT/sa" "$OUT/sb" | grep -cE '^[<>]'); echo "  symbol diffs after root-strip: $N  (0 => OID-shareable modulo path)"

# --- 3: blast radius (needs a body-editable + a renameable symbol) ----------
# Caller passes SCIP_BODY_SED / SCIP_API_SED to describe the two edits, e.g.
#   SCIP_BODY_SED='s#/old#/new#'  SCIP_API_SED='s/\bfoo\b/foo2/g'
W="$OUT/W"; cp -R "$TGT" "$W"; : >"$OUT/re.t"
( cd "$W" && /usr/bin/time -p "${IDX[@]}" >/dev/null 2>>"$OUT/re.t" ); syms "$W" >"$OUT/s.base"
if [ -n "${SCIP_BODY_SED:-}" ]; then
  grep -rl . "$W" --include='*.ts' --include='*.py' --include='*.rs' | xargs perl -pi -e "$SCIP_BODY_SED" 2>/dev/null
  ( cd "$W" && "${IDX[@]}" >/dev/null 2>&1 ); syms "$W" >"$OUT/s.body"
  echo "== (3a) BODY edit symbol-set delta (expect 0) =="; diff "$OUT/s.base" "$OUT/s.body" | grep -cE '^[<>]'
fi
if [ -n "${SCIP_API_SED:-}" ]; then
  grep -rl . "$W" --include='*.ts' --include='*.py' --include='*.rs' | xargs perl -pi -e "$SCIP_API_SED" 2>/dev/null
  ( cd "$W" && "${IDX[@]}" >/dev/null 2>&1 ); syms "$W" >"$OUT/s.api"
  echo "== (3b) API edit symbol-set delta (renamed sym + referrers only) =="; diff "$OUT/s.base" "$OUT/s.api" | grep -cE '^[<>]'
fi

# --- 4: dl ingest cost vs generation ----------------------------------------
printf '%s\n' 'seen(s) <- scip_def(s, f, _).' '? seen(s).' > "$OUT/ingest.dl"
echo "== (4) dl INGEST time of the generated index (compare to GEN above) =="
SPREFA_SCIP_INDEX="$A/index.scip" /usr/bin/time -p "$DL" "$OUT/ingest.dl" --root "$A" --no-daemon 2>"$OUT/in.t" | grep -E 'rows'
grep -E 'real|user' "$OUT/in.t"
echo "[scip_perf] scratch kept at $OUT"
