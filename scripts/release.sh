#!/usr/bin/env bash
# Usage: scripts/release.sh [rev]; DRY_RUN=1 builds without tagging or uploading.
# Builds in worktrees of sprefa <rev>, sqlite_ivm and hafley-rs origin/main (the symlinked path deps).
set -euo pipefail

rev=${1:-origin/main}
repo=hafley66/sprefa
projects=${PROJECTS:-$HOME/projects}
targets=(aarch64-apple-darwin x86_64-apple-darwin)
# Homebrew's rustc sits first on PATH and has no x86_64 std; rustup's toolchain has both targets.
toolchain=${RELEASE_TOOLCHAIN:-nightly}

root=$(git rev-parse --show-toplevel)
git -C "$root" fetch -q origin --tags
commit=$(git -C "$root" rev-parse --verify "$rev^{commit}")
for sibling in sqlite_ivm hafley-rs; do git -C "$projects/$sibling" fetch -q origin; done
ivm_commit=$(git -C "$projects/sqlite_ivm" rev-parse --verify origin/main^{commit})
rs_commit=$(git -C "$projects/hafley-rs" rev-parse --verify origin/main^{commit})

work=$(mktemp -d)
cleanup() {
  git -C "$root" worktree remove --force "$work/sprefa" 2>/dev/null || true
  git -C "$projects/sqlite_ivm" worktree remove --force "$work/sqlite_ivm" 2>/dev/null || true
  git -C "$projects/hafley-rs" worktree remove --force "$work/hafley-rs" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
git -C "$root" worktree add -q --detach "$work/sprefa" "$commit"
git -C "$projects/sqlite_ivm" worktree add -q --detach "$work/sqlite_ivm" "$ivm_commit"
git -C "$projects/hafley-rs" worktree add -q --detach "$work/hafley-rs" "$rs_commit"
ln -s "$work/hafley-rs" "$work/sqlite_ivm/hafley-rs"
ln -s "$work/sqlite_ivm" "$work/sprefa/sqlite_ivm"
ln -s "$work/hafley-rs" "$work/sprefa/hafley-rs"
src=$work/sprefa

rustup target add --toolchain "$toolchain" "${targets[@]}" >/dev/null
export PATH="$(dirname "$(rustup +"$toolchain" which rustc)"):$PATH"

version=$(cargo metadata --no-deps --format-version 1 --manifest-path "$src/Cargo.toml" \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "dl8"))')
tag="v$version"
short() { git -C "$1" rev-parse --short "$2"; }
built_from="sprefa $(short "$root" "$commit"), sqlite_ivm $(short "$projects/sqlite_ivm" "$ivm_commit"), hafley-rs $(short "$projects/hafley-rs" "$rs_commit")"
echo "release: $tag from $built_from"

if [ -z "${DRY_RUN:-}" ]; then
  if gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
    echo "release: $tag is already published; bump the version" >&2; exit 1
  fi
  if existing=$(git -C "$root" rev-parse -q --verify "refs/tags/$tag^{commit}"); then
    [ "$existing" = "$commit" ] || { echo "release: tag $tag points at $existing, not $commit" >&2; exit 1; }
  else
    git -C "$root" tag -a "$tag" -m "dl8 v$version" "$commit"
    git -C "$root" push -q origin "$tag"
  fi
fi

target_args=(); for t in "${targets[@]}"; do target_args+=(--target "$t"); done
(cd "$src" && dist build --tag="$tag" --artifacts=all "${target_args[@]}")
out=$src/target/distrib
assets=("$out"/*-installer.sh "$out"/*.tar.xz "$out"/*.tar.xz.sha256 "$out"/sha256.sum "$out"/source.tar.gz "$out"/source.tar.gz.sha256)
ls -1 "${assets[@]}"

if [ -n "${DRY_RUN:-}" ]; then
  keep=$root/target/release-dry-run/$tag
  mkdir -p "$keep" && cp "${assets[@]}" "$keep/"
  echo "release: dry run, artifacts in $keep"
  exit 0
fi

installer=$(basename "$out"/*-installer.sh)
url="https://github.com/$repo/releases/download/$tag/$installer"
gh release create "$tag" --repo "$repo" --verify-tag \
  --title "dl8 v$version (macOS)" \
  --notes "dl8 binary only, macOS, Apple Silicon and Intel. Built locally from $built_from. Ad-hoc signed, not notarized. sqlite-backed evaluation loads libsqlite_ivm.dylib, which is not included.

\`\`\`sh
curl --proto '=https' --tlsv1.2 -LsSf $url | sh
\`\`\`" "${assets[@]}"

# Install from the public URL into a scratch CARGO_HOME; shell rc files untouched.
home=$work/install
curl --proto '=https' --tlsv1.2 -LsSf "$url" | CARGO_HOME=$home sh -s -- --no-modify-path >/dev/null
"$home/bin/dl8" --version
echo "release: $tag published"
