#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${FM_INSTALL_GIT_URL:-https://github.com/Dicklesworthstone/frankenmermaid.git}"
INSTALL_PATH="${FM_INSTALL_PATH:-}"
INSTALL_ROOT="${FM_INSTALL_ROOT:-$HOME/.local}"
INSTALL_BIN_DIR="$INSTALL_ROOT/bin"
PACKAGE_NAME="frankenmermaid-cli"
PACKAGE_DIR="fm-cli"
BIN_NAME="fm-cli"
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
  # shellcheck disable=SC1090
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

main() {
  need_cmd git
  ensure_rust_toolchain

  if ! command -v cc >/dev/null 2>&1; then
    echo "error: a C toolchain is required to build fm-cli from source; install 'cc'/'gcc' and retry" >&2
    exit 1
  fi

  mkdir -p "$INSTALL_BIN_DIR"

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
      --root "$INSTALL_ROOT"
      --bin "$BIN_NAME"
    )
    source_description="$resolved_path"
  else
    mapfile -t ref_args < <(build_ref_args)
    cargo_args=(
      install
      --git "$REPO_URL"
      "${ref_args[@]}"
      --locked
      --force
      --root "$INSTALL_ROOT"
      --bin "$BIN_NAME"
      "$PACKAGE_NAME"
    )
    source_description="$REPO_URL"
  fi

  echo "==> Installing $PACKAGE_NAME from $source_description"
  CARGO_NET_GIT_FETCH_WITH_CLI="${CARGO_NET_GIT_FETCH_WITH_CLI:-true}" cargo "${cargo_args[@]}"

  if [[ ! -x "$INSTALL_BIN_DIR/$BIN_NAME" ]]; then
    echo "error: expected installed binary at $INSTALL_BIN_DIR/$BIN_NAME" >&2
    exit 1
  fi

  echo "==> Installed $BIN_NAME to $INSTALL_BIN_DIR/$BIN_NAME"
  "$INSTALL_BIN_DIR/$BIN_NAME" --version

  case ":$PATH:" in
    *":$INSTALL_BIN_DIR:"*) ;;
    *)
      echo
      echo "Add $INSTALL_BIN_DIR to your PATH if it is not already there:"
      echo "  export PATH=\"$INSTALL_BIN_DIR:\$PATH\""
      ;;
  esac
}

main "$@"
