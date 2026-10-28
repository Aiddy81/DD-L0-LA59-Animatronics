#!/usr/bin/env bash
# Builds the Windows simulator inside the ddl0-sim-dev image and stages
# simulator/dist/. Run from simulator/ inside the container; build.ps1 and CI
# both call this.
#   PROFILE=release|debug (default release)   LOCKED=1 to require Cargo.lock
#   CHECK=1 also runs rustfmt --check and clippy with warnings as errors
# Ends by running code.py headless against every scenario; a failure exits 3,
# after dist/ is staged, so the built simulator can be used to debug it.
set -euo pipefail

TARGET=x86_64-pc-windows-gnu
PROFILE="${PROFILE:-release}"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
flags=(--target "$TARGET")
[ "$PROFILE" = release ] && flags+=(--release)
[ "${LOCKED:-0}" = 1 ] && flags+=(--locked)

if [ "${CHECK:-0}" = 1 ]; then
  cargo fmt --check
  cargo clippy "${flags[@]}" -- -D warnings
fi

cargo build "${flags[@]}"

# Restage from scratch, but keep what the user saved from the app, their
# drop-in shims, exported layouts and the simulated CIRCUITPY drive.
mkdir -p dist
find dist -mindepth 1 -maxdepth 1 ! -name ddl0-sim.prefs.toml ! -name captures ! -name shims ! -name fs ! -name layouts -exec rm -rf {} +
cp "$TARGET_DIR/$TARGET/$PROFILE/ddl0-sim.exe" dist/
cp -r /opt/python-embed/. dist/
for f in /usr/lib/gcc/x86_64-w64-mingw32/*-posix/libstdc++-6.dll \
         /usr/lib/gcc/x86_64-w64-mingw32/*-posix/libgcc_s_seh-1.dll \
         /usr/x86_64-w64-mingw32/lib/libwinpthread-1.dll; do
  cp "$f" dist/
done

echo "== Scenario checks (code.py headless)"
# No __pycache__ in the mounted checkout (it would be root-owned).
PYTHONDONTWRITEBYTECODE=1 python3 tests/headless.py || {
  echo "Scenario checks failed: see the FAIL lines above."
  exit 3
}
