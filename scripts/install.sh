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

auth_header() {
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        echo "Authorization: token ${GITHUB_TOKEN}"
    elif command -v gh &>/dev/null && gh auth status &>/dev/null; then
        echo "Authorization: token $(gh auth token)"
    else
        echo ""
    fi
}

get_latest_version() {
    local header
    header="$(auth_header)"

    if command -v gh &>/dev/null && gh auth status &>/dev/null; then
        gh release view --repo "${REPO}" --json tagName --jq '.tagName' 2>/dev/null && return
    fi

    local url="https://api.github.com/repos/${REPO}/releases/latest"
    if command -v curl &>/dev/null; then
        if [ -n "$header" ]; then
            curl -fsSL -H "$header" "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//'
        else
            curl -fsSL "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//'
        fi
    elif command -v wget &>/dev/null; then
        wget -qO- "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//'
    else
        error "curl or wget required"
    fi
}

download() {
    local url="$1" dest="$2" header
    header="$(auth_header)"

    if command -v gh &>/dev/null && gh auth status &>/dev/null; then
        local version_tag asset_name
        asset_name="$(basename "$url")"
        version_tag="$(echo "$url" | sed 's|.*/download/\([^/]*\)/.*|\1|')"
        if gh release download "$version_tag" --repo "${REPO}" -p "$asset_name" -D "$(dirname "$dest")" --clobber 2>/dev/null; then
            mv "$(dirname "$dest")/$asset_name" "$dest"
            return 0
        fi
    fi

    if command -v curl &>/dev/null; then
        if [ -n "$header" ]; then
            curl -fsSL -H "$header" -H "Accept: application/octet-stream" -o "$dest" "$url"
        else
            curl -fsSL -o "$dest" "$url"
        fi
    else
        wget -qO "$dest" "$url"
    fi
}

configure_path() {
    local dir="$1"
    local export_line="export PATH=\"${dir}:\$PATH\""
    local updated=()

    # zshenv: always ensure (loaded even in non-interactive shells — needed for MCP)
    # zshrc/bashrc: only if the file already exists
    local profiles=(".zshenv" ".zshrc" ".bashrc")
    for profile in "${profiles[@]}"; do
        local profile_path="$HOME/$profile"
        [ ! -f "$profile_path" ] && [ "$profile" != ".zshenv" ] && continue
        if grep -qF "$dir" "$profile_path" 2>/dev/null; then
            continue
        fi
        printf '\n%s\n' "$export_line" >> "$profile_path"
        updated+=("$profile")
    done

    if [ ${#updated[@]} -eq 0 ]; then
        info "PATH already configured"
    else
        info "PATH added to: ${updated[*]}"
        echo "  Restart your shell or run: source ~/${updated[0]}"
    fi
}

TMP_DIR=""
cleanup() { [ -n "$TMP_DIR" ] && rm -rf "$TMP_DIR"; }

main() {
    local platform version asset_name download_url

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

    case "$platform" in
        aarch64-apple-darwin)     asset_name="graphmind-cli-macos-arm64" ;;
        x86_64-apple-darwin)      asset_name="graphmind-cli-macos-x64" ;;
        x86_64-unknown-linux-gnu) asset_name="graphmind-cli-linux-x64" ;;
        *) error "No prebuilt binary for platform: $platform" ;;
    esac
    download_url="https://github.com/${REPO}/releases/download/${version}/${asset_name}"

    TMP_DIR="$(mktemp -d)"
    trap cleanup EXIT

    info "Downloading ${download_url}"
    download "$download_url" "${TMP_DIR}/${BINARY}" || error "Download failed. Check that release ${version} has a binary for ${platform}"

    chmod +x "${TMP_DIR}/${BINARY}"

    mkdir -p "$INSTALL_DIR"
    mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"

    info "Installed to ${INSTALL_DIR}/${BINARY}"

    configure_path "$INSTALL_DIR"

    echo ""
    info "Done! Run 'graphmind setup' to complete configuration, then 'graphmind --help' to get started."
}

main "$@"
