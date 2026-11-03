#!/usr/bin/env bash
# Packs a built simulator and the firmware into release archives.
#   simulator/release/package.sh <platform> <version> [out_dir] [dist_dir]
# Run from the repo root after a build has staged dist_dir (default
# simulator/dist/). Platforms: windows, linux, macos-arm64. Writes
#   ddl0-sim-windows-<version>.zip      the app, runnable once unpacked
#   ddl0-sim-linux-<version>.tar.gz     the same; tar keeps the exec bit
#   ddl0-sim-macos-arm64-<version>.tar.gz
#   ddl0-firmware-<version>.zip         PropMaker_Feather/'s contents, for CIRCUITPY
#   SHA256SUMS                          every ddl0-* archive in out_dir
# into out_dir (default release-out/). Run it once per platform into the same
# out_dir; each run rewrites the (identical) firmware zip and SHA256SUMS. The app finds its assets in a
# simulator/ folder above the exe and the firmware by walking up to
# PropMaker_Feather/code.py, so the app zip keeps the repo layout.
# Repo files come from `git archive HEAD`, so only tracked files ship: never a
# local log.txt, boot_out.txt or sounds/. Needs git, python3 and sha256sum.
set -euo pipefail

platforms="windows, linux, macos-arm64"
usage() { echo "usage: $0 <platform> <version> [out_dir] [dist_dir]   (platforms: $platforms)" >&2; exit 2; }
[ $# -ge 2 ] && [ $# -le 4 ] || usage
platform=$1
version=$2
out=${3:-release-out}
dist=${4:-simulator/dist}

case "$platform" in
  windows) exe=ddl0-sim.exe ext=zip ;;
  linux | macos-arm64) exe=ddl0-sim ext=tar.gz ;;
  *) echo "Unsupported platform '$platform' (supported: $platforms)" >&2; exit 2 ;;
esac
# The version ends up in file names, so keep it to a safe character set.
[[ "$version" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || { echo "Bad version '$version'" >&2; exit 2; }
[ -f "$dist/$exe" ] || { echo "$dist/$exe not found: build first" >&2; exit 1; }

app="ddl0-sim-$platform-$version"
fw="ddl0-firmware-$version"
mkdir -p "$out"
out=$(cd "$out" && pwd)
stage="$out/stage"
# Clear only what this script writes, never the rest of out_dir.
rm -rf "$stage" "$out/$app.$ext" "$out/$fw.zip" "$out/SHA256SUMS"
mkdir -p "$stage/$app" "$stage/$fw"

git archive HEAD -- PropMaker_Feather simulator/assets simulator/shims simulator/scenarios \
  simulator/docs simulator/sim.toml simulator/parts.example.toml README.md LICENSE \
  | tar -x -C "$stage/$app"
# The Blender source of the model isn't needed to run the app.
rm -rf "$stage/$app/simulator/assets/src"

# dist/ minus the tests exe and what the user saves beside the exe
# (prefs, captures, the simulated drive, layouts, drop-in shims).
mkdir -p "$stage/$app/simulator/dist"
tar -C "$dist" \
  --exclude=./ddl0-sim-tests.exe --exclude=./ddl0-sim.prefs.toml \
  --exclude=./captures --exclude=./fs --exclude=./layouts --exclude=./shims \
  -cf - . | tar -x -C "$stage/$app/simulator/dist"

git archive HEAD:PropMaker_Feather | tar -x -C "$stage/$fw"

# zip isn't in the dev image; Python's zipfile is. The app archive has its
# folder at the top; the firmware zip holds the files themselves, for
# CIRCUITPY's root. In a tar.gz only the exe is executable: an artifact
# download has lost every mode, and nothing else needs one.
python3 - "$stage" "$out" "$app" "$fw" "$ext" "$exe" <<'EOF'
import os, sys, tarfile, zipfile
stage, out, app, fw, ext, exe = sys.argv[1:]

def pack(src, dest, prefix):
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for root, dirs, files in os.walk(src):
            dirs.sort()
            for name in sorted(files):
                path = os.path.join(root, name)
                arc = os.path.relpath(path, src).replace(os.sep, "/")
                z.write(path, prefix + arc)

def pack_tar(src, dest, prefix):
    exe_arc = prefix + "simulator/dist/" + exe

    def normalise(info):
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        info.mode = 0o755 if info.isdir() or info.name == exe_arc else 0o644
        return info

    with tarfile.open(dest, "w:gz", compresslevel=9, dereference=True) as t:
        for root, dirs, files in os.walk(src):
            dirs.sort()
            for name in dirs + sorted(files):
                path = os.path.join(root, name)
                arc = prefix + os.path.relpath(path, src).replace(os.sep, "/")
                t.add(path, arc, recursive=False, filter=normalise)

src = os.path.join(stage, app)
if ext == "zip":
    pack(src, os.path.join(out, app + ".zip"), app + "/")
else:
    pack_tar(src, os.path.join(out, app + ".tar.gz"), app + "/")
pack(os.path.join(stage, fw), os.path.join(out, fw + ".zip"), "")
EOF

# Fail the run if the app archive is missing what it needs or ships what it
# mustn't.
python3 - "$out/$app.$ext" "$app" "$exe" <<'EOF'
import sys, tarfile, zipfile
path, app, exe = sys.argv[1:]
dist = f"{app}/simulator/dist/"
problems = []
if path.endswith(".zip"):
    names = set(zipfile.ZipFile(path).namelist())
else:
    with tarfile.open(path) as t:
        members = {m.name: m for m in t.getmembers()}
    names = {n + "/" if m.isdir() else n for n, m in members.items()}
    exe_entry = members.get(dist + exe)
    if exe_entry is not None and not exe_entry.mode & 0o111:
        problems.append(f"{dist + exe} isn't executable")
need = [dist + exe, f"{app}/simulator/assets/droid.glb", f"{app}/simulator/sim.toml",
        f"{app}/simulator/shims/board.py", f"{app}/PropMaker_Feather/code.py",
        f"{app}/README.md", f"{app}/LICENSE"]
banned = ("ddl0-sim-tests", "ddl0-sim.prefs.toml", "captures/", "fs/", "layouts/", "shims/")
problems += [f"missing {n}" for n in need if n not in names]
problems += [f"must not ship {n}" for n in names
             if n.startswith(dist) and n[len(dist):].startswith(banned)]
problems += [f"must not ship {n}" for n in names if "/assets/src/" in n]
if problems:
    sys.exit("Package check failed:\n  " + "\n  ".join(problems))
print(f"{path}: {len(names)} entries, layout OK")
EOF

rm -rf "$stage"
(cd "$out" && shopt -s nullglob && sha256sum ddl0-*.zip ddl0-*.tar.gz > SHA256SUMS && cat SHA256SUMS)
