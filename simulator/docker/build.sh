#!/usr/bin/env bash
# Builds the simulator inside the ddl0-sim-dev image and stages
# simulator/dist/. Run from simulator/ inside the container; build.ps1,
# build.sh and CI all call this. macOS has no container: there it runs
# natively, on an Apple Silicon Mac with rustup.
#   PLATFORM=windows|linux|macos (default windows)
#   PY_MACOS=<dir> (macos only): python-build-standalone's
#          aarch64-apple-darwin install_only build, unpacked
#   PROFILE=release|debug (default release)   LOCKED=1 to require Cargo.lock
#   CHECK=1 also runs rustfmt --check and clippy with warnings as errors
#   TEST=1 also builds the Rust unit tests.
#          windows: stages them as dist/ddl0-sim-tests.exe. They are a Windows
#          exe, so they can't run here: build.ps1 -Test and CI's Windows job
#          run it from dist/, beside the Python and MinGW DLLs it needs.
#          linux, macos: runs them here, against the bundled Python.
#   OWNER=uid:gid hands dist/ to that user when done (the container is root).
#   SCENARIOS=0 skips the scenario checks below (CI's macOS runner, whose
#          sleeps run 5-10x long, can't keep their real-time clock).
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
  macos)
    TARGET=aarch64-apple-darwin; EXE=ddl0-sim
    PY_MACOS="${PY_MACOS:?set PY_MACOS to the unpacked python-build-standalone folder}"
    PY_MACOS="$(cd "$PY_MACOS" && pwd)"
    export PYO3_PYTHON="$PY_MACOS/bin/python3"
    # As on Linux: -L because sysconfig names the build folder, and the exe
    # finds libpython in dist/python/lib through its rpath.
    export RUSTFLAGS="${RUSTFLAGS:-} -L native=$PY_MACOS/lib -C link-arg=-Wl,-rpath,@executable_path/python/lib"
    ;;
  *) echo "PLATFORM must be windows, linux or macos, not '$PLATFORM'" >&2; exit 2 ;;
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
elif [ "${TEST:-0}" = 1 ] && [ "$PLATFORM" = macos ]; then
  DYLD_LIBRARY_PATH="$PY_MACOS/lib" PYTHONHOME="$PY_MACOS" cargo test "${flags[@]}"
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
elif [ "$PLATFORM" = macos ]; then
  # Same trim as Linux. bsdtar's -h also turns symlinks into files.
  mkdir dist/python
  tar -C "$PY_MACOS" -chf - \
      --exclude=./include --exclude=./bin --exclude=./share \
      --exclude=./lib/pkgconfig \
      --exclude='./lib/tcl*' --exclude='./lib/tk*' --exclude='./lib/Tix*' \
      --exclude='./lib/itcl*' --exclude='./lib/thread*' \
      --exclude='./lib/libtcl*' --exclude='./lib/libtk*' \
      --exclude='./lib/python3.*/test' --exclude='./lib/python3.*/idlelib' \
      --exclude='./lib/python3.*/tkinter' --exclude='./lib/python3.*/turtledemo' \
      --exclude='./lib/python3.*/ensurepip' --exclude='./lib/python3.*/site-packages/*' \
      --exclude='./lib/python3.*/lib-dynload/_tkinter*' . \
    | tar -C dist/python -xf -
  # The exe must name libpython by @rpath so it's found beside it, wherever
  # the folder is unpacked. If the build recorded another path, repoint it.
  lib=libpython3.13.dylib
  rpath_name="@rpath/$lib"
  linked=$(otool -L "dist/$EXE" | awk -v lib="$lib" '$1 ~ "/" lib "$" { print $1 }')
  [ -n "$linked" ] || { echo "dist/$EXE doesn't link $lib" >&2; exit 1; }
  if [ "$linked" != "$rpath_name" ]; then
    install_name_tool -change "$linked" "$rpath_name" "dist/$EXE"
  fi
  install_name_tool -id "$rpath_name" "dist/python/lib/$lib"
  strip -S "dist/python/lib/$lib"
  # Editing a Mach-O breaks its signature, and Apple Silicon won't load an
  # unsigned one: sign both again (ad hoc, so no identity is needed).
  codesign -s - -f "dist/python/lib/$lib" "dist/$EXE"
  codesign -v "dist/python/lib/$lib" "dist/$EXE"
  otool -L "dist/$EXE" | grep -F "$rpath_name"
else
  if [ -n "$tests_exe" ]; then cp "$tests_exe" dist/ddl0-sim-tests.exe; fi
  cp -r /opt/python-embed/. dist/
  for f in /usr/lib/gcc/x86_64-w64-mingw32/*-posix/libstdc++-6.dll \
           /usr/lib/gcc/x86_64-w64-mingw32/*-posix/libgcc_s_seh-1.dll \
           /usr/x86_64-w64-mingw32/lib/libwinpthread-1.dll; do
    cp "$f" dist/
  done
fi

if [ "${SCENARIOS:-1}" = 0 ]; then
  echo "== Scenario checks skipped (SCENARIOS=0)"
  exit 0
fi
echo "== Scenario checks (code.py headless)"
# No __pycache__ in the mounted checkout (it would be root-owned).
host_python=python3
[ "$PLATFORM" = macos ] && host_python="$PY_MACOS/bin/python3"
PYTHONDONTWRITEBYTECODE=1 "$host_python" tests/headless.py || {
  echo "Scenario checks failed: see the FAIL lines above."
  exit 3
}
