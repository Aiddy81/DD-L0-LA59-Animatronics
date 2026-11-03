# CI and releases

## What runs on every pull request

GitHub Actions runs the **Simulator** workflow (`.github/workflows/simulator.yml`) on every PR and on pushes to `main`, `dev` and `Documentation`:

| Job | What it checks |
|---|---|
| **code.py headless (CircuitPython strict)** | Runs the real firmware with no window: boop, angry, idle, the State panel's readings, the behaviour buttons, no blocked loop, then every scenario. Also `shim_dir.py`. |
| **Simulator build (Windows exe)** | Builds the exe in the Docker image with `rustfmt` and `clippy`, and attaches **ddl0-sim-windows** to the run. |
| **Rust unit tests (Windows)** | Downloads that folder on a Windows runner and runs `ddl0-sim-tests.exe`. |
| **build-linux** | Builds the Linux simulator in the same image, with `rustfmt`, `clippy`, the unit tests and the scenarios, and attaches **ddl0-sim-linux**. |
| **build-macos** | Builds natively on an Apple Silicon runner with `rustfmt`, `clippy` and the unit tests, checks the binary is signed and finds its Python, and attaches **ddl0-sim-macos-arm64**. It skips the scenarios: that runner's sleeps run 5–10× long. |
| **package** | Once every build passes, packs all the release downloads and attaches them as **ddl0-release-preview**, so packaging is tested on every PR. |

A red ❌ on any job blocks the merge. Each build is kept for 14 days.

## Run the same checks yourself

The firmware test, in the dev image (no host Python needed):

```powershell
docker run --rm -v "${PWD}:/workspace" -w /workspace ddl0-sim-dev python3 simulator/tests/headless.py
```

One scenario only:

```powershell
docker run --rm -v "${PWD}:/workspace" -w /workspace ddl0-sim-dev python3 simulator/tests/headless.py --scenario boop
```

The Rust unit tests on Windows:

```powershell
.\simulator\build.ps1 -Test
```

On Linux or a Mac:

```bash
./simulator/build.sh --test
```

## Releases

Releases are automatic. Every push to `main` (a merged `Documentation → main` pull request) publishes a [GitHub release](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/releases) once every job on that commit has passed. If any job fails, nothing is released.

- **Tags** are dates: `vYYYY.MM.DD`, then `.2`, `.3` for later releases the same day. The hand-made `v1.0.0` stays as it is.
- **Notes** list every PR merged since the last tag, grouped by branch prefix (`feature/`, `bugfix/`, `docs/`, `ci/`, other), built by `.github/scripts/release_notes.py`.
- **Downloads:**
  - `ddl0-sim-windows-<version>.zip`, `ddl0-sim-linux-<version>.tar.gz`, `ddl0-sim-macos-arm64-<version>.tar.gz`: the simulator with its Python, the firmware and the files it needs, ready to run.
  - `ddl0-firmware-<version>.zip`: the contents of `PropMaker_Feather/`, to copy to CIRCUITPY.
  - `SHA256SUMS`, also shown as a table in the notes.
- The sound files aren't in git, so no download has `sounds/`.

The full packaging details are in the [README's Releasing section](https://github.com/Aiddy81/DD-L0-LA59-Animatronics#releasing).

## The wiki

The **Wiki** workflow (`.github/workflows/wiki-sync.yml`) copies the `wiki/` folder to the GitHub wiki whenever `wiki/` changes on `main`, or when run by hand from the Actions tab. So the wiki describes what's released. It's separate from the release jobs and never blocks a release. See [Contributing and branching](Contributing-and-Branching).

## More detail (for tinkerers)

- `headless.py` needs Python 3.11+ (CI uses 3.13). It uses `simulator/tests/simhw.py`, a pure-Python `simhw`, so it runs without the Rust app.
- `settings.rs`, `wav.rs` and `deploy.rs` tests need only `rustc` and can be run alone; the [README's Tests and CI section](https://github.com/Aiddy81/DD-L0-LA59-Animatronics#tests-and-ci) has the command.
- The `release` job is the only one in `simulator.yml` with write access, uses only `GITHUB_TOKEN`, and runs one at a time. Every action is pinned to a commit SHA.
