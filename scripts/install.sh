#!/usr/bin/env bash
set -euo pipefail

REPO="aouicher/graphmind"
INSTALL_DIR="${GRAPHMIND_INSTALL_DIR:-$HOME/.local/bin}"
BINARY="graphmind"

info()  { printf '\033[1;34m%s\033[0m\n' "$*"; }
error() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin) os="apple-darwin" ;;
        Linux)  os="unknown-linux-gnu" ;;
        *)      error "Unsupported OS: $os" ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        arm64|aarch64) arch="aarch64" ;;
        *)             error "Unsupported architecture: $arch" ;;
    esac

    echo "${arch}-${os}"
}

get_latest_version() {
    local url="https://api.github.com/repos/${REPO}/releases/latest"
    if command -v curl &>/dev/null; then
        curl -fsSL "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//'
    elif command -v wget &>/dev/null; then
        wget -qO- "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//'
    else
        error "curl or wget required"
    fi
}

download() {
    local url="$1" dest="$2"
    if command -v curl &>/dev/null; then
        curl -fsSL -o "$dest" "$url"
    else
        wget -qO "$dest" "$url"
    fi
}

main() {
    local platform version asset_name download_url tmp

    platform="$(detect_platform)"
    info "Detected platform: ${platform}"

    if [ -n "${1:-}" ]; then
        version="$1"
    else
        info "Fetching latest version..."
        version="$(get_latest_version)"
    fi

    [ -z "$version" ] && error "Could not determine latest version. Check https://github.com/${REPO}/releases"

    info "Installing graphmind ${version}"

    asset_name="${BINARY}-${platform}"
    download_url="https://github.com/${REPO}/releases/download/${version}/${asset_name}"

    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    info "Downloading ${download_url}"
    download "$download_url" "${tmp}/${BINARY}" || error "Download failed. Check that release ${version} has a binary for ${platform}"

    chmod +x "${tmp}/${BINARY}"

    mkdir -p "$INSTALL_DIR"
    mv "${tmp}/${BINARY}" "${INSTALL_DIR}/${BINARY}"

    info "Installed to ${INSTALL_DIR}/${BINARY}"

    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
        echo ""
        info "Add to your PATH:"
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi

    echo ""
    info "Done! Run 'graphmind --help' to get started."
}

main "$@"
