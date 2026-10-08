#!/usr/bin/env bash
# Downloads the pinned llama.cpp release for the host target triple and installs
# it as src-tauri/binaries/llama-server-<target-triple> (the name Tauri's
# `bundle.externalBin` expects). The macOS binary loads its dylibs through
# `@rpath` = `@loader_path`, so the shared libraries are copied next to it.
set -euo pipefail

TAG="b11507"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="$here/../binaries"

triple="$(rustc -vV | sed -n 's/^host: //p')"
case "$triple" in
  aarch64-apple-darwin)      asset="llama-${TAG}-bin-macos-arm64.tar.gz" ;;
  x86_64-apple-darwin)       asset="llama-${TAG}-bin-macos-x64.tar.gz" ;;
  x86_64-unknown-linux-gnu)  asset="llama-${TAG}-bin-ubuntu-x64.tar.gz" ;;
  *) echo "unsupported host triple: $triple" >&2; exit 1 ;;
esac

if [[ -x "$dest/llama-server-$triple" ]]; then
  echo "already installed: $dest/llama-server-$triple"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
url="https://github.com/ggml-org/llama.cpp/releases/download/${TAG}/${asset}"
echo "downloading $url"
curl -fL --retry 3 -o "$tmp/$asset" "$url"
tar -xzf "$tmp/$asset" -C "$tmp"

bin="$(find "$tmp" -type f -name llama-server | head -n1)"
[[ -n "$bin" ]] || { echo "llama-server not found in $asset" >&2; exit 1; }
src="$(dirname "$bin")"

mkdir -p "$dest"
shopt -s nullglob
for lib in "$src"/lib*.dylib "$src"/lib*.so*; do
  cp -P "$lib" "$dest"/
done
cp "$bin" "$dest/llama-server-$triple"
chmod +x "$dest/llama-server-$triple"
echo "installed $dest/llama-server-$triple"
