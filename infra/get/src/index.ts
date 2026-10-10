const SCRIPT_CACHE = "public, max-age=300";
const DOWNLOAD_BASE = "https://dl.everyinit.dev";
const publicKey = `-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAxnwluZOtr/JF1bVunveJa1VgLwuahtVdJT4AAEGEBQU=
-----END PUBLIC KEY-----`;

function installer(packages: string[]): string {
  const initialPackages = JSON.stringify(packages.join(" "));
  return `#!/bin/sh
set -eu

BASE_URL="\${EVIN_DOWNLOAD_BASE:-${DOWNLOAD_BASE}}"
PACKAGES=${initialPackages}
CHANNEL=canary
PREFIX="\${HOME:-}/.local"
PREFIX_SET=0

usage() {
  cat <<'USAGE'
Usage: install.sh [--with-esh] [--channel canary|latest] [--prefix DIR]
       install.sh --help
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --with-esh)
      case " $PACKAGES " in *" evin "*) PACKAGES="evin esh" ;; esac
      shift
      ;;
    --channel)
      [ "$#" -ge 2 ] || { echo "--channel needs a value" >&2; exit 2; }
      CHANNEL="$2"
      shift 2
      ;;
    --prefix)
      [ "$#" -ge 2 ] || { echo "--prefix needs a directory" >&2; exit 2; }
      PREFIX="$2"
      PREFIX_SET=1
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

case "$CHANNEL" in canary|latest) ;; *) echo "unsupported channel: $CHANNEL" >&2; exit 2 ;; esac
if [ "$PREFIX_SET" -eq 0 ] && [ -z "\${HOME:-}" ]; then
  echo "HOME is unset; provide --prefix DIR" >&2
  exit 1
fi
[ -n "$PREFIX" ] || { echo "installation prefix must not be empty" >&2; exit 2; }
case "$(uname -s)" in Linux) ;; *) echo "only Linux release artifacts are available" >&2; exit 1 ;; esac
case "$(uname -m)" in x86_64|amd64) ;; *) echo "only x86_64 release artifacts are available" >&2; exit 1 ;; esac
command -v curl >/dev/null 2>&1 || { echo "curl is required" >&2; exit 1; }
command -v tar >/dev/null 2>&1 || { echo "tar is required" >&2; exit 1; }
command -v openssl >/dev/null 2>&1 || { echo "OpenSSL is required to verify release signatures" >&2; exit 1; }
command -v grep >/dev/null 2>&1 || { echo "grep is required" >&2; exit 1; }
for tool in awk base64 chmod cp mktemp mv sed; do
  command -v "$tool" >/dev/null 2>&1 || { echo "$tool is required" >&2; exit 1; }
done
if command -v sha256sum >/dev/null 2>&1; then
  hash_file() { sha256sum "$1"; }
elif command -v shasum >/dev/null 2>&1; then
  hash_file() { shasum -a 256 "$1"; }
else
  echo "sha256sum or shasum is required" >&2
  exit 1
fi

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT
trap 'exit 1' HUP INT TERM
KEY_FILE="$TMP_DIR/release-key.pub"
cat > "$KEY_FILE" <<'EVIN_PUBLIC_KEY'
${publicKey}
EVIN_PUBLIC_KEY

install_one() {
  binary="$1"
  channel_manifest="$TMP_DIR/$binary-channel.json"
  curl -fsSL "$BASE_URL/$binary/$CHANNEL.json" -o "$channel_manifest"
  version=$(sed -n 's/^[[:space:]]*"latest_version"[[:space:]]*:[[:space:]]*"\\([^" ]*\\)"[,]*[[:space:]]*$/\\1/p' "$channel_manifest")
  printf '%s\\n' "$version" | grep -Eq '^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?$' || {
    echo "invalid $binary version in channel manifest" >&2
    return 1
  }
  target="x86_64-unknown-linux-musl"
  archive="$binary-v$version-$target.tar.gz"
  release_url="$BASE_URL/$binary/v$version"
  package_dir="$TMP_DIR/$binary"
  mkdir -p "$package_dir" "$TMP_DIR/extract-$binary"
  curl -fsSL "$release_url/SHA256SUMS" -o "$package_dir/SHA256SUMS"
  curl -fsSL "$release_url/SHA256SUMS.sig" -o "$package_dir/SHA256SUMS.sig.b64"
  curl -fsSL "$release_url/$archive" -o "$package_dir/$archive"
  base64 -d "$package_dir/SHA256SUMS.sig.b64" > "$package_dir/SHA256SUMS.sig"
  openssl pkeyutl -verify -pubin -inkey "$KEY_FILE" -rawin \\
    -in "$package_dir/SHA256SUMS" -sigfile "$package_dir/SHA256SUMS.sig" >/dev/null
  expected=$(awk -v name="$archive" '$2 == name { print $1; count++ } END { if (count != 1) exit 1 }' "$package_dir/SHA256SUMS") || {
    echo "checksum manifest does not contain exactly one entry for $archive" >&2
    return 1
  }
  actual=$(hash_file "$package_dir/$archive" | awk '{ print $1 }')
  [ "$actual" = "$expected" ] || { echo "SHA-256 mismatch for $archive" >&2; return 1; }
  tar -xzf "$package_dir/$archive" -C "$TMP_DIR/extract-$binary"
  release_root="$TMP_DIR/extract-$binary/$binary-v$version-$target"
  [ -x "$release_root/bin/$binary" ] || { echo "archive is missing executable bin/$binary" >&2; return 1; }
  [ -f "$release_root/LICENSE" ] || { echo "archive is missing LICENSE" >&2; return 1; }
  [ -d "$release_root/THIRD_PARTY_NOTICES" ] || { echo "archive is missing THIRD_PARTY_NOTICES" >&2; return 1; }
  destination="$PREFIX/bin"
  share_dir="$PREFIX/share/everyinit/$binary"
  mkdir -p "$destination" "$share_dir"
  cp "$release_root/bin/$binary" "$destination/.$binary.install.$$"
  chmod 755 "$destination/.$binary.install.$$"
  mv -f "$destination/.$binary.install.$$" "$destination/$binary"
  cp "$release_root/LICENSE" "$share_dir/.LICENSE.install.$$"
  mv -f "$share_dir/.LICENSE.install.$$" "$share_dir/LICENSE"
  notices_stage="$TMP_DIR/$binary-notices"
  cp -R "$release_root/THIRD_PARTY_NOTICES" "$notices_stage"
  rm -rf "$share_dir/THIRD_PARTY_NOTICES"
  mv "$notices_stage" "$share_dir/THIRD_PARTY_NOTICES"
  printf 'Installed %s %s to %s\\n' "$binary" "$version" "$destination/$binary"
}

for binary in $PACKAGES; do install_one "$binary"; done
case ":$PATH:" in *":$PREFIX/bin:"*) ;; *) printf 'Add %s/bin to PATH to use the installed tools.\\n' "$PREFIX" ;; esac
`;
}

export default {
  async fetch(request: Request): Promise<Response> {
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("Method not allowed\\n", {
        status: 405,
        headers: { Allow: "GET, HEAD", "Cache-Control": "no-store" },
      });
    }
    const path = new URL(request.url).pathname.replace(/\/+$/, "") || "/";
    const routes = new Map([
      ["/", ["evin"]],
      ["/evin", ["evin"]],
      ["/esh", ["esh"]],
      ["/all", ["evin", "esh"]],
    ]);
    const packages = routes.get(path);
    if (!packages) {
      return new Response("Not found\\n", {
        status: 404,
        headers: { "Cache-Control": "no-store" },
      });
    }
    const headers = {
      "Cache-Control": SCRIPT_CACHE,
      "Content-Type": "text/x-shellscript; charset=utf-8",
      "X-Content-Type-Options": "nosniff",
    };
    return new Response(request.method === "HEAD" ? null : installer(packages), {
      status: 200,
      headers,
    });
  },
} satisfies ExportedHandler<Env>;
