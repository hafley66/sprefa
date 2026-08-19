#!/usr/bin/env bash
set -euo pipefail

# `just build-dl6c`'s executable, copied to a temp dir and run there with PATH
# stripped of swipl, emits the bytes compile_dl6/3 does.

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
prolog="$repo/v6/prolog"

# One with `use`, one with `sh`, one with anonymous types. golden-flex.dl6 is
# NOT here: it `use`s 0_golden-flex-imported.dl6, which is absent from git.
fixtures=(source-mutations resident-coroutine anonymous-type-syntax type-name-module-prefix)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

just --justfile "$repo/justfile" --working-directory "$repo" build-dl6c >/dev/null
cp "$prolog/target/dl6c" "$work/dl6c"
cp -R "$repo/v6/dl/fixtures" "$work/fixtures"
mkdir -p "$work/out" "$work/reference"

failures=0
report() {
  if [ "$1" = 0 ]; then
    echo "DL6C-ROUNDTRIP ok   $2"
  else
    echo "DL6C-ROUNDTRIP FAIL $2"
    failures=$((failures + 1))
  fi
}

sha="$(git -C "$repo" rev-parse --short HEAD)"
version="$("$work/dl6c" --version)"
[ "$version" = "dl6c $sha" ] && report 0 "--version prints $sha" || report 1 "--version printed '$version', wanted 'dl6c $sha'"

# cwd is the temp dir and every path is relative to it, so a state that still
# needed the v6/prolog checkout would fail here rather than pass silently.
run_dl6c() {
  ( cd "$work" && PATH=/usr/bin:/bin ./dl6c "$1" --target "$2" --out "$3" )
}

for fixture in "${fixtures[@]}"; do
  for target in ts rust; do
    case "$target" in
      ts)   extension=ts; emitter="" ;;
      rust) extension=rs; emitter="-l $prolog/emit_rust.pl" ;;
    esac
    case "$target" in
      ts)   goal="compile_dl6('$repo/v6/dl/fixtures/$fixture.dl6','$work/reference/$fixture.ts')" ;;
      rust) goal="compile_dl6('$repo/v6/dl/fixtures/$fixture.dl6','$work/reference/$fixture.rs',[emitter(emit_rust:emit_program)])" ;;
    esac
    # shellcheck disable=SC2086
    swipl -q -l "$prolog/compile.pl" $emitter -g "$goal" -g halt >/dev/null 2>&1
    run_dl6c "fixtures/$fixture.dl6" "$target" out >/dev/null 2>&1
    if cmp -s "$work/out/$fixture.$extension" "$work/reference/$fixture.$extension"; then
      report 0 "$fixture --target $target byte-identical"
    else
      report 1 "$fixture --target $target differs from compile_dl6/3"
    fi
  done
done

printf 'rel person(name: text, age: treee).\n' >"$work/fixtures/dl6c-unsupported.dl6"
set +e
unsupported_text="$(run_dl6c fixtures/dl6c-unsupported.dl6 ts out 2>&1 >/dev/null)"
unsupported_code=$?
set -e
[ "$unsupported_code" = 2 ] && report 0 "unsupported construct exits 2" || report 1 "unsupported construct exited $unsupported_code, wanted 2"
case "$unsupported_text" in
  *column_type_unknown*) report 0 "unsupported construct names column_type_unknown" ;;
  *) report 1 "unsupported construct text named nothing: $unsupported_text" ;;
esac

set +e
( cd "$work" && PATH=/usr/bin:/bin ./dl6c fixtures/source-mutations.dl6 --out out ) >/dev/null 2>&1
missing_target_code=$?
set -e
[ "$missing_target_code" = 1 ] && report 0 "missing --target exits 1" || report 1 "missing --target exited $missing_target_code, wanted 1"

echo "DL6C-ROUNDTRIP failures=$failures"
[ "$failures" = 0 ]
