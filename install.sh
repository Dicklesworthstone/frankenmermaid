#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${FM_INSTALL_GIT_URL:-https://github.com/Dicklesworthstone/frankenmermaid.git}"
INSTALL_PATH="${FM_INSTALL_PATH:-}"
INSTALL_ROOT="${FM_INSTALL_ROOT:-$HOME/.local}"
INSTALL_BIN_DIR="$INSTALL_ROOT/bin"
PACKAGE_NAME="frankenmermaid-cli"
PACKAGE_DIR="fm-cli"
CANONICAL_BIN_NAME="frankenmermaid"
LEGACY_BIN_NAME="fm-cli"
RUSTUP_INIT_URL="${FM_RUSTUP_INIT_URL:-https://sh.rustup.rs}"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command '$1' was not found in PATH" >&2
    exit 1
  fi
}

ensure_rust_toolchain() {
  if command -v cargo >/dev/null 2>&1; then
    return
  fi

  need_cmd curl
  echo "==> cargo not found; installing a minimal Rust toolchain via rustup"
  curl --proto '=https' --tlsv1.2 -fsSL "$RUSTUP_INIT_URL" | sh -s -- -y --profile minimal
  # shellcheck disable=SC1090,SC1091
  source "$HOME/.cargo/env"
}

build_ref_args() {
  if [[ -n "${FM_INSTALL_GIT_REV:-}" ]]; then
    printf -- '--rev\n%s\n' "$FM_INSTALL_GIT_REV"
  elif [[ -n "${FM_INSTALL_GIT_TAG:-}" ]]; then
    printf -- '--tag\n%s\n' "$FM_INSTALL_GIT_TAG"
  else
    printf -- '--branch\n%s\n' "${FM_INSTALL_GIT_BRANCH:-main}"
  fi
}

source_main() {
  need_cmd git
  ensure_rust_toolchain

  if ! command -v cc >/dev/null 2>&1; then
    echo "error: a C toolchain is required to build fm-cli from source; install 'cc'/'gcc' and retry" >&2
    exit 1
  fi

  mkdir -p "$INSTALL_BIN_DIR"
  local source_root
  source_root="$(mktemp -d "$INSTALL_ROOT/.frankenmermaid-source.XXXXXX")"
  echo "==> Retained source installation stage: $source_root"

  if [[ -n "$INSTALL_PATH" ]]; then
    # If pointed at a workspace root, resolve to the fm-cli crate directory.
    local resolved_path="$INSTALL_PATH"
    if [[ -f "$INSTALL_PATH/Cargo.toml" ]] && grep -q '^\[workspace\]' "$INSTALL_PATH/Cargo.toml" 2>/dev/null; then
      if [[ -d "$INSTALL_PATH/crates/$PACKAGE_DIR" ]]; then
        resolved_path="$INSTALL_PATH/crates/$PACKAGE_DIR"
      fi
    fi
    cargo_args=(
      install
      --path "$resolved_path"
      --locked
      --force
      --root "$source_root"
      --bin "$CANONICAL_BIN_NAME"
      --bin "$LEGACY_BIN_NAME"
    )
    source_description="$resolved_path"
  else
    ref_args=()
    while IFS= read -r ref_arg; do ref_args+=("$ref_arg"); done < <(build_ref_args)
    cargo_args=(
      install
      --git "$REPO_URL"
      "${ref_args[@]}"
      --locked
      --force
      --root "$source_root"
      --bin "$CANONICAL_BIN_NAME"
      --bin "$LEGACY_BIN_NAME"
      "$PACKAGE_NAME"
    )
    source_description="$REPO_URL"
  fi

  echo "==> Installing $PACKAGE_NAME from $source_description"
  CARGO_NET_GIT_FETCH_WITH_CLI="${CARGO_NET_GIT_FETCH_WITH_CLI:-true}" cargo "${cargo_args[@]}"

  for bin_name in "$CANONICAL_BIN_NAME" "$LEGACY_BIN_NAME"; do
    [[ -x "$source_root/bin/$bin_name" ]] || { echo "error: missing staged command $bin_name" >&2; exit 1; }
    if [[ -e "$INSTALL_BIN_DIR/$bin_name" || -L "$INSTALL_BIN_DIR/$bin_name" ]]; then
      [[ ! -e "$INSTALL_BIN_DIR/$bin_name" || -f "$INSTALL_BIN_DIR/$bin_name" ]] || { echo "error: existing command is not a file" >&2; exit 1; }
      if [[ -f "$INSTALL_BIN_DIR/$bin_name" ]]; then cp -p "$INSTALL_BIN_DIR/$bin_name" "$source_root/$bin_name.previous.bytes"; fi
    fi
  done
  for bin_name in "$CANONICAL_BIN_NAME" "$LEGACY_BIN_NAME"; do
    if [[ -e "$INSTALL_BIN_DIR/$bin_name" || -L "$INSTALL_BIN_DIR/$bin_name" ]]; then mv "$INSTALL_BIN_DIR/$bin_name" "$source_root/$bin_name.previous"; fi
    cp "$source_root/bin/$bin_name" "$INSTALL_BIN_DIR/$bin_name"
    chmod 0755 "$INSTALL_BIN_DIR/$bin_name"
  done

  echo "==> Installed $CANONICAL_BIN_NAME and $LEGACY_BIN_NAME to $INSTALL_BIN_DIR"
  "$INSTALL_BIN_DIR/$CANONICAL_BIN_NAME" --version

  case ":$PATH:" in
    *":$INSTALL_BIN_DIR:"*) ;;
    *)
      echo
      echo "Add $INSTALL_BIN_DIR to your PATH if it is not already there:"
      echo "  export PATH=\"$INSTALL_BIN_DIR:\$PATH\""
      ;;
  esac
}


FROM_SOURCE=0
# Existing explicit source selectors retain their meaning.
if [[ -n "${FM_INSTALL_PATH:-}${FM_INSTALL_GIT_URL:-}${FM_INSTALL_GIT_REV:-}${FM_INSTALL_GIT_TAG:-}${FM_INSTALL_GIT_BRANCH:-}" ]]; then
  FROM_SOURCE=1
fi
REPO="Dicklesworthstone/frankenmermaid"
BIN_NAME="frankenmermaid"
INSTALL_DIR="${FM_INSTALL_ROOT:-$HOME/.local}/bin"
VERSION="${VERSION:-latest}"
REQUIRE_MINISIGN=0
MINISIGN_KEY="RWTQGPeLsnm9G7VFdFWkkcRi3wJK/PqsYxWC+oLNN74W9IjBxRU1Xu70"

log() { echo "[frankenmermaid] $*" >&2; }
fail() { echo "[frankenmermaid] $*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from-source) FROM_SOURCE=1; shift ;;
    --version) [[ $# -ge 2 ]] || fail "--version requires a value"; VERSION="$2"; shift 2 ;;
    --dest) [[ $# -ge 2 ]] || fail "--dest requires a directory"; INSTALL_DIR="$2"; shift 2 ;;
    --require-minisign) REQUIRE_MINISIGN=1; shift ;;
    --help|-h)
      echo "Usage: install.sh [--version VERSION] [--dest DIR] [--require-minisign] [--from-source]"
      echo "Downloads are SHA256-verified; minisign verifies authenticity when available."
      echo "Download scratch and an existing binary backup are retained."
      exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

download() {
  local url="$1"
  local out="$2"
  if command -v curl >/dev/null 2>&1; then
    local attempt
    for attempt in 1 2 3; do
      if curl -fL --retry 2 --retry-delay 1 --retry-all-errors "$url" -o "$out"; then
        [[ -s "$out" ]] && return 0
      fi
      log "Download attempt $attempt failed"
    done
  fi
  if command -v wget >/dev/null 2>&1; then
    local attempt
    for attempt in 1 2 3; do
      if wget -O "$out" "$url"; then
        [[ -s "$out" ]] && return 0
      fi
      log "Download attempt $attempt failed"
    done
  fi
  return 1
}

if [[ "$FROM_SOURCE" == 1 ]]; then
  if [[ "$VERSION" != latest ]]; then
    VERSION="${VERSION#v}"
    [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "version must be a stable semantic version"
    [[ -z "${FM_INSTALL_PATH:-}${FM_INSTALL_GIT_REV:-}${FM_INSTALL_GIT_TAG:-}${FM_INSTALL_GIT_BRANCH:-}" ]] \
      || fail "--version conflicts with an explicit source path/revision; choose one selector"
    FM_INSTALL_GIT_TAG="v$VERSION"
  fi
  [[ "$REQUIRE_MINISIGN" == 0 ]] || fail "--require-minisign applies to release archives"
  INSTALL_BIN_DIR="$INSTALL_DIR"
  INSTALL_ROOT="${INSTALL_DIR%/bin}"
  [[ "$INSTALL_DIR" == "$INSTALL_ROOT/bin" ]] || fail "source --dest must end in /bin"
  source_main
  exit 0
fi

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Linux) platform="linux" ;;
  Darwin) platform="darwin" ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT) platform="windows" ;;
  *) fail "unsupported OS: $os" ;;
esac

case "$arch" in
  x86_64|amd64) arch="amd64" ;;
  arm64|aarch64) arch="arm64" ;;
  *) fail "unsupported architecture: $arch" ;;
esac

if [[ "$platform" == "windows" ]]; then
  asset="${BIN_NAME}-windows-${arch}.zip"
  bin_file="${BIN_NAME}.exe"
else
  asset="${BIN_NAME}-${platform}-${arch}.tar.xz"
  bin_file="${BIN_NAME}"
fi

if [[ "$VERSION" == latest ]]; then
  command -v curl >/dev/null 2>&1 || fail "curl is required to resolve latest; use --version with wget"
  release_url="$(curl -fsSL --retry 2 --retry-all-errors -o /dev/null -w '%{url_effective}' \
    "https://github.com/${REPO}/releases/latest")" || fail "cannot resolve latest release"
  VERSION="${release_url##*/}"
fi
VERSION="${VERSION#v}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "version must be a stable semantic version"
url="https://github.com/${REPO}/releases/download/v${VERSION}/${asset}"

mkdir -p "$INSTALL_DIR"
tmpdir="$(mktemp -d "$INSTALL_DIR/.frankenmermaid-install.XXXXXX")"

trap 'log "Retained download scratch: $tmpdir"' EXIT

archive="$tmpdir/$asset"

log "Downloading $url"
download "$url" "$archive" || fail "cannot download $asset; installation was not changed"
download "${url}.sha256" "${archive}.sha256" || fail "cannot download SHA256 sidecar"
expected="$(awk -v name="$asset" '$2 == name || $2 == "*" name {print $1; count++} END {if (count != 1) exit 1}' \
  "${archive}.sha256")" || fail "invalid SHA256 sidecar"
[[ "$expected" =~ ^[[:xdigit:]]{64}$ ]] || fail "invalid SHA256 digest"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$archive" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$archive" | awk '{print $1}')"
else
  fail "sha256sum or shasum is required"
fi
[[ "$actual" == "$expected" ]] || fail "SHA256 mismatch; installation was not changed"
log "SHA256 verified for $asset"

# Every native release archive must have a valid signature when minisign is available.
if command -v minisign >/dev/null 2>&1; then
  download "${url}.minisig" "${archive}.minisig" || fail "cannot download minisign signature"
  minisign -Vm "$archive" -x "${archive}.minisig" -P "$MINISIGN_KEY" >/dev/null \
    || fail "minisign verification failed; installation was not changed"
  log "Minisign authenticity verified"
else
  [[ "$REQUIRE_MINISIGN" == 0 ]] || fail "minisign is required but unavailable"
  log "minisign unavailable: SHA256 verified, authenticity was not verified"
fi

if [[ "$platform" == "windows" ]]; then
  if command -v unzip >/dev/null 2>&1; then
    unzip -o "$archive" -d "$tmpdir" >/dev/null || fail "failed to extract $asset"
  else
    fail "unzip not found (required for windows zip)"
  fi
else
  tar -xJf "$archive" -C "$tmpdir" || fail "failed to extract $asset"
fi

if [[ ! -f "$tmpdir/$bin_file" ]]; then
  fail "downloaded archive missing $bin_file"
fi

legacy_file="fm-cli"
[[ "$platform" != windows ]] || legacy_file="fm-cli.exe"
binary_version="$("$tmpdir/$bin_file" --version)" || fail "downloaded command cannot run"
[[ "$binary_version" == "fm-cli $VERSION" || "$binary_version" == "frankenmermaid $VERSION" ]] \
  || fail "downloaded command version does not match v$VERSION"
# Validate and copy both old byte streams before renaming either directory entry.
for existing_name in "$bin_file" "$legacy_file"; do
  existing_path="$INSTALL_DIR/$existing_name"
  if [[ -e "$existing_path" || -L "$existing_path" ]]; then
    [[ ! -e "$existing_path" || -f "$existing_path" ]] || fail "existing command is not a file: $existing_path"
    if [[ -f "$existing_path" ]]; then
      cp -p "$existing_path" "$tmpdir/${existing_name}.previous.bytes"
    fi
  fi
done
for existing_name in "$bin_file" "$legacy_file"; do
  existing_path="$INSTALL_DIR/$existing_name"
  if [[ -e "$existing_path" || -L "$existing_path" ]]; then
    mv "$existing_path" "$tmpdir/${existing_name}.previous"
    log "Previous command retained at $tmpdir/${existing_name}.previous"
  fi
done

if command -v install >/dev/null 2>&1; then
  install -m 0755 "$tmpdir/$bin_file" "$INSTALL_DIR/$bin_file"
else
  cp "$tmpdir/$bin_file" "$INSTALL_DIR/$bin_file"
  chmod 0755 "$INSTALL_DIR/$bin_file"
fi

log "Installed $bin_file to $INSTALL_DIR"
log "Make sure $INSTALL_DIR is in your PATH."

# Keep the documented legacy command alongside the canonical command.
cp "$INSTALL_DIR/$bin_file" "$INSTALL_DIR/$legacy_file"
chmod 0755 "$INSTALL_DIR/$legacy_file"
"$INSTALL_DIR/$bin_file" --version
"$INSTALL_DIR/$legacy_file" --version
