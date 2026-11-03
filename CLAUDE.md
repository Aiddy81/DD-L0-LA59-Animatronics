# CLAUDE.md

This file defines how Claude should behave in this project. Follow these guidelines consistently across all sessions.

---

## Project Context

- **Project Name** — DD-L0-LA59 Animatronics
- **Purpose** — Firmware for an animatronic droid that breathes, boops, gets angry and seeks attention: servo-driven wings, RGB "eyes", a white LED, sound effects and motion sensing.
- **Hardware** — Adafruit RP2040 Prop-Maker Feather (LIS3DH accelerometer, I2S amp) · 8-Channel PWM/Servo FeatherWing (PCA9685, 50 Hz) · 2 wing servos · 2 RGB LEDs · 1 white LED.
- **Firmware Stack** — CircuitPython 9.x, `asyncio`, Adafruit libraries (`adafruit_servokit`, `adafruit_lis3dh`, `audiomixer`).
- **Simulator Stack** — Rust · Bevy (3D, renders via wgpu) · bevy_egui · PyO3 embedding CPython. Cross-compiled for Windows in Docker.
- **Architecture**
  - `PropMaker_Feather/code.py` — short entry point; `main()` runs `boot_sequence` then launches the behaviours as tasks coordinated by `asyncio.Event`s.
  - `PropMaker_Feather/hardware.py` — board setup, channel map, LEDs, audio (`play_wav`), shared events. `config.py` — tunables from `settings.toml` via `os.getenv()`, defaulting to the tuned values.
  - `PropMaker_Feather/behaviours/` — one module per behaviour: `boot`, `breathing` (`ambient_breathing`), `motion` (`motion_sense`, `nose_boops`, `angry_animation`), `attention` (`idle_attention`), `leds` (`random_led_scheduler`).
  - `simulator/` — desktop app that runs the **unmodified** firmware against Python stand-ins for the hardware modules (`simulator/shims/`) and renders the droid in 3D.
- **PCA9685 channel map** — must match the physical wiring:

  | Channel | Part | Notes |
  |---|---|---|
  | 0 | Right wing servo | 544–2400 µs, lower angle = higher wing |
  | 1 | Left wing servo | mounted inverted; calibrated via `SERVO2_MIN/MAX` |
  | 2–4 | Main eye R, G, B | driven as 360° "servos", pulse range 0–6550 µs |
  | 5–7 | Small eye R, G, B | same as main eye |

  The white LED is `pwmio.PWMOut` on `board.EXTERNAL_NEOPIXELS`.
- **More detail** — `.github/copilot-instructions.md` and `README.md`.

---

## Core Principles

- **Simplicity First** — Make every change as simple as possible. Minimise code impact.
- **Security First** — All code must be secure. No exceptions.
- **No Laziness** — Find root causes. No temporary fixes. Senior developer standards only.
- **Minimal Impact** — Touch only what's necessary. Avoid introducing regressions.

---

## Model Strategy & Task Delegation

- **Architectural Planning:** Use `opus` capabilities to analyze system requirements, map out multi-file changes, and construct detailed step-by-step implementation specs.
- **Code Implementation:** Delegate file modifications, routine refactoring, and main feature implementation to `sonnet` agents.
- **Boilerplate & Testing:** Utilize `haiku` subagents for lightweight tasks such as generating standard unit tests, routine linting, or simple file reading.
- **Guardrails:** Do not allow `sonnet` or `haiku` execution steps to make macro-architectural decisions independently. If an implementation step breaks the core plan, escalate back to `opus` for plan revision.

---

## Branching & Git Workflow

- **`main`** — what is running on the robot. Never push to it directly.
- **`Documentation`** — the current integration branch. **All work branches are taken off `Documentation`** and PR'd back into it.
- Work branches are named descriptively: `feature/simulator`, `bugfix/eye-flicker`, `docs/wiring`.

---

## Environment Setup

### Host rule

**Never install any application or tool on the Windows host without asking first and getting an explicit yes.** The host runs finished binaries only; builds happen in Docker. If a tool is missing, say what is missing, what it is for and the alternatives, then let the user decide.

### Prerequisites

- Docker Desktop (running)
- A CIRCUITPY drive (the Prop-Maker Feather over USB) for hardware deploys

### Firmware deploy

Copy the contents of `PropMaker_Feather/` (`code.py`, `config.py`, `hardware.py`, `settings.toml`, `behaviours/`, `lib/`, `sounds/`) to the root of the CIRCUITPY drive. The board restarts and runs `code.py`. Debug via `print()` on the serial console.

Libraries are pre-compiled `.mpy` files copied by hand into `PropMaker_Feather/lib/`; `requirements.txt` is unused. `settings.toml` holds the tunables (ints and quoted strings only — CircuitPython 9's limit) and is committed, so it must never hold secrets.

### Simulator

```powershell
.\simulator\build.ps1          # builds the dev image if needed, cross-compiles, stages simulator\dist\
.\simulator\dist\ddl0-sim.exe  # runs PropMaker_Feather\code.py in the 3D simulator
```

Cargo's registry and target dirs live in Docker volumes, so nothing large lands in OneDrive. Python comes from the official embeddable zip staged into `dist\python\` — no host Python is needed.

---

## Code Style Conventions

- **Firmware (Python)** — follow the existing `code.py`: `snake_case` functions, `SCREAMING_SNAKE_CASE` constants, one `async` function per behaviour, coordinate with `asyncio.Event`. Use `await asyncio.sleep()` inside tasks — a blocking `time.sleep()` stalls every other behaviour. Stick to the CircuitPython subset of the standard library.
- **Simulator (Rust)** — `cargo fmt` and `cargo clippy` (run in the container) clean. Small systems/functions; no `unwrap()` on anything that can fail at runtime in normal use.
- **Shims (Python)** — mirror the real CircuitPython/Adafruit API surface that `code.py` uses, nothing more. Model hardware maths faithfully (e.g. angle → pulse → duty at 50 Hz) so the simulator shows what the robot will really do.
- **Comments** — Comment *why*, not *what*. Delete commented-out code before committing new work (existing commented code in `code.py` is the user's to remove).

---

## Testing

- **Firmware change** — run it in the simulator first (boot, breathing, nudge → boop, 3 nudges / hard shake → angry, trigger idle → attention), then verify on hardware.
- **Simulator change** — `simulator/build.ps1` succeeds; the app starts and the scenarios above behave as `code.py` describes; a syntax error in `code.py` shows a traceback in the log without crashing.

---

## Definition of Done

| Task Type | Done Criteria |
|-----------|--------------|
| **Firmware Behaviour** | Verified in the simulator, then on hardware; README sound/behaviour tables updated if affected |
| **Bug Fix** | Root cause identified, fix reproduced and verified in the simulator (and hardware where physical) |
| **Simulator Feature** | Builds in Docker, scenario checks pass, README updated |
| **Refactor** | Behaviour unchanged in the simulator |
| **Library / Dependency Upgrade** | Changelog reviewed, CircuitPython version compatibility checked, simulator still builds |

---

## Off-Limits Without Explicit Instruction

Never do the following without being explicitly asked:

- Install anything on the host.
- Modify `PropMaker_Feather/lib/` (`.mpy` libraries) or `boot_out.txt`.
- Change or delete anything in `.github/`.
- Change PCA9685 channel assignments, servo pulse ranges or the `SERVO2_*` / `BREATH_SERVO2_*` calibration values (in `settings.toml` and `config.py`) — they match the physical build.
- Upgrade the CircuitPython major version or major crate versions.
- Reformat files unrelated to the current task.
- Push directly to `main`.

If any of these seem necessary to complete a task, **stop and flag it** before proceeding.

---

## Communication & Ambiguity

- If a task is ambiguous, ask **one clarifying question** — the most important one — and wait. Do not ask multiple questions at once.
- If proceeding under an assumption, state it explicitly: *"I'm assuming X — let me know if that's wrong."*
- For large tasks, confirm the plan before writing any code.
- Keep status updates concise: what was done, what's next, any blockers. No padding.
- If a constraint in this file conflicts with a user instruction, flag the conflict and defer to the user.
- **Commands for the user to run** — whenever the user needs to run a command, also give it in its own fenced code block tagged with its shell (`powershell` on this Windows host, `bash` only for Git Bash). Put one command in each block, with no prompt (`PS>`, `$`) and no output inside, so it can be copied or run with one click. This applies even when the command is also mentioned in the prose.

---

## Workflow Orchestration

### Plan Mode

Enter plan mode for **any non-trivial task** (3+ steps, or involving architectural decisions).

- If something goes sideways, **stop and re-plan immediately** — don't keep pushing.
- Use plan mode for verification steps, not just building.
- Write detailed specs upfront to reduce ambiguity.

### Subagent Strategy

- Use subagents liberally to keep the main context window clean.
- Offload research, exploration, and parallel analysis to subagents.
- For complex problems, throw more compute at it via subagents.
- One task per subagent for focused execution.

### Self-Improvement Loop

After **any correction** from the user:

1. Update `tasks/lessons.md` with the pattern.
2. Write rules for yourself that prevent the same mistake.
3. Ruthlessly iterate on these lessons until the mistake rate drops.
4. Review lessons at the start of each session for relevance to the project.

### Verification Before Done

- Never mark a task complete without proving it works.
- Diff behaviour between `main` and your changes when relevant.
- Ask yourself: *"Would a staff engineer approve this?"*
- Build in Docker, run the simulator, check the log, demonstrate correctness.

### Demand Elegance (Balanced)

- For non-trivial changes, pause and ask: *"Is there a more elegant way?"*
- If a fix feels hacky, apply this: *"Knowing everything I know now, implement the elegant solution."*
- Skip this for simple, obvious fixes — don't over-engineer.
- Challenge your own work before presenting it.

### Autonomous Bug Fixing

- When given a bug report: **just fix it**. No hand-holding required.
- Point at logs, errors, and simulator output — then resolve them.
- Zero context switching required from the user.

---

## Task Management

| Step | Action |
|------|--------|
| **Plan First** | Write a plan to `tasks/todo.md` with checkable items |
| **Verify Plan** | Check in before starting implementation |
| **Track Progress** | Mark items complete as you go |
| **Explain Changes** | Provide a high-level summary at each step |
| **Document Results** | Add a review section to `tasks/todo.md` |
| **Capture Lessons** | Update `tasks/lessons.md` after corrections |

---

## Secure Coding Guidelines

- **Secrets** — Never commit secrets (Wi-Fi credentials in `settings.toml`, tokens). Keep them out of git.
- **Dependencies** — Pin crate and Docker base-image versions; check for vulnerable or outdated crates before upgrading.
- **Downloads** — Fetch build inputs (e.g. the Python embeddable zip) only from official sources, inside the container, and verify checksums.
- **PR Review** — Review changes for security issues before merging.
