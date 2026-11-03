# Project layout

```
PropMaker_Feather/        the firmware: copied to the CIRCUITPY drive
  code.py                 entry point: boot, then start every behaviour
  hardware.py             board setup, PCA9685 channel map, LEDs, audio, shared events
  config.py               tunables from settings.toml, with defaults
  settings.toml           the tunables (whole numbers and quoted strings only)
  log.py boot.py log_off.py   serial output, optional log.txt
  behaviours/             boot, breathing, motion, attention, trench_run, leds
  sounds/                 WAV files (not in git: add your own)
  lib/                    Adafruit .mpy libraries (copied by hand; don't edit)
simulator/                the desktop simulator (Windows, Linux, macOS)
  src/                    Rust (Bevy + egui + PyO3)
  shims/                  Python stand-ins for the CircuitPython hardware modules
  scenarios/              scripted tests (TOML)
  tests/                  headless.py, shim_dir.py, simhw.py
  docs/custom-shims.md    guide to writing shims
  assets/                 3D model (droid.glb) and its Blender source
  docker/                 dev image and build script
  build.ps1               build entry point on Windows
  build.sh                build entry point on Linux and macOS
  release/package.sh      packs the release downloads
  sim.toml                wing mapping, servo and board timing
  parts.example.toml      template for extra parts
  dist/                   build output (not in git)
wiki/                     these wiki pages (synced to the GitHub wiki)
tasks/                    todo.md (work log) and lessons.md
.github/workflows/        CI and releases (simulator.yml), wiki publishing (wiki-sync.yml)
.github/scripts/          release_notes.py
```

## Where to change what

| I want to… | Edit |
|---|---|
| Change a timing or threshold | `PropMaker_Feather/settings.toml` ([Settings reference](Settings-Reference)) |
| Change how a behaviour moves | `PropMaker_Feather/behaviours/<name>.py` |
| Add a new tunable | `config.py` (`_int(...)`) and `settings.toml`, then the [Settings reference](Settings-Reference) |
| Change which channel a part is on | `hardware.py`. ⚠️ Must match the wiring. |
| Show new hardware in the simulator | `simulator/parts.toml` and maybe a shim ([Adding new hardware](Adding-New-Hardware)) |
| Change how wings map to the 3D model | `simulator/sim.toml` |
| Change the simulator app | `simulator/src/` ([Simulator architecture](Simulator-Architecture)) |
| Add a test | `simulator/scenarios/*.toml` |

## More detail (for tinkerers)

- The firmware is never modified to run in the simulator; the shims mirror only the CircuitPython and Adafruit API surface it uses.
- The root `CLAUDE.md` and `.github/copilot-instructions.md` hold guidance for AI coding assistants.
