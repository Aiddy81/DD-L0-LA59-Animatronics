# CI and releases

## What runs on every pull request

GitHub Actions runs the **Simulator** workflow (`.github/workflows/simulator.yml`) on every PR and on pushes to `main`, `dev` and `Documentation`:

| Job | What it checks |
|---|---|
| **code.py headless (CircuitPython strict)** | Runs the real firmware with no window: boop, angry, idle, the State panel's readings, the behaviour buttons, no blocked loop, then every scenario. Also `shim_dir.py`. |
| **Simulator build (Windows exe)** | Builds the exe in the Docker image with `rustfmt` and `clippy`, and attaches the **ddl0-sim-windows** folder to the run (kept 14 days). |
| **Rust unit tests (Windows)** | Downloads that folder on a Windows runner and runs `ddl0-sim-tests.exe`. |

A red ❌ on any job blocks the merge.

## Run the same checks yourself

The firmware test, in the dev image (no host Python needed):

```powershell
docker run --rm -v "${PWD}:/workspace" -w /workspace ddl0-sim-dev python3 simulator/tests/headless.py
```

One scenario only:

```powershell
docker run --rm -v "${PWD}:/workspace" -w /workspace ddl0-sim-dev python3 simulator/tests/headless.py --scenario boop
```

The Rust unit tests:

```powershell
.\simulator\build.ps1 -Test
```

## The wiki

The **Wiki** workflow (`.github/workflows/wiki-sync.yml`) copies the `wiki/` folder to the GitHub wiki whenever `wiki/` changes on `main`, or when run by hand from the Actions tab. So the wiki always describes what's released. See [Contributing and branching](Contributing-and-Branching).

## Releases

- Firmware releases are on the [Releases page](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/releases) (latest: **Robot Firmware v1.0.0**).
- Downloadable simulator releases with release notes are planned in [issue #92](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/issues/92). Until then, the **ddl0-sim-windows** artifact on a CI run is the newest build.

## More detail (for tinkerers)

- `headless.py` needs Python 3.11+ (CI uses 3.13). It uses `simulator/tests/simhw.py`, a pure-Python `simhw`, so it runs without the Rust app.
- `settings.rs`, `wav.rs` and `deploy.rs` tests need only `rustc` and can be run alone; the [README's Tests and CI section](https://github.com/Aiddy81/DD-L0-LA59-Animatronics#tests-and-ci) has the command.
