#!/usr/bin/env bash
# ground-truth.sh -- grade the dead-module rail against rustc's own dead_code
# lint on a fixture whose every file is labelled with what each tool must say.
# rustc is a one-way oracle: a file it flags is certainly dead, but a `pub` item
# in a lib crate is invisible to it forever, so it can never flag dead_pub.rs.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
CRATE="$HERE/fixtures/deadcrate"
GLOB='v6/dl/deadcode/fixtures/deadcrate/src/*.rs'
SEED='v6/dl/deadcode/fixtures/deadcrate/src/lib.rs'
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ground-truth.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
rc=0
say() { printf '%-6s %-22s %s\n' "$1" "$2" "$3"; }
bad() { rc=1; say FAIL "$1" "$2"; }

( cd "$CRATE" && cargo check --quiet ) >"$WORK/rustc.txt" 2>&1 || true
grep -oE 'src/[a-z_]+\.rs' "$WORK/rustc.txt" | sed 's|src/||' | sort -u >"$WORK/rustc.set"

bash "$HERE/dead-module-rail.sh" "$ROOT" "$GLOB" "$SEED" >"$WORK/rail.txt" 2>&1 \
  || { cat "$WORK/rail.txt"; echo "FAIL   rail run"; exit 1; }
sed -n '/^== rail_unproven_module/,/^== module_reach/p' "$WORK/rail.txt" \
  | grep -oE '[A-Za-z_]+\.(rs|ts)$' | sort -u >"$WORK/unproven.set" || true
touch "$WORK/unproven.set"
sed -n '/^== rail_dead_module/,/^== rail_unreachable/p' "$WORK/rail.txt" \
  | grep -oE '[a-z_]+\.rs$' | sort -u >"$WORK/rail.set" || true
touch "$WORK/rail.set"

# file            rustc   rail    why this case exists
while read -r file want_rustc want_rail why; do
  [ -z "$file" ] && continue
  in_rustc=no; in_rail=no
  grep -qx "$file" "$WORK/rustc.set" && in_rustc=yes
  grep -qx "$file" "$WORK/rail.set" && in_rail=yes
  [ "$in_rustc" = "$want_rustc" ] || bad "$file" "rustc said $in_rustc, label says $want_rustc"
  [ "$in_rail" = "$want_rail" ] || bad "$file" "rail said $in_rail, label says $want_rail"
  [ "$in_rustc$in_rail" = "$want_rustc$want_rail" ] && say ok "$file" "$why"
done <<'LABELS'
dead_private.rs      yes yes both see a private module nothing calls
dead_trait_impls.rs  yes yes both see a trait whose name no call site uses
dead_pub.rs          no  yes pub in a lib crate is invisible to rustc forever
live_pub.rs          no  no  called from the crate root
live_private.rs      no  no  called from the crate root
live_trait_impls.rs  no  no  one dyn call must reach every impl of the trait
lib.rs               no  no  the crate root is a declared seed
ambiguous_owner.rs   no  no  a receiver call carries no callee_path, only the name
ambiguous_other.rs   no  no  the second refresh, which makes the name ambiguous
test_only_defs.rs    no  no  five cfg(test) defs must not count toward the floor
called_only_from_tests.rs yes yes its only caller sits in another file's cfg(test) mod
mixed_call_sites.rs  no  no  one shipped site and one test site name the same pair
LABELS

# rustc's findings are a subset of the rail's by construction; a file rustc
# flags that the rail misses is a false negative and the sharper failure.
missed="$(comm -23 "$WORK/rustc.set" "$WORK/rail.set" || true)"
[ -z "$missed" ] || bad "subset" "rustc flagged but rail missed: $(echo $missed)"

# The third bucket is the rail's honesty about what call-family data cannot
# decide. A file reached only through a name several files define is neither
# proven live nor proven dead; asserting it keeps the ambiguity from quietly
# collapsing into either answer.
for file in ambiguous_owner.rs ambiguous_other.rs; do
  if grep -qx "$file" "$WORK/unproven.set"; then
    say ok "$file" "unproven: refresh names two files, so no call proves either"
  else
    bad "$file" "expected in rail_unproven_module, absent"
  fi
done

[ "$rc" = 0 ] && echo "GROUND-TRUTH OK  rustc=$(wc -l <"$WORK/rustc.set" | tr -d ' ') rail=$(wc -l <"$WORK/rail.set" | tr -d ' ')"
exit "$rc"
