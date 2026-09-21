#!/usr/bin/env sh
#
# Install the `vivi` binary from GitHub release assets.
#
# Release archives are attached to this repository's GitHub releases, which is
# the only distribution channel — there is no package-manager formula. The
# script selects the archive for the local platform and falls back to a source
# build when no archive exists for it.
#
# The companion `vivi-pty` binary ships from its own repository,
# https://github.com/ianzepp/vivi-pty.
#
# Environment:
#   VIVI_REPO         owner/repo to install from   (default ianzepp/vivi)
#   VIVI_VERSION      release tag to install       (default: latest release)
#   VIVI_INSTALL_DIR  destination directory        (default ~/.local/bin)
#   VIVI_BIN_NAME     primary binary name          (default vivi)
#
set -eu

REPO="${VIVI_REPO:-ianzepp/vivi}"
BIN_NAME="${VIVI_BIN_NAME:-vivi}"
INSTALL_DIR="${VIVI_INSTALL_DIR:-${HOME}/.local/bin}"
VERSION="${VIVI_VERSION:-}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command not found: $1" >&2
    exit 1
  fi
}

latest_version() {
  need curl
  curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" |
    sed -n 's/.*"tag_name": "\(v[^"]*\)".*/\1/p' |
    head -n 1
}

target_triple() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}:${arch}" in
    Darwin:arm64) echo "aarch64-apple-darwin" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    Linux:x86_64) echo "x86_64-unknown-linux-gnu" ;;
    Linux:aarch64 | Linux:arm64) echo "aarch64-unknown-linux-gnu" ;;
    *)
      echo "unsupported platform: ${os}/${arch}" >&2
      return 1
      ;;
  esac
}

install_binary_release() {
  target="$1"
  tmp="${TMPDIR:-/tmp}/vivi-install.$$"
  archive="${tmp}/${BIN_NAME}-${target}.tar.gz"
  url="https://github.com/${REPO}/releases/download/${VERSION}/${BIN_NAME}-${target}.tar.gz"

  mkdir -p "${tmp}"
  if ! curl -fsSL "${url}" -o "${archive}"; then
    rm -rf "${tmp}"
    return 1
  fi

  tar -xzf "${archive}" -C "${tmp}"
  if [ ! -f "${tmp}/${BIN_NAME}" ]; then
    echo "error: no ${BIN_NAME} binary found in release archive" >&2
    rm -rf "${tmp}"
    return 1
  fi
  mkdir -p "${INSTALL_DIR}"
  install -m 0755 "${tmp}/${BIN_NAME}" "${INSTALL_DIR}/${BIN_NAME}"
  echo "Installed ${BIN_NAME} to ${INSTALL_DIR}/${BIN_NAME}"
  rm -rf "${tmp}"
}

install_from_source() {
  need cargo
  echo "Installing ${BIN_NAME} from source..."
  cargo install --git "https://github.com/${REPO}.git" --tag "${VERSION}" --root "${INSTALL_DIR%/bin}"
}

main() {
  if [ -z "${VERSION}" ]; then
    VERSION="$(latest_version)"
  fi
  if [ -z "${VERSION}" ]; then
    echo "error: could not determine latest ${REPO} release" >&2
    exit 1
  fi

  target="$(target_triple)"
  echo "Installing ${BIN_NAME} ${VERSION} for ${target}"

  if install_binary_release "${target}"; then
    : # installed from the release archive
  else
    echo "No binary release for ${target}; falling back to cargo install"
    install_from_source
  fi

  case ":${PATH}:" in
    *":${INSTALL_DIR}"*) ;;
    *) echo "Note: add ${INSTALL_DIR} to PATH if ${BIN_NAME} is not found." ;;
  esac
}

main "$@"
