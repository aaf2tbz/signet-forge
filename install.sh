#!/bin/bash
# Signet Forge installer
# Usage: curl -sSL https://raw.githubusercontent.com/aaf2tbz/signet-forge/main/install.sh | bash

set -euo pipefail

REPO="aaf2tbz/signet-forge"
BINARY="forge"

echo "Signet Forge Installer"
echo "======================"
echo ""

# Detect OS and arch
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64|amd64) ARCH="x64" ;;
  arm64|aarch64) ARCH="arm64" ;;
  *) echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
esac

case "$OS" in
  darwin) PLATFORM="macos-${ARCH}" ;;
  linux) PLATFORM="linux-${ARCH}" ;;
  *) echo "Error: unsupported OS: $OS"; exit 1 ;;
esac

# Check for required tools
for cmd in curl tar; do
  if ! command -v "$cmd" &>/dev/null; then
    echo "Error: '$cmd' is required but not installed."
    exit 1
  fi
done

# Get latest release tag
echo "Fetching latest release..."
LATEST=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
if [ -z "$LATEST" ]; then
  echo "Error: failed to fetch latest release from GitHub."
  echo "Check your internet connection or try again later."
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${LATEST}/forge-${PLATFORM}.tar.gz"
echo "Installing Forge ${LATEST} for ${PLATFORM}..."

# Download to temp directory
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

HTTP_CODE=$(curl -sSL -w "%{http_code}" "$URL" -o "${TMP}/forge.tar.gz")
if [ "$HTTP_CODE" != "200" ]; then
  echo "Error: download failed (HTTP ${HTTP_CODE})."
  echo "URL: ${URL}"
  echo ""
  echo "This platform/architecture may not have a prebuilt binary."
  echo "You can build from source: cargo install --git https://github.com/${REPO}"
  exit 1
fi

# Extract
tar xzf "${TMP}/forge.tar.gz" -C "$TMP"

# Verify binary exists in extracted files
if [ ! -f "${TMP}/${BINARY}" ]; then
  # Try looking in a subdirectory (some tarballs nest the binary)
  FOUND=$(find "$TMP" -name "$BINARY" -type f | head -1)
  if [ -z "$FOUND" ]; then
    echo "Error: '${BINARY}' not found in the downloaded archive."
    exit 1
  fi
  mv "$FOUND" "${TMP}/${BINARY}"
fi

# Choose install destination
if [ -d "$HOME/.cargo/bin" ]; then
  DEST="$HOME/.cargo/bin"
elif [ -d "$HOME/.local/bin" ]; then
  DEST="$HOME/.local/bin"
else
  DEST="/usr/local/bin"
fi

# Check write permission; fall back to sudo if needed
if [ -w "$DEST" ]; then
  mv "${TMP}/${BINARY}" "${DEST}/${BINARY}"
  chmod +x "${DEST}/${BINARY}"
else
  echo "Installing to ${DEST} (requires sudo)..."
  sudo mv "${TMP}/${BINARY}" "${DEST}/${BINARY}"
  sudo chmod +x "${DEST}/${BINARY}"
fi

echo ""
echo "Forge ${LATEST} installed to ${DEST}/${BINARY}"

# Check if DEST is in PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$DEST"; then
  echo ""
  echo "Warning: ${DEST} is not in your PATH."
  echo "Add it to your shell profile:"
  echo "  export PATH=\"${DEST}:\$PATH\""
fi

echo ""
echo "Run 'forge' to get started."
