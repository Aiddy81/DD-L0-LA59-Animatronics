# Building the simulator

> **You'll need:** Windows 10/11, [Docker Desktop](https://www.docker.com/products/docker-desktop/) running, Git · **Time:** 20–40 minutes the first time, a few minutes after

You don't need Rust, Python or anything else on your PC. Everything is built inside a Docker container.

## Steps

1. Start **Docker Desktop** and wait until it says it's running.
2. Clone the project and switch to the `Documentation` branch (where work happens):

   ```powershell
   git clone https://github.com/Aiddy81/DD-L0-LA59-Animatronics.git
   ```

   ```powershell
   cd DD-L0-LA59-Animatronics
   ```

   ```powershell
   git checkout Documentation
   ```

3. Build it and start it:

   ```powershell
   .\simulator\build.ps1 -Run
   ```

4. Next time, just run the exe:

   ```powershell
   .\simulator\dist\ddl0-sim.exe
   ```

### ✅ It should look like this

The build ends with every scenario passing, and the simulator opens. `simulator\dist\` now holds `ddl0-sim.exe`, the embeddable Python and the DLLs it needs.

## Build options

| Switch | Does |
|---|---|
| `-Run` | Start the simulator after building. |
| `-Test` | Also build and run the Rust unit tests (`ddl0-sim-tests.exe`). |
| `-DebugBuild` | Debug build instead of release. |
| `-RebuildImage` | Rebuild the Docker dev image (after a `Dockerfile` change). |
| `-Image <name>` | Use another image name (default `ddl0-sim-dev`). |

## ⚠️ Something went wrong?

| Problem | Fix |
|---|---|
| *Docker is not reachable* | Start Docker Desktop and wait for `docker info` to work, then run the script again. |
| *Docker Desktop is unable to start* | Restart Docker Desktop (or Windows). Check WSL 2 is installed and up to date: `wsl --update`. |
| The build ends with `FAIL` lines | A firmware change broke a behaviour or scenario. The exe is still built, so open it to see what went wrong. |
| Running scripts is disabled | Allow scripts for this window: `Set-ExecutionPolicy -Scope Process Bypass`, then run the script again. |

## More detail (for tinkerers)

- `build.ps1` cross-compiles for `x86_64-pc-windows-gnu` with MinGW in a `rust:1.92-bookworm`-based image (`simulator/docker/`), via `docker/build.sh`.
- Cargo's registry and target dirs live in Docker volumes, so nothing large lands in your project folder (or OneDrive).
- Python comes from the official embeddable zip, downloaded inside the container and checksum-checked.
- Rebuilds keep `dist\shims\`, `fs\`, `captures\`, `layouts\` and `ddl0-sim.prefs.toml`.
- The last step runs `simulator/tests/headless.py` against the firmware and every scenario.
- `cargo fmt` and `cargo clippy` (run in the container) must be clean. See [CI and releases](CI-and-Releases).
- Building on Linux or macOS isn't supported yet ([issue #93](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/issues/93)).
