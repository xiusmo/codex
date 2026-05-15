#!/bin/sh

set -eu

REPO="${CODEXX_REPO:-xiusmo/codex}"
TAG="${1:-}"

if [ -z "$TAG" ]; then
  echo "Usage: scripts/release-local-codexx.sh codexx-vX.Y.Z" >&2
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required to upload a codexx release." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to build codexx locally." >&2
  exit 1
fi

target="$(rustc -vV | awk '/^host:/ {print $2}')"
case "$target" in
  aarch64-apple-darwin | x86_64-apple-darwin | x86_64-unknown-linux-gnu | aarch64-unknown-linux-gnu)
    ;;
  *)
    echo "Unsupported local release target: $target" >&2
    exit 1
    ;;
esac

asset="codexx-$target.tar.gz"
dist_dir="dist/codexx-local-release"
stage_dir="$dist_dir/stage"

rm -rf "$dist_dir"
mkdir -p "$stage_dir"

echo "==> Building codexx for $target"
(cd codex-rs && cargo build -p codex-cli --bin codex --release)

cp codex-rs/target/release/codex "$stage_dir/codex"
chmod 0755 "$stage_dir/codex"

echo "==> Packaging $asset"
tar -C "$stage_dir" -czf "$dist_dir/$asset" codex

echo "==> Verifying packaged binary"
tmp_dir="$dist_dir/verify"
mkdir -p "$tmp_dir"
tar -xzf "$dist_dir/$asset" -C "$tmp_dir"
"$tmp_dir/codex" --version

if gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  echo "==> Uploading asset to existing release $TAG"
else
  echo "==> Creating release $TAG"
  gh release create "$TAG" \
    --repo "$REPO" \
    --title "$TAG" \
    --notes "Local codexx release for $target."
fi

gh release upload "$TAG" "$dist_dir/$asset" --repo "$REPO" --clobber
echo "==> Uploaded https://github.com/$REPO/releases/tag/$TAG"
