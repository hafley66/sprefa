#!/usr/bin/env bash

# Builds dist-local/: the v6 toolchain a coworker installs, minus
# sprefa-extract (a cargo-dist artifact, named in install.sh's output text).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$REPO/dist-local"
TSV2="$REPO/v6/tsv2"
ESBUILD="$TSV2/node_modules/.bin/esbuild"

for tool in swipl install_name_tool otool codesign node; do
  command -v "$tool" >/dev/null || { echo "release-local: missing $tool" >&2; exit 1; }
done
[ -x "$ESBUILD" ] || { echo "release-local: run 'cd $TSV2 && pnpm install' first" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$OUT"

# ── 1. dl6c: the compiler as a stand-alone swipl saved state ─────────────────

eval "$(swipl --dump-runtime-variables)"
SWIPL_LIB_DIR="$PLLIBDIR"
EMULATOR="$OUT/dl6c-emulator"

resolve_dep() {
  case "$1" in
    @rpath/*) printf '%s/%s\n' "$SWIPL_LIB_DIR" "${1#@rpath/}" ;;
    *) printf '%s\n' "$1" ;;
  esac
}

relocate() {
  local target="$1" anchor="$2" dep base
  for dep in $(otool -L "$target" | tail -n +2 | awk '{ print $1 }'); do
    case "$dep" in
      @rpath/*|/opt/homebrew/*) ;;
      *) continue ;;
    esac
    base="$(basename "$dep")"
    if [ ! -f "$OUT/$base" ]; then
      cp -L "$(resolve_dep "$dep")" "$OUT/$base"
      chmod u+w "$OUT/$base"
      install_name_tool -id "@loader_path/$base" "$OUT/$base"
      relocate "$OUT/$base" "@loader_path"
    fi
    install_name_tool -change "$dep" "$anchor/$base" "$target" 2>/dev/null
  done
  install_name_tool -delete_rpath "$SWIPL_LIB_DIR" "$target" 2>/dev/null || true
  codesign --force --sign - "$target" 2>/dev/null
}

STUB="$(mktemp -t dl6c_stub.XXXXXX).pl"
trap 'rm -f "$STUB"' EXIT
cat > "$STUB" <<PROLOG
:- use_module(library(main)).
:- asserta((user:file_search_path(foreign, Dir) :-
              current_prolog_flag(executable, Exe),
              file_directory_name(Exe, Dir))).
:- use_module('$REPO/v6/prolog/compile.pl').
main([In, Out]) :- !, compile_dl6(In, Out), halt(0).
main(_) :- format(user_error, "usage: dl6c IN.dl6 OUT.ts~n", []), halt(2).
list_foreign :- forall(shlib:current_library(_, _, F, _, _), (print(F), nl)), halt(0).
PROLOG

if [ "${SPREFA_BUNDLE_SWIPL:-1}" = 0 ]; then
  swipl -q --stand_alone=true -o "$OUT/dl6c" -g main -c "$STUB"
else
  # The state re-runs every loaded module's use_foreign_library at restore, so
  # the .so set is read off a probe state rather than guessed.
  swipl -q --stand_alone=true -o "$OUT/dl6c-probe" -g list_foreign -c "$STUB"
  FOREIGN_LIBS="$("$OUT/dl6c-probe" | tr -d \')"
  rm -f "$OUT/dl6c-probe"

  for so in $FOREIGN_LIBS; do
    base="$(basename "$so")"
    cp -L "$so" "$OUT/$base"
    chmod u+w "$OUT/$base"
    install_name_tool -id "@loader_path/$base" "$OUT/$base" 2>/dev/null
    relocate "$OUT/$base" "@executable_path"
  done

  # qsave appends the state PAST __LINKEDIT, which install_name_tool then
  # refuses to touch, so the emulator is relocated before qsave copies it.
  cp -L "$PLBASE/bin/$PLARCH/swipl" "$EMULATOR"
  chmod u+w "$EMULATOR"
  relocate "$EMULATOR" "@executable_path"
  swipl -q -l "$STUB" \
    -g "qsave_program('$OUT/dl6c', [stand_alone(true), emulator('$EMULATOR'), goal(main), toplevel(halt)])" \
    -g halt
  rm -f "$EMULATOR"
fi

# ── 2. sprefa-run: the runtime as one esbuild bundle ─────────────────────────

NATIVE_ADDON="$(node -e '
const { createRequire } = require("node:module");
const fromTsv2 = createRequire(process.argv[1] + "/package.json");
const fromClient = createRequire(fromTsv2.resolve("@libsql/client"));
const fromLibsql = createRequire(fromClient.resolve("libsql"));
const { currentTarget } = fromLibsql("@neon-rs/load");
process.stdout.write(fromLibsql.resolve("@libsql/" + currentTarget()));
' "$TSV2")"
cp "$NATIVE_ADDON" "$OUT/sprefa-sqlite.node"

# The neon loader reaches its addon through a template-literal require, which
# no bundler can see; the resolver patch below points it at the sibling copy.
read -r -d '' BANNER <<'JS' || true
#!/usr/bin/env node
import { createRequire as __sprefaCreateRequire } from "node:module";
import { dirname as __sprefaDirname, join as __sprefaJoin } from "node:path";
import { fileURLToPath as __sprefaFromUrl } from "node:url";
const require = __sprefaCreateRequire(import.meta.url);
const __sprefaAddon = __sprefaJoin(__sprefaDirname(__sprefaFromUrl(import.meta.url)), "sprefa-sqlite.node");
const __sprefaModule = require("node:module");
const __sprefaResolveFilename = __sprefaModule._resolveFilename;
__sprefaModule._resolveFilename = function (request, ...rest) {
  if (/^@libsql\/(darwin|linux|win32|android)-/.test(request)) return __sprefaAddon;
  return __sprefaResolveFilename.call(this, request, ...rest);
};
JS

NODE_PATH="$TSV2/node_modules" "$ESBUILD" "$REPO/scripts/pack/sprefa-run.ts" \
  --bundle --format=esm --platform=node --target=node24 --legal-comments=none \
  --banner:js="$BANNER" --outfile="$OUT/sprefa-run"
chmod +x "$OUT/sprefa-run"

# ── 3. install.sh ───────────────────────────────────────────────────────────

cat > "$OUT/install.sh" <<'INSTALL'
#!/usr/bin/env sh
# Installs the v6 toolchain. Usage: ./install.sh [PREFIX]  (default ~/.local/bin)
set -eu

PREFIX="${1:-$HOME/.local/bin}"
HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$PREFIX"

for required in dl6c sprefa-run sprefa-sqlite.node; do
  [ -f "$HERE/$required" ] || { echo "install: missing $required" >&2; exit 1; }
  cp "$HERE/$required" "$PREFIX/$required"
done
# dl6c dlopens its .so siblings out of its own directory and every dylib is
# anchored to @executable_path, so the whole set lands in one flat PREFIX.
for artifact in "$HERE"/*.dylib "$HERE"/*.so; do
  [ -f "$artifact" ] && cp "$artifact" "$PREFIX/$(basename "$artifact")"
done
chmod +x "$PREFIX/dl6c" "$PREFIX/sprefa-run"

COMPILER_NOTE="self-contained: it loads the .dylib/.so siblings"
[ -f "$HERE/libswipl.10.dylib" ] || COMPILER_NOTE="needs swi-prolog 10 on this machine"

cat <<EOF
installed into $PREFIX
  dl6c               .dl6 -> TypeScript compiler ($COMPILER_NOTE)
  sprefa-run         the runtime (needs node 24+ on PATH and sprefa-sqlite.node)

  dl6c prog.dl6 prog.ts
  sprefa-run --module prog.ts --arrivals rows.jsonl --count some_rel

put $PREFIX on PATH if it is not already:
  export PATH="$PREFIX:\$PATH"

sprefa-extract ships separately through cargo-dist; install it with
  curl --proto '=https' --tlsv1.2 -LsSf \\
    https://github.com/<owner>/<repo>/releases/latest/download/sprefa-extract-installer.sh | sh
EOF
INSTALL
chmod +x "$OUT/install.sh"

echo "release-local: dist-local ready"
ls -l "$OUT"
