set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$here/../.."
cd "$root"
for f in lower emit_ts analyze 0_type_plane ARCH compile 0_program_check print_dl 0_dot_expand; do
  swipl -g main -t halt "$here/predmap.pl" -- "v6/prolog/$f.pl" > "$here/$f.predmap.json"
done
swipl -g main -t halt "$here/predmap.pl" -- v6/prolog/compile/parse_dl_dcg.pl \
  > "$here/parse_dl_dcg.predmap.json"
cd "$here"
for c in cuts/*.cuts.json; do
  name="$(basename "$c" .cuts.json)"
  python3 partition.py report "$c" > "reports/$name.md"
done
cat reports/*.md > receipts.md
echo "receipts.md $(wc -l < receipts.md) lines"
python3 mkheads.py > heads.md
echo "heads.md $(wc -l < heads.md) lines"
