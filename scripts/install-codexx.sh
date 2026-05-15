#!/bin/sh

set -eu

REPO="${CODEXX_REPO:-xiusmo/codex}"
RELEASE="latest"
BIN_DIR="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"
BIN_PATH="$BIN_DIR/codexx"
CODEX_HOME_DIR="${CODEX_HOME:-$HOME/.codex}"
STANDALONE_ROOT="$CODEX_HOME_DIR/packages/codexx"
RELEASES_DIR="$STANDALONE_ROOT/releases"
CURRENT_LINK="$STANDALONE_ROOT/current"

step() {
  printf '==> %s\n' "$1"
}

download_file() {
  url="$1"
  output="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O "$output" "$url"
    return
  fi

  echo "curl or wget is required to install codexx." >&2
  exit 1
}

download_text() {
  url="$1"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O - "$url"
    return
  fi

  echo "curl or wget is required to install codexx." >&2
  exit 1
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --release)
        if [ "$#" -lt 2 ]; then
          echo "--release requires a value." >&2
          exit 1
        fi
        RELEASE="$2"
        shift
        ;;
      --repo)
        if [ "$#" -lt 2 ]; then
          echo "--repo requires owner/repo." >&2
          exit 1
        fi
        REPO="$2"
        shift
        ;;
      --help | -h)
        cat <<EOF
Usage: install-codexx.sh [--release TAG] [--repo OWNER/REPO]

Installs the codexx fork launcher while sharing CODEX_HOME with official Codex.
codexx uses CODEX_AUTH_PROFILE=codexx so it does not read or write auth.json.
EOF
        exit 0
        ;;
      *)
        echo "Unknown argument: $1" >&2
        exit 1
        ;;
    esac
    shift
  done
}

resolve_tag() {
  if [ "$RELEASE" != "latest" ]; then
    case "$RELEASE" in
      codexx-v*) printf '%s\n' "$RELEASE" ;;
      v*) printf 'codexx-%s\n' "$RELEASE" ;;
      *) printf 'codexx-v%s\n' "$RELEASE" ;;
    esac
    return
  fi

  release_json="$(download_text "https://api.github.com/repos/$REPO/releases/latest")"
  tag="$(printf '%s\n' "$release_json" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  if [ -z "$tag" ]; then
    echo "Failed to resolve latest codexx release from $REPO." >&2
    exit 1
  fi
  printf '%s\n' "$tag"
}

detect_target() {
  case "$(uname -s)" in
    Darwin) os="apple-darwin" ;;
    Linux) os="unknown-linux-gnu" ;;
    *)
      echo "install-codexx.sh supports macOS and Linux. Download Windows assets manually for now." >&2
      exit 1
      ;;
  esac

  case "$(uname -m)" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *)
      echo "Unsupported architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac

  printf '%s-%s\n' "$arch" "$os"
}

write_launcher() {
  mkdir -p "$BIN_DIR"
  tmp_launcher="$BIN_DIR/.codexx.$$"
  cat >"$tmp_launcher" <<'EOF'
#!/bin/sh
set -eu

CODEX_HOME_DIR="${CODEX_HOME:-$HOME/.codex}"
CODEXX_BIN="$CODEX_HOME_DIR/packages/codexx/current/codex"
export CODEX_AUTH_PROFILE="${CODEX_AUTH_PROFILE:-codexx}"
exec "$CODEXX_BIN" "$@"
EOF
  chmod 0755 "$tmp_launcher"
  mv -f "$tmp_launcher" "$BIN_PATH"
}

add_to_path_hint() {
  case ":$PATH:" in
    *":$BIN_DIR:"*) return ;;
  esac
  step "$BIN_DIR is not on PATH"
  step "Current terminal: export PATH=\"$BIN_DIR:\$PATH\" && codexx"
}

parse_args "$@"

if ! command -v tar >/dev/null 2>&1; then
  echo "tar is required to install codexx." >&2
  exit 1
fi

target="$(detect_target)"
tag="$(resolve_tag)"
asset="codexx-$target.tar.gz"
url="https://github.com/$REPO/releases/download/$tag/$asset"
release_dir="$RELEASES_DIR/$tag-$target"
tmp_dir="$(mktemp -d)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

step "Installing codexx"
step "Repository: $REPO"
step "Release: $tag"
step "Target: $target"

if [ ! -x "$release_dir/codex" ]; then
  mkdir -p "$RELEASES_DIR"
  archive="$tmp_dir/$asset"
  extract_dir="$tmp_dir/extract"
  mkdir -p "$extract_dir"

  step "Downloading $asset"
  download_file "$url" "$archive"
  tar -xzf "$archive" -C "$extract_dir"

  rm -rf "$release_dir"
  mkdir -p "$release_dir"
  cp "$extract_dir/codex" "$release_dir/codex"
  chmod 0755 "$release_dir/codex"
fi

rm -f "$CURRENT_LINK"
ln -s "$release_dir" "$CURRENT_LINK"
write_launcher
add_to_path_hint

"$BIN_PATH" --version >/dev/null
step "Installed: $BIN_PATH"
step "Run: codexx"
printf 'codexx %s installed successfully.\n' "$tag"
