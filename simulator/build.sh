#!/usr/bin/env bash
# Builds the DD-L0-LA59 simulator on Linux or macOS (the counterpart of
# build.ps1).
#   Linux: builds inside Docker; nothing but Docker is needed on the host.
#   macOS (Apple Silicon only): builds natively. Needs rustup
#     (https://rustup.rs) and Xcode's command line tools. The script installs
#     nothing; it only downloads the CPython it bundles (checksum checked)
#     into simulator/.python-macos/.
#
# Stages a self-contained folder:
#   simulator/dist/ddl0-sim   the simulator
#   simulator/dist/python/    bundled CPython
# Rebuilding keeps simulator/dist/shims/ (drop-in shims), fs/ (the simulated
# CIRCUITPY drive), captures/, layouts/ and ddl0-sim.prefs.toml.
#
# After staging, tests/headless.py runs code.py against every scenario. If any
# check fails the script fails and names it, but dist/ is already built so
# the simulator can be used to debug it.
#
# Usage: simulator/build.sh [--run] [--debug] [--test] [--rebuild-image]
#   --run            launch the simulator after a successful build
#   --debug          debug profile (faster to compile, slower to run)
#   --test           also run the Rust unit tests
#   --rebuild-image  Linux: rebuild the ddl0-sim-dev image even if it exists
set -euo pipefail

IMAGE="${IMAGE:-ddl0-sim-dev}"
# Rust version on macOS; the Docker image and CI use the same.
TOOLCHAIN=1.92.0
run=0 profile=release test=0 rebuild=0
for arg in "$@"; do
  case "$arg" in
    --run) run=1 ;;
    --debug) profile=debug ;;
    --test) test=1 ;;
    --rebuild-image) rebuild=1 ;;
    -h|--help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg (see --help)" >&2; exit 2 ;;
  esac
done

sim="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$sim/.." && pwd)"
locked=0
[ -f "$sim/Cargo.lock" ] && locked=1

# docker/build.sh does the build and staging on both; CI runs the same script.
# These run under `||`, where set -e is off: a failed step before it exits.
build_linux() {
  if ! docker info >/dev/null 2>&1; then
    echo "Docker is not reachable. Start Docker (and check your user may run it), then re-run this script." >&2
    exit 1
  fi
  if [ "$rebuild" = 1 ] || ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "== Building image $IMAGE"
    docker build -t "$IMAGE" "$sim/docker" || exit 1
  fi
  echo "== Building in $IMAGE ($profile; first build takes a while)"
  # The target volume is separate from the Windows build's.
  docker run --rm \
    -v "$repo:/workspace" \
    -v ddl0-sim-cargo-registry:/usr/local/cargo/registry \
    -v ddl0-sim-target-linux:/target \
    -e CARGO_TARGET_DIR=/target \
    -e PLATFORM=linux -e "PROFILE=$profile" -e "LOCKED=$locked" -e "TEST=$test" \
    -e "OWNER=$(id -u):$(id -g)" \
    -w /workspace/simulator "$IMAGE" bash docker/build.sh
}

build_macos() {
  if [ "$(uname -m)" != arm64 ]; then
    echo "Only Apple Silicon Macs are supported (this one is $(uname -m))." >&2
    exit 1
  fi
  if ! xcode-select -p >/dev/null 2>&1; then
    echo "Xcode's command line tools are needed (the linker and codesign): run" >&2
    echo "  xcode-select --install" >&2
    echo "then re-run this script." >&2
    exit 1
  fi
  if ! command -v rustup >/dev/null 2>&1; then
    echo "rustup is needed: install it from https://rustup.rs, then re-run this script." >&2
    exit 1
  fi
  if ! rustup run "$TOOLCHAIN" cargo --version >/dev/null 2>&1; then
    echo "Rust $TOOLCHAIN is needed: run" >&2
    echo "  rustup toolchain install $TOOLCHAIN --profile minimal -c rustfmt,clippy" >&2
    echo "then re-run this script." >&2
    exit 1
  fi
  bash "$sim/docker/fetch-python-macos.sh" "$sim/.python-macos" || exit 1
  echo "== Building natively ($profile; first build takes a while)"
  (cd "$sim" && RUSTUP_TOOLCHAIN="$TOOLCHAIN" PLATFORM=macos \
    PY_MACOS="$sim/.python-macos/python" PROFILE="$profile" LOCKED="$locked" TEST="$test" \
    bash docker/build.sh)
}

status=0
case "$(uname -s)" in
  Linux) build_linux || status=$? ;;
  Darwin) build_macos || status=$? ;;
  *)
    echo "On Windows use simulator\\build.ps1." >&2
    exit 1 ;;
esac
if [ "$status" = 3 ]; then
  echo "Scenario checks failed: see the FAIL lines above." >&2
  echo "The simulator was still built in $sim/dist, so you can debug it there." >&2
  exit 3
fi
[ "$status" = 0 ] || { echo "build failed ($status)" >&2; exit "$status"; }

echo
echo "== Built: $sim/dist/ddl0-sim"

if [ "$run" = 1 ]; then
  (cd "$sim/dist" && nohup ./ddl0-sim >/dev/null 2>&1 &)
fi
