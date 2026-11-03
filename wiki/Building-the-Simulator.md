# Building the simulator

> **You'll need:** Git, and Docker (Windows and Linux) or Rust (Mac) · **Time:** 20–40 minutes the first time, a few minutes after

Just want to use it? [Download it](Get-the-Simulator) instead. Building is for changing the simulator itself.

## 1. Get the code

Clone the project and switch to the `Documentation` branch (where work happens):

```bash
git clone https://github.com/Aiddy81/DD-L0-LA59-Animatronics.git
```

```bash
cd DD-L0-LA59-Animatronics
```

```bash
git checkout Documentation
```

## 2a. On Windows

You need [Docker Desktop](https://www.docker.com/products/docker-desktop/), running. You don't need Rust or Python: everything is built inside a container.

1. Start **Docker Desktop** and wait until it says it's running.
2. Build it and start it:

   ```powershell
   .\simulator\build.ps1 -Run
   ```

3. Next time, just run the exe:

   ```powershell
   .\simulator\dist\ddl0-sim.exe
   ```

| Switch | Does |
|---|---|
| `-Run` | Start the simulator after building. |
| `-Test` | Also build and run the Rust unit tests (`ddl0-sim-tests.exe`). |
| `-DebugBuild` | Debug build instead of release. |
| `-RebuildImage` | Rebuild the Docker dev image (after a `Dockerfile` change). |
| `-Image <name>` | Use another image name (default `ddl0-sim-dev`). |

## 2b. On Linux (x86_64)

You need Docker, and your user must be allowed to run it. The build uses the same image as Windows.

```bash
./simulator/build.sh --run
```

Then run `simulator/dist/ddl0-sim` directly. `--test` also runs the Rust unit tests, `--debug` builds the debug profile and `--rebuild-image` rebuilds the dev image. The computer running it needs a Vulkan driver, an X11 or Wayland desktop (with `libwayland-client`, `libxkbcommon` and ALSA) and an XDG desktop portal for the **🗁 Firmware** dialog.

## 2c. On a Mac (Apple Silicon)

There's no Docker step: it builds natively. You need [rustup](https://rustup.rs) and Xcode's command line tools. The script installs neither and tells you what's missing.

```bash
xcode-select --install
```

```bash
rustup toolchain install 1.92.0 --profile minimal -c rustfmt,clippy
```

```bash
./simulator/build.sh --run
```

The first build downloads the Python it bundles (checksum-checked) into `simulator/.python-macos/`. The binary is signed ad hoc, because Apple Silicon won't run an unsigned one. `--test` and `--debug` work as on Linux.

### ✅ It should look like this

The build ends with every scenario passing and the simulator opens. `simulator/dist/` now holds the program and its bundled Python.

## ⚠️ Something went wrong?

| Problem | Fix |
|---|---|
| *Docker is not reachable* | Start Docker Desktop and wait for `docker info` to work, then run the script again. |
| *Docker Desktop is unable to start* (Windows) | Restart Docker Desktop (or Windows). Check WSL 2 is up to date: `wsl --update`. |
| *permission denied* talking to Docker (Linux) | Add your user to the `docker` group, then log out and back in. |
| The Mac script says rustup or Xcode tools are missing | Install them with the commands in 2c. |
| The build ends with `FAIL` lines | A firmware change broke a behaviour or scenario. The program is still built, so open it to see what went wrong. |
| Running scripts is disabled (Windows) | Allow scripts for this window: `Set-ExecutionPolicy -Scope Process Bypass`, then run the script again. |

## More detail (for tinkerers)

- On Windows, `build.ps1` cross-compiles for `x86_64-pc-windows-gnu` with MinGW in a `rust:1.92-bookworm`-based image (`simulator/docker/`), via `docker/build.sh`. Linux uses the same script with `PLATFORM=linux`; macOS with `PLATFORM=macos`, natively.
- Cargo's registry and target dirs live in Docker volumes, so nothing large lands in your project folder (or OneDrive).
- Every platform bundles its own Python, downloaded during the build and checksum-checked, so no Python is needed on the computer that runs it.
- Rebuilds keep `dist/shims/`, `fs/`, `captures/`, `layouts/` and `ddl0-sim.prefs.toml`.
- The last step runs `simulator/tests/headless.py` against the firmware and every scenario.
- `cargo fmt` and `cargo clippy` must be clean. See [CI and releases](CI-and-Releases).
