#!/usr/bin/env bash
# Downloads the CPython the macOS build bundles and links against, and checks
# its SHA-256 before unpacking:
#   simulator/docker/fetch-python-macos.sh <dest_dir>
# Leaves <dest_dir>/python/ (bin/python3, lib/libpython3.13.dylib, stdlib).
# python.org has no relocatable macOS build, so this is python-build-standalone,
# the same release as the Linux build in the Dockerfile. Checksum pinned from
# the release's SHA256SUMS and GitHub's asset digest. build.sh and CI share it.
set -euo pipefail

PY_VERSION=3.13.5
PBS_TAG=20250708
PBS_SHA256=ac3708b0e11c9377210961ccfa7c9c497564723c2ceec09e1a96b43c4bb12c2c

[ $# -eq 1 ] || { echo "usage: $0 <dest_dir>" >&2; exit 2; }
dest=$1
name="cpython-${PY_VERSION}+${PBS_TAG}-aarch64-apple-darwin-install_only.tar.gz"
stamp="$dest/python/.ddl0-$PY_VERSION-$PBS_TAG"
if [ -f "$stamp" ]; then exit 0; fi

mkdir -p "$dest"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -sSfL -o "$tmp/$name" \
  "https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_TAG}/${name}"
echo "${PBS_SHA256}  $tmp/$name" | shasum -a 256 -c -
rm -rf "$dest/python"
tar -xzf "$tmp/$name" -C "$dest"
touch "$stamp"
