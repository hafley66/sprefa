#!/usr/bin/env bash
# One-command release. Bumps the crate version, rebuilds the SAME-versioned VSIX
# (so the released binary embeds a matching extension), rolls the changelog,
# commits, and tags exactly ONE tag — GitHub suppresses cargo-dist Release runs
# when more than 3 tags are pushed at once, so releases go one at a time.
#
# It does NOT push (that is the one manual gate). Finish with:
#   git push origin main && git push origin v<version>
#
#   usage: scripts/release.sh X.Y.Z
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:?usage: scripts/release.sh X.Y.Z}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must be X.Y.Z, got '$version'"; exit 1; }
tag="v$version"
date="$(date +%Y-%m-%d)"

git rev-parse "$tag" >/dev/null 2>&1 && { echo "tag $tag already exists"; exit 1; }

echo "[release] crate version -> $version"
perl -0pi -e "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"$version\"/m" Cargo.toml

echo "[release] rebuilding the same-versioned VSIX"
scripts/build-vsix.sh   # stamps editors/vscode-dl/package.json to $version + rebuilds dl-lsp.vsix

echo "[release] refreshing Cargo.lock + verifying build.rs version parity"
cargo build -q -p sprefa-dl --bin dl   # updates Cargo.lock; build.rs panics if the VSIX drifts

echo "[release] rolling CHANGELOG.md ([Unreleased] -> [$version])"
grep -q "## \[$version\]" CHANGELOG.md \
  || perl -0pi -e "s/## \[Unreleased\]\n/## [Unreleased]\n\n## [$version] - $date\n/" CHANGELOG.md

git add Cargo.toml Cargo.lock CHANGELOG.md \
        editors/vscode-dl/package.json editors/vscode-dl/dl-lsp.vsix
git commit -q -m "release: $tag"
git tag -a "$tag" -m "$tag"

echo
echo "[release] committed + tagged $tag. Push to ship (one tag):"
echo "    git push origin main && git push origin $tag"
