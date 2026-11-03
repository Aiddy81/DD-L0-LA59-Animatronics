#!/usr/bin/env bash
# Builds the DD-L0-LA59 simulator for Linux inside Docker (the Linux
# counterpart of build.ps1). Nothing but Docker is needed on the host.
#
# Stages a self-contained folder:
#   simulator/dist/ddl0-sim   the simulator
#   simulator/dist/python/    bundled CPython (from the image)
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
#   --test           also run the Rust unit tests (in the container)
#   --rebuild-image  rebuild the ddl0-sim-dev image even if it exists
set -euo pipefail

IMAGE="${IMAGE:-ddl0-sim-dev}"
run=0 profile=release test=0 rebuild=0
for arg in "$@"; do
  case "$arg" in
    --run) run=1 ;;
    --debug) profile=debug ;;
    --test) test=1 ;;
    --rebuild-image) rebuild=1 ;;
    -h|--help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg (see --help)" >&2; exit 2 ;;
  esac
done

case "$(uname -s)" in
  Linux) ;;
  Darwin)
    echo "macOS: coming in part 2 of issue #93. For now the simulator builds on Windows and Linux." >&2
    exit 1 ;;
  *)
    echo "On Windows use simulator\\build.ps1." >&2
    exit 1 ;;
esac

sim="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$sim/.." && pwd)"

if ! docker info >/dev/null 2>&1; then
  echo "Docker is not reachable. Start Docker (and check your user may run it), then re-run this script." >&2
  exit 1
fi

if [ "$rebuild" = 1 ] || ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "== Building image $IMAGE"
  docker build -t "$IMAGE" "$sim/docker"
fi

locked=0
[ -f "$sim/Cargo.lock" ] && locked=1

echo "== Building in $IMAGE ($profile; first build takes a while)"
# docker/build.sh does the build and staging; CI runs the same script.
# The target volume is separate from the Windows build's.
status=0
docker run --rm \
  -v "$repo:/workspace" \
  -v ddl0-sim-cargo-registry:/usr/local/cargo/registry \
  -v ddl0-sim-target-linux:/target \
  -e CARGO_TARGET_DIR=/target \
  -e PLATFORM=linux -e "PROFILE=$profile" -e "LOCKED=$locked" -e "TEST=$test" \
  -e "OWNER=$(id -u):$(id -g)" \
  -w /workspace/simulator "$IMAGE" bash docker/build.sh || status=$?
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
