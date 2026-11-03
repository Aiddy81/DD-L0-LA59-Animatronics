# Contributing and branching

Thanks for helping! Small fixes, such as a typo on a wiki page, are as welcome as new behaviours.

## Branches

| Branch | What it is |
|---|---|
| `main` | What's running on the robot. Never push to it directly. Every merge into it publishes a [release](CI-and-Releases#releases). |
| `dev` | Tested work on its way to `main`. |
| `Documentation` | The current integration branch. **Take every work branch from here** and open your pull request back into it. |

Flow: your branch → `Documentation` → `dev` → `main`.

Name work branches by what they do: `feature/simulator`, `bugfix/eye-flicker`, `docs/wiring`.

## Steps

1. Branch from `Documentation`:

   ```powershell
   git checkout Documentation
   ```

   ```powershell
   git pull
   ```

   ```powershell
   git checkout -b docs/my-change
   ```

2. Make your change. For firmware, try it in the simulator first (boot, breathing, nudge → boop, 3 nudges / hard shake → angry, trigger idle → attention).
3. Run the checks ([CI and releases](CI-and-Releases)).
4. Push and open a pull request into **`Documentation`**.

## Rules of thumb

- **Firmware:** `snake_case` functions, `SCREAMING_SNAKE_CASE` constants, one `async` function per behaviour, `await asyncio.sleep()` not `time.sleep()`, CircuitPython 9 only.
- **Simulator:** `cargo fmt` and `cargo clippy` clean; no `unwrap()` on anything that can fail in normal use.
- **Shims:** mirror only the API the firmware uses, and model the hardware maths faithfully.
- **Don't change without asking:** `PropMaker_Feather/lib/`, `boot_out.txt`, the PCA9685 channel map, servo pulse ranges, the `SERVO2_*` / `BREATH_SERVO2_*` calibration. They match the physical robot.
- **No secrets** in `settings.toml` or anywhere in git.

## Keeping the wiki up to date

The wiki lives in the `wiki/` folder and is reviewed in pull requests like code. On merge to `main`, the **Wiki** workflow publishes it.

**Update the wiki in the same pull request when you add or change:**

- a **behaviour**: [Behaviour table](Behaviour-Table), [Make her breathe, boop and more](Droid-Behaviours), [How the firmware works](How-the-Firmware-Works);
- a **setting**: [Settings reference](Settings-Reference);
- a **sound**: [Sound list](Sound-List);
- a **panel, button or key** in the simulator: the matching "Using the simulator" page, using the label exactly as it appears in the app, and its screenshot;
- a **part kind** or `simhw` call: [Adding new hardware](Adding-New-Hardware), [Adding a part kind](Adding-a-Part-Kind);
- the **wiring or channel map**: [Wiring](Wiring), [Channel map](Channel-Map).

Wiki tips:

- Page file names are the page names (`Settings-Reference.md` → `Settings-Reference`). Link with `[text](Page-Name)`. New pages go in `_Sidebar.md`.
- Images go in `wiki/images/` and every one needs alt text: `![What the picture shows](images/name.png)`.
- Keep the layout: *You'll need / Time*, numbered steps, ✅ *It should look like this*, ⚠️ *Something went wrong?*, then **More detail (for tinkerers)**.
- Take simulator screenshots with **📷 Screenshot** (they land in `dist\captures\`).

## More detail (for tinkerers)

- The first time, the GitHub wiki must have one page saved in the web UI before the workflow can push to it; after that it's automatic.
- Editing pages in the GitHub wiki's web editor works, but the next sync from `main` overwrites it. Edit `wiki/` instead.
