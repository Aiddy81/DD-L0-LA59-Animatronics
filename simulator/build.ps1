<#
.SYNOPSIS
    Build the DD-L0-LA59 simulator for Windows inside Docker.

.DESCRIPTION
    Nothing is installed on the host. The ddl0-sim-dev image cross-compiles
    with MinGW (x86_64-pc-windows-gnu) and stages a self-contained folder:

      simulator\dist\ddl0-sim.exe      the simulator
      simulator\dist\python3*.dll ...  official embeddable Python (from the image)
      simulator\dist\lib*.dll          MinGW runtime

    Cargo's registry and target dirs live in Docker volumes, so the multi-GB
    Bevy build never lands in OneDrive.

.PARAMETER RebuildImage
    Rebuild the ddl0-sim-dev image even if it exists.

.PARAMETER Run
    Launch the simulator after a successful build.

.PARAMETER DebugBuild
    Debug profile instead of release (faster to compile, slower to run).

.EXAMPLE
    .\simulator\build.ps1 -Run
#>
[CmdletBinding()]
param(
    [switch]$RebuildImage,
    [switch]$Run,
    [switch]$DebugBuild,
    [string]$Image = 'ddl0-sim-dev'
)

$ErrorActionPreference = 'Stop'
$sim = $PSScriptRoot
$repo = (Resolve-Path (Join-Path $sim '..')).Path
$dist = Join-Path $sim 'dist'
$target = 'x86_64-pc-windows-gnu'
$profileDir = if ($DebugBuild) { 'debug' } else { 'release' }
$profileFlag = if ($DebugBuild) { '' } else { '--release' }

docker info *> $null
if ($LASTEXITCODE -ne 0) {
    throw (@(
        'Docker is not reachable. Start Docker Desktop, wait for `docker info`',
        'to succeed, then re-run this script.'
    ) -join [Environment]::NewLine)
}

docker image inspect $Image *> $null
if ($RebuildImage -or $LASTEXITCODE -ne 0) {
    Write-Host "== Building image $Image" -ForegroundColor Cyan
    docker build -t $Image (Join-Path $sim 'docker')
    if ($LASTEXITCODE -ne 0) { throw "docker build failed ($LASTEXITCODE)" }
}

# A running simulator locks its exe; the copy would then fail.
Get-Process ddl0-sim -ErrorAction SilentlyContinue | Stop-Process -Force

$locked = if (Test-Path (Join-Path $sim 'Cargo.lock')) { '--locked' } else { '' }

# Single-quoted: these are bash variables, expanded in the container.
$script = 'set -e
cargo build $PROFILE_FLAG $LOCKED --target $TARGET
rm -rf dist && mkdir -p dist
cp "/target/$TARGET/$PROFILE_DIR/ddl0-sim.exe" dist/
cp -r /opt/python-embed/. dist/
for f in /usr/lib/gcc/x86_64-w64-mingw32/*-posix/libstdc++-6.dll \
         /usr/lib/gcc/x86_64-w64-mingw32/*-posix/libgcc_s_seh-1.dll \
         /usr/x86_64-w64-mingw32/lib/libwinpthread-1.dll; do
  cp "$f" dist/
done'

Write-Host "== Cross-compiling in $Image ($profileDir; first build takes a while)" -ForegroundColor Cyan
docker run --rm `
    -v "${repo}:/workspace" `
    -v ddl0-sim-cargo-registry:/usr/local/cargo/registry `
    -v ddl0-sim-target:/target `
    -e CARGO_TARGET_DIR=/target `
    -e "TARGET=$target" -e "PROFILE_DIR=$profileDir" `
    -e "PROFILE_FLAG=$profileFlag" -e "LOCKED=$locked" `
    -w /workspace/simulator $Image bash -c $script
if ($LASTEXITCODE -ne 0) { throw "build failed ($LASTEXITCODE)" }

$exe = Join-Path $dist 'ddl0-sim.exe'
Write-Host ""
Write-Host "== Built: $exe" -ForegroundColor Green

if ($Run) {
    Start-Process -FilePath $exe -WorkingDirectory $dist
}
