#!/usr/bin/env bash
# Builds the simulator inside the ddl0-sim-dev image and stages
# simulator/dist/. Run from simulator/ inside the container; build.ps1,
# build.sh and CI all call this.
#   PLATFORM=windows|linux (default windows)
#   PROFILE=release|debug (default release)   LOCKED=1 to require Cargo.lock
#   CHECK=1 also runs rustfmt --check and clippy with warnings as errors
#   TEST=1 also builds the Rust unit tests.
#          windows: stages them as dist/ddl0-sim-tests.exe. They are a Windows
#          exe, so they can't run here: build.ps1 -Test and CI's Windows job
#          run it from dist/, beside the Python and MinGW DLLs it needs.
#          linux: runs them here, against the image's Python.
#   OWNER=uid:gid hands dist/ to that user when done (the container is root).
# Ends by running code.py headless against every scenario; a failure exits 3,
# after dist/ is staged, so the built simulator can be used to debug it.
set -euo pipefail

PLATFORM="${PLATFORM:-windows}"
case "$PLATFORM" in
  windows) TARGET=x86_64-pc-windows-gnu; EXE=ddl0-sim.exe ;;
  linux)
    TARGET=x86_64-unknown-linux-gnu; EXE=ddl0-sim
    PY_LINUX=/opt/python-linux
    export PYO3_PYTHON="$PY_LINUX/bin/python3"
    # -L: this Python reports the library folder it was built in
    # (/install/lib), not where it is. rpath: the exe loads libpython from
    # dist/python/lib, beside it. With an explicit --target these flags reach
    # only target code, not build scripts.
    export RUSTFLAGS="${RUSTFLAGS:-} -L native=$PY_LINUX/lib -C link-arg=-Wl,-rpath,\$ORIGIN/python/lib"
    ;;
  *) echo "PLATFORM must be windows or linux, not '$PLATFORM'" >&2; exit 2 ;;
esac

if [ -n "${OWNER:-}" ]; then
  trap 'chown -R "$OWNER" dist 2>/dev/null || true' EXIT
fi

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

tests_exe=
if [ "${TEST:-0}" = 1 ] && [ "$PLATFORM" = linux ]; then
  # Before staging, so a failing test stops the build. Tests that start
  # Python need to find its library and stdlib in the image.
  LD_LIBRARY_PATH="$PY_LINUX/lib" PYTHONHOME="$PY_LINUX" cargo test "${flags[@]}"
elif [ "${TEST:-0}" = 1 ]; then
  # Cargo names the test exe with a hash; its JSON output says which it is.
  tests_exe=$(cargo test "${flags[@]}" --no-run --message-format=json | python3 -c '
import json, sys
exes = [m["executable"] for m in map(json.loads, sys.stdin)
        if m.get("reason") == "compiler-artifact" and m.get("executable")
        and m["profile"]["test"]]
if len(exes) != 1:
    sys.exit(f"expected one unit-test exe, got {exes}")
print(exes[0])
')
fi

# Restage from scratch, but keep what the user saved from the app, their
# drop-in shims, exported layouts and the simulated CIRCUITPY drive.
mkdir -p dist
find dist -mindepth 1 -maxdepth 1 ! -name ddl0-sim.prefs.toml ! -name captures ! -name shims ! -name fs ! -name layouts -exec rm -rf {} +
cp "$TARGET_DIR/$TARGET/$PROFILE/$EXE" dist/
if [ "$PLATFORM" = linux ]; then
  # The app sets PYTHONHOME to dist/python. Left out, as the firmware never
  # needs them: headers, the interpreter binaries, the link-time libpython
  # name, Tcl/Tk, IDLE, pip and the stdlib's own tests. share/ is terminfo,
  # whose names differ only in case (they'd clash in a Windows checkout).
  # Symlinks are copied as files, which a Windows checkout can also hold.
  mkdir dist/python
  tar -C "$PY_LINUX" -chf - \
      --exclude=./include --exclude=./bin --exclude=./share \
      --exclude=./lib/pkgconfig --exclude=./lib/libpython3.13.so \
      --exclude='./lib/tcl*' --exclude='./lib/tk*' --exclude='./lib/Tix*' \
      --exclude='./lib/itcl*' --exclude='./lib/thread*' \
      --exclude='./lib/python3.*/test' --exclude='./lib/python3.*/idlelib' \
      --exclude='./lib/python3.*/tkinter' --exclude='./lib/python3.*/turtledemo' \
      --exclude='./lib/python3.*/ensurepip' --exclude='./lib/python3.*/site-packages/*' \
      --exclude='./lib/python3.*/lib-dynload/_tkinter*' . \
    | tar -C dist/python -xf -
  # Shipped with debug symbols, which are most of its 240 MB.
  strip --strip-debug dist/python/lib/libpython3.13.so.1.0
else
  if [ -n "$tests_exe" ]; then cp "$tests_exe" dist/ddl0-sim-tests.exe; fi
  cp -r /opt/python-embed/. dist/
  for f in /usr/lib/gcc/x86_64-w64-mingw32/*-posix/libstdc++-6.dll \
           /usr/lib/gcc/x86_64-w64-mingw32/*-posix/libgcc_s_seh-1.dll \
           /usr/x86_64-w64-mingw32/lib/libwinpthread-1.dll; do
    cp "$f" dist/
  done
fi

echo "== Scenario checks (code.py headless)"
# No __pycache__ in the mounted checkout (it would be root-owned).
PYTHONDONTWRITEBYTECODE=1 python3 tests/headless.py || {
  echo "Scenario checks failed: see the FAIL lines above."
  exit 3
}
