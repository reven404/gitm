#!/bin/sh
# gitm installer — downloads the latest release binary into ~/.local/bin.
#
# Usage:
#   curl -fsSL https://github.com/reven404/gitm/raw/main/install.sh | sh
#   curl -fsSL ... | sh -s -- --bin /usr/local/bin        # install elsewhere
#   curl -fsSL ... | sh -s -- --version v0.1.3            # pin a release
#   curl -fsSL ... | sh -s -- --owner reven404 --name gitm  # fork
#
# Env overrides: GITM_REPO_OWNER, GITM_REPO_NAME.
# Flags:  --bin <dir>   install dir (default ~/.local/bin)
#         --version <v> release tag (default: latest)
#         --owner <o>   repo owner
#         --name <n>    repo name
#         -h, --help    this help
#
# No sudo by default — installs to a user-writable dir. If you target a
# system dir like /usr/local/bin, run under sudo yourself.

set -eu

REPO_OWNER="${GITM_REPO_OWNER:-reven404}"
REPO_NAME="${GITM_REPO_NAME:-gitm}"
VERSION="latest"
BIN_DIR="${HOME}/.local/bin"

usage() {
  sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --bin)     BIN_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --owner)   REPO_OWNER="$2"; shift 2 ;;
    --name)    REPO_NAME="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) echo "install.sh: unknown flag: $1" >&2; exit 1 ;;
  esac
done

need() { command -v "$1" >/dev/null 2>&1 || { echo "install.sh: '$1' not found in PATH" >&2; exit 1; }; }
need curl
need tar

# --- detect host arch/os (match release asset naming: gitm-<arch>-<os>.tar.gz) ---
ARCH=$(uname -m 2>/dev/null || echo "")
OS=$(uname -s 2>/dev/null | tr '[:upper:]' '[:lower:]' || echo "")
case "$ARCH" in
  arm64|aarch64) ARCH=aarch64 ;;
  x86_64|amd64)  ARCH=x86_64 ;;
  *) echo "install.sh: unsupported arch: ${ARCH:-unknown}" >&2; exit 1 ;;
esac
case "$OS" in
  darwin)        OS=darwin ;;
  linux)         OS=linux ;;
  *) echo "install.sh: unsupported os: ${OS:-unknown} (prebuilt binaries are darwin/linux only)" >&2; exit 1 ;;
esac

ASSET="gitm-${ARCH}-${OS}.tar.gz"
BASE="https://github.com/${REPO_OWNER}/${REPO_NAME}"
if [ "$VERSION" = "latest" ]; then
  URL="${BASE}/releases/latest/download/${ASSET}"
  # Resolve the actual tag for the success message.
  TAG=$(curl -fsSL -o /dev/null -w '%{url_effective}' "${BASE}/releases/latest" \
        | sed 's|.*/tag/||')
  [ -n "$TAG" ] || TAG="latest"
else
  TAG="$VERSION"
  URL="${BASE}/releases/download/${TAG}/${ASSET}"
fi

# --- prepare install dir ---
mkdir -p "$BIN_DIR"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/gitm-install.XXXXXX")
trap 'rm -rf "$TMP"' EXIT INT TERM

echo "→ downloading ${TAG} / ${ASSET}"
if ! curl -fsSL -H "User-Agent: gitm-install" -o "${TMP}/${ASSET}" "$URL"; then
  echo "install.sh: download failed for ${URL}" >&2
  echo "  (if this is a fresh release, CI may still be uploading assets — retry in a minute)" >&2
  exit 1
fi

echo "→ extracting to ${BIN_DIR}"
tar xzf "${TMP}/${ASSET}" -C "$BIN_DIR" gitm
chmod 0755 "${BIN_DIR}/gitm"

# --- verify it runs ---
if "${BIN_DIR}/gitm" version >/dev/null 2>&1; then
  INSTALLED=$("${BIN_DIR}/gitm" version 2>/dev/null | head -1)
else
  INSTALLED="${BIN_DIR}/gitm"
fi

# --- PATH hint ---
case ":${PATH}:" in
  *":${BIN_DIR}:"*) ;;
  *)
    cat >&2 <<EOF

⚠  ${BIN_DIR} is not on your PATH.
   Add this to your shell rc (~/.zshrc or ~/.bashrc):

       export PATH="${BIN_DIR}:\$PATH"

   Then start a new shell.
EOF
    ;;
esac

echo
echo "✓ gitm installed: ${INSTALLED}"
echo "  Run \`${BIN_DIR}/gitm version\` to verify, \`${BIN_DIR}/gitm update --check\` later."
