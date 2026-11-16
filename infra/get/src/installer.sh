#!/bin/sh
# Copyright (C) 2026 Eduardo Marques Santos
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, version 3.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
# Everyinit release installer
# Supports: Linux x86_64
set -eu

# -----------------------------------------------------------------------------
# Default Configuration
# -----------------------------------------------------------------------------
DEFAULT_BASE_URL="https://dl.everyinit.dev"
BASE_URL="${EVIN_DOWNLOAD_BASE:-$DEFAULT_BASE_URL}"
CHANNEL="canary"
PACKAGE_SELECTION=""
VERSION_OVERRIDE=""
PREFIX="${HOME:-}/.local"
PREFIX_SET=0
PACKAGES=""
TARGET="x86_64-unknown-linux-musl"

TMP_DIR=""
CURRENT_BINARY=""
CHANNEL_MANIFEST=""
CHANNEL_VERSION=""

# -----------------------------------------------------------------------------
# Release Public Key
# Pinned key matches infra/get/release-key.pub
# -----------------------------------------------------------------------------
write_release_key() {
  cat > "$1" <<'PUBLIC_KEY'
-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEASEIJSsmmbNb1hJFIXewhdIb8hVVD4SkhtEWjmbKExYY=
-----END PUBLIC KEY-----
PUBLIC_KEY
}

# -----------------------------------------------------------------------------
# Logging and Error Handling
# -----------------------------------------------------------------------------
warn() {
  printf '%s\n' "$1" >&2
}

die() {
  warn "$1"
  exit "${2:-1}"
}

usage() {
  cat <<'USAGE'
Usage: install.sh --package evin|esh|all [options]

Options:
  --package NAME     Select evin, esh, or all (required)
  --channel NAME     Select the canary channel (default: canary)
  --version VERSION  Install a specific version from the selected channel
  --prefix DIR       Install under DIR (default: ~/.local)
  --base-url URL     Use another release mirror
  -h, --help         Show this help

Examples:
  install.sh --package evin
  install.sh --package all --channel canary
USAGE
}

cleanup() {
  trap - EXIT HUP INT TERM
  if [ -n "${TMP_DIR:-}" ] && [ -d "$TMP_DIR" ]; then
    rm -rf "$TMP_DIR"
  fi
  if [ -n "${CURRENT_BINARY:-}" ] && [ -n "${PREFIX:-}" ]; then
    rm -f "$PREFIX/bin/.$CURRENT_BINARY.install.$$" 2>/dev/null || true
    rm -f "$PREFIX/share/everyinit/$CURRENT_BINARY/.LICENSE.install.$$" 2>/dev/null || true
    rm -f "$PREFIX/share/everyinit/$CURRENT_BINARY/.THIRD_PARTY_NOTICES.md.install.$$" 2>/dev/null || true
    rm -rf "$PREFIX/share/everyinit/$CURRENT_BINARY/.LICENSES.install.$$" 2>/dev/null || true
  fi
}

# -----------------------------------------------------------------------------
# Validation and Environment Checks
# -----------------------------------------------------------------------------
check_platform() {
  os=$(uname -s 2>/dev/null || true)
  if [ "$os" != "Linux" ]; then
    die "only Linux release artifacts are available" 1
  fi

  arch=$(uname -m 2>/dev/null || true)
  case "$arch" in
    x86_64|amd64)
      ;;
    *)
      die "only x86_64 release artifacts are available" 1
      ;;
  esac
}

check_prerequisites() {
  command -v curl >/dev/null 2>&1 || die "curl is required" 1
  command -v tar >/dev/null 2>&1 || die "tar is required" 1
  command -v openssl >/dev/null 2>&1 || die "OpenSSL is required to verify release signatures" 1
  command -v grep >/dev/null 2>&1 || die "grep is required" 1

  for tool in awk base64 chmod cp mktemp mv sed; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool is required" 1
  done

  if command -v sha256sum >/dev/null 2>&1; then
    hash_file() {
      sha256sum "$1"
    }
  elif command -v shasum >/dev/null 2>&1; then
    hash_file() {
      shasum -a 256 "$1"
    }
  else
    die "sha256sum or shasum is required" 1
  fi
}

decode_base64() {
  if command -v base64 >/dev/null 2>&1; then
    base64 -d < "$1" > "$2"
  else
    openssl base64 -d -A -in "$1" -out "$2"
  fi
}

# -----------------------------------------------------------------------------
# Argument Parsing and Configuration Validation
# -----------------------------------------------------------------------------
parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --package)
        [ "$#" -ge 2 ] || die "--package needs a value" 2
        PACKAGE_SELECTION=$2
        shift 2
        ;;
      --package=*)
        PACKAGE_SELECTION=${1#*=}
        shift
        ;;
      --channel)
        [ "$#" -ge 2 ] || die "--channel needs a value" 2
        CHANNEL=$2
        shift 2
        ;;
      --channel=*)
        CHANNEL=${1#*=}
        shift
        ;;
      --version)
        [ "$#" -ge 2 ] || die "--version needs a value" 2
        VERSION_OVERRIDE=$2
        shift 2
        ;;
      --version=*)
        VERSION_OVERRIDE=${1#*=}
        shift
        ;;
      --prefix)
        [ "$#" -ge 2 ] || die "--prefix needs a directory" 2
        PREFIX=$2
        PREFIX_SET=1
        shift 2
        ;;
      --prefix=*)
        PREFIX=${1#*=}
        PREFIX_SET=1
        shift
        ;;
      --base-url)
        [ "$#" -ge 2 ] || die "--base-url needs a value" 2
        BASE_URL=$2
        shift 2
        ;;
      --base-url=*)
        BASE_URL=${1#*=}
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      --)
        shift
        break
        ;;
      *)
        warn "unknown option: $1"
        usage >&2
        exit 2
        ;;
    esac
  done

  if [ "$#" -gt 0 ]; then
    warn "unexpected argument: $1"
    usage >&2
    exit 2
  fi
}

validate_config() {
  case "$PACKAGE_SELECTION" in
    evin)
      PACKAGES="evin"
      ;;
    esh)
      PACKAGES="esh"
      ;;
    all)
      PACKAGES="evin esh"
      ;;
    *)
      warn "--package must be evin, esh, or all"
      usage >&2
      exit 2
      ;;
  esac

  case "$CHANNEL" in
    canary)
      ;;
    *)
      die "unsupported channel: $CHANNEL" 2
      ;;
  esac

  if [ -n "$VERSION_OVERRIDE" ] && ! printf '%s\n' "$VERSION_OVERRIDE" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
    die "invalid requested version: $VERSION_OVERRIDE" 2
  fi

  if [ "$PREFIX_SET" -eq 0 ] && [ -z "${HOME:-}" ]; then
    die "HOME is unset; provide --prefix DIR" 1
  fi

  if [ -z "$PREFIX" ]; then
    die "installation prefix must not be empty" 2
  fi

  case "$PREFIX" in
    */)
      [ "$PREFIX" != "/" ] && PREFIX="${PREFIX%/}"
      ;;
  esac
}

# -----------------------------------------------------------------------------
# Installation
# -----------------------------------------------------------------------------
install_package() {
  pkg_name=$1
  CURRENT_BINARY="$pkg_name"

  version=$VERSION_OVERRIDE
  if [ -z "$version" ]; then
    version=$(sed -n "/\"$pkg_name\"[[:space:]]*:/,/}/s/.*\"version\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" "$CHANNEL_MANIFEST" | head -n 1)
  fi
  if [ -z "$version" ] || ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
    warn "invalid $pkg_name version in channel manifest"
    return 1
  fi
  if [ -n "$CHANNEL_VERSION" ] && [ "$version" != "$CHANNEL_VERSION" ]; then
    warn "$pkg_name version does not match the shared channel version"
    return 1
  fi

  archive="$pkg_name-v$version-$TARGET.tar.gz"
  release_url="$BASE_URL/$pkg_name/$CHANNEL/v$version"
  package_dir="$TMP_DIR/$pkg_name"
  extract_dir="$TMP_DIR/extract-$pkg_name"

  mkdir -p "$package_dir" "$extract_dir"

  curl -fsSL "$release_url/SHA256SUMS" -o "$package_dir/SHA256SUMS" || {
    warn "failed to download checksum manifest from $release_url/SHA256SUMS"
    return 1
  }
  curl -fsSL "$release_url/SHA256SUMS.sig" -o "$package_dir/SHA256SUMS.sig.b64" || {
    warn "failed to download checksum signature from $release_url/SHA256SUMS.sig"
    return 1
  }
  curl -fsSL "$release_url/$archive" -o "$package_dir/$archive" || {
    warn "failed to download archive from $release_url/$archive"
    return 1
  }

  decode_base64 "$package_dir/SHA256SUMS.sig.b64" "$package_dir/SHA256SUMS.sig" || {
    warn "failed to decode release signature for $pkg_name"
    return 1
  }

  if ! openssl pkeyutl -verify -pubin -inkey "$KEY_FILE" -rawin \
    -in "$package_dir/SHA256SUMS" -sigfile "$package_dir/SHA256SUMS.sig" >/dev/null 2>&1; then
    warn "signature verification failed for $pkg_name release checksums"
    return 1
  fi

  expected=$(awk -v name="$archive" '$2 == name { print $1; count++ } END { if (count != 1) exit 1 }' "$package_dir/SHA256SUMS") || {
    warn "checksum manifest does not contain exactly one entry for $archive"
    return 1
  }

  actual=$(hash_file "$package_dir/$archive" | awk '{ print $1 }')
  if [ -z "$actual" ] || [ "$actual" != "$expected" ]; then
    warn "SHA-256 mismatch for $archive"
    return 1
  fi

  tar -xzf "$package_dir/$archive" -C "$extract_dir" || {
    warn "failed to extract $archive"
    return 1
  }

  release_root="$extract_dir/$pkg_name-v$version-$TARGET"
  [ -x "$release_root/bin/$pkg_name" ] || {
    warn "archive is missing executable bin/$pkg_name"
    return 1
  }
  [ -f "$release_root/LICENSE" ] || {
    warn "archive is missing LICENSE"
    return 1
  }
  [ -f "$release_root/THIRD_PARTY_NOTICES.md" ] || {
    warn "archive is missing THIRD_PARTY_NOTICES.md"
    return 1
  }
  [ -d "$release_root/LICENSES" ] || {
    warn "archive is missing LICENSES"
    return 1
  }

  destination="$PREFIX/bin"
  share_dir="$PREFIX/share/everyinit/$pkg_name"
  mkdir -p "$destination" "$share_dir"

  # Install binary atomically
  cp "$release_root/bin/$pkg_name" "$destination/.$pkg_name.install.$$"
  chmod 755 "$destination/.$pkg_name.install.$$"
  mv -f "$destination/.$pkg_name.install.$$" "$destination/$pkg_name"

  # Install LICENSE atomically
  cp "$release_root/LICENSE" "$share_dir/.LICENSE.install.$$"
  mv -f "$share_dir/.LICENSE.install.$$" "$share_dir/LICENSE"

  # Install THIRD_PARTY_NOTICES.md atomically
  cp "$release_root/THIRD_PARTY_NOTICES.md" "$share_dir/.THIRD_PARTY_NOTICES.md.install.$$"
  mv -f "$share_dir/.THIRD_PARTY_NOTICES.md.install.$$" "$share_dir/THIRD_PARTY_NOTICES.md"

  # Install LICENSES directory atomically within the target directory
  rm -rf "$share_dir/.LICENSES.install.$$"
  cp -R "$release_root/LICENSES" "$share_dir/.LICENSES.install.$$"
  rm -rf "$share_dir/LICENSES"
  mv "$share_dir/.LICENSES.install.$$" "$share_dir/LICENSES"

  # Clean legacy unversioned third-party notices directory if present
  rm -rf "$share_dir/THIRD_PARTY_NOTICES"

  CURRENT_BINARY=""
  printf 'Installed %s %s to %s\n' "$pkg_name" "$version" "$destination/$pkg_name"
}

check_path() {
  case ":$PATH:" in
    *":$PREFIX/bin:"*)
      ;;
    *)
      printf 'Add %s/bin to PATH to use the installed tools.\n' "$PREFIX"
      ;;
  esac
}

# -----------------------------------------------------------------------------
# Main Entrypoint
# -----------------------------------------------------------------------------
main() {
  trap cleanup EXIT
  trap 'cleanup; exit 1' HUP INT TERM

  check_platform
  check_prerequisites
  parse_args "$@"
  validate_config

  TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/everyinit-installer.XXXXXX")
  KEY_FILE="$TMP_DIR/release-key.pub"
  write_release_key "$KEY_FILE"

  if [ -z "$VERSION_OVERRIDE" ]; then
    CHANNEL_MANIFEST="$TMP_DIR/$CHANNEL.json"
    curl -fsSL "$BASE_URL/$CHANNEL.json" -o "$CHANNEL_MANIFEST" || {
      warn "failed to download $CHANNEL channel manifest from $BASE_URL/$CHANNEL.json"
      exit 1
    }
    CHANNEL_VERSION=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$CHANNEL_MANIFEST" | head -n 1)
    if [ -z "$CHANNEL_VERSION" ] || ! printf '%s\n' "$CHANNEL_VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
      die "invalid shared version in $CHANNEL channel manifest" 1
    fi
  fi

  for pkg in $PACKAGES; do
    install_package "$pkg" || exit 1
  done

  check_path
}

main "$@"
