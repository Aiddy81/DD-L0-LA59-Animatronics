#!/usr/bin/env bash
# Packs a built simulator and the firmware into release zips.
#   simulator/release/package.sh <platform> <version> [out_dir]
# Run from the repo root after the build has staged simulator/dist/. Writes
#   ddl0-sim-<platform>-<version>.zip   the app, runnable once unzipped
#   ddl0-firmware-<version>.zip         PropMaker_Feather/'s contents, for CIRCUITPY
#   SHA256SUMS
# into out_dir (default release-out/). The app finds its assets in a
# simulator/ folder above the exe and the firmware by walking up to
# PropMaker_Feather/code.py, so the app zip keeps the repo layout.
# Repo files come from `git archive HEAD`, so only tracked files ship: never a
# local log.txt, boot_out.txt or sounds/. Needs git, python3 and sha256sum.
set -euo pipefail

usage() { echo "usage: $0 <platform> <version> [out_dir]   (platforms: windows)" >&2; exit 2; }
[ $# -ge 2 ] && [ $# -le 3 ] || usage
platform=$1
version=$2
out=${3:-release-out}

case "$platform" in
  windows) exe=ddl0-sim.exe ;;
  *) echo "Unsupported platform '$platform' (supported: windows)" >&2; exit 2 ;;
esac
# The version ends up in file names, so keep it to a safe character set.
[[ "$version" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || { echo "Bad version '$version'" >&2; exit 2; }
[ -f "simulator/dist/$exe" ] || { echo "simulator/dist/$exe not found: build first" >&2; exit 1; }

app="ddl0-sim-$platform-$version"
fw="ddl0-firmware-$version"
mkdir -p "$out"
out=$(cd "$out" && pwd)
stage="$out/stage"
# Clear only what this script writes, never the rest of out_dir.
rm -rf "$stage" "$out/$app.zip" "$out/$fw.zip" "$out/SHA256SUMS"
mkdir -p "$stage/$app" "$stage/$fw"

git archive HEAD -- PropMaker_Feather simulator/assets simulator/shims simulator/scenarios \
  simulator/docs simulator/sim.toml simulator/parts.example.toml README.md LICENSE \
  | tar -x -C "$stage/$app"
# The Blender source of the model isn't needed to run the app.
rm -rf "$stage/$app/simulator/assets/src"

# dist/ minus the tests exe and what the user saves beside the exe
# (prefs, captures, the simulated drive, layouts, drop-in shims).
mkdir -p "$stage/$app/simulator/dist"
tar -C simulator/dist \
  --exclude=./ddl0-sim-tests.exe --exclude=./ddl0-sim.prefs.toml \
  --exclude=./captures --exclude=./fs --exclude=./layouts --exclude=./shims \
  -cf - . | tar -x -C "$stage/$app/simulator/dist"

git archive HEAD:PropMaker_Feather | tar -x -C "$stage/$fw"

# zip isn't in the dev image; Python's zipfile is. The app zip has its folder
# at the top; the firmware zip holds the files themselves, for CIRCUITPY's root.
python3 - "$stage" "$out" "$app" "$fw" <<'EOF'
import os, sys, zipfile
stage, out, app, fw = sys.argv[1:]

def pack(src, dest, prefix):
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for root, dirs, files in os.walk(src):
            dirs.sort()
            for name in sorted(files):
                path = os.path.join(root, name)
                arc = os.path.relpath(path, src).replace(os.sep, "/")
                z.write(path, prefix + arc)

pack(os.path.join(stage, app), os.path.join(out, app + ".zip"), app + "/")
pack(os.path.join(stage, fw), os.path.join(out, fw + ".zip"), "")
EOF

# Fail the run if the app zip is missing what it needs or ships what it mustn't.
python3 - "$out/$app.zip" "$app" "$exe" <<'EOF'
import sys, zipfile
zip_path, app, exe = sys.argv[1:]
names = set(zipfile.ZipFile(zip_path).namelist())
dist = f"{app}/simulator/dist/"
need = [dist + exe, f"{app}/simulator/assets/droid.glb", f"{app}/simulator/sim.toml",
        f"{app}/simulator/shims/board.py", f"{app}/PropMaker_Feather/code.py",
        f"{app}/README.md", f"{app}/LICENSE"]
banned = ("ddl0-sim-tests", "ddl0-sim.prefs.toml", "captures/", "fs/", "layouts/", "shims/")
problems = [f"missing {n}" for n in need if n not in names]
problems += [f"must not ship {n}" for n in names
             if n.startswith(dist) and n[len(dist):].startswith(banned)]
problems += [f"must not ship {n}" for n in names if "/assets/src/" in n]
if problems:
    sys.exit("Package check failed:\n  " + "\n  ".join(problems))
print(f"{zip_path}: {len(names)} files, layout OK")
EOF

rm -rf "$stage"
(cd "$out" && sha256sum "$app.zip" "$fw.zip" > SHA256SUMS && cat SHA256SUMS)
