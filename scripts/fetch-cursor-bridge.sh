#!/usr/bin/env bash
# Installs the pinned Cursor SDK Bridge (ADR-003) where livstyle looks for it:
#   ${XDG_DATA_HOME:-~/.local/share}/livstyle/cursor-sdk-bridge/<version>/
# Hashes are pinned here, not taken from the downloaded SHA256SUMS.txt.
set -euo pipefail

VERSION="v1.0.38"
BASE="https://github.com/cursor/sdk-bridge/releases/download/${VERSION}"

declare -A PINNED=(
  [linux-x64]=a36e2b6180865d997c1d903f1606767d01162be745633e478b8d80957898eb1d
  [linux-arm64]=44b9855bd06f7788a30df70646b87e6f06d16f371985d23ea187b8bdaf3c46f2
  [darwin-x64]=06eade399dbf5838b867fd83eab663540c8e9cd130075caf96ae1ab4305965a5
  [darwin-arm64]=6b6380647b6e05bd64b3b60e77fcdb7ac8ad161fee66b7cbc4a68b2457670b12
  [win32-x64]=56135dd4a71145dbbda231319198e270085f639b4b5265b459268625f0485683
)

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=darwin ;;
  MINGW* | MSYS* | CYGWIN*) os=win32 ;;
  *) echo "unsupported OS $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) echo "unsupported arch $(uname -m)" >&2; exit 1 ;;
esac
platform="${os}-${arch}"
expected="${PINNED[$platform]:-}"
[[ -n "$expected" ]] || { echo "no pinned build for $platform" >&2; exit 1; }

dest="${XDG_DATA_HOME:-$HOME/.local/share}/livstyle/cursor-sdk-bridge/${VERSION}"
archive="cursor-sdk-bridge-standalone-${platform}.tar.gz"
mkdir -p "$dest"

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

if [[ ! -f "$dest/$archive" ]] || [[ "$(sha256 "$dest/$archive")" != "$expected" ]]; then
  echo "downloading $archive"
  curl --fail --location --proto '=https' --output "$dest/$archive.part" "$BASE/$archive"
  mv "$dest/$archive.part" "$dest/$archive"
fi

actual="$(sha256 "$dest/$archive")"
if [[ "$actual" != "$expected" ]]; then
  rm -f "$dest/$archive"
  echo "checksum mismatch for $archive: got $actual, expected $expected" >&2
  exit 1
fi

tar -xzf "$dest/$archive" -C "$dest"
protocol="$(sed -n 's/.*"protocol": *"\([^"]*\)".*/\1/p' "$dest/manifest.json")"
if [[ "$protocol" != "sdk.v1" ]]; then
  echo "unexpected protocol ${protocol:-<none>} in manifest.json" >&2
  exit 1
fi
echo "installed cursor-sdk-bridge ${VERSION} (${platform}) to $dest"
echo "verify with: livstyle cursor smoke --offline"
