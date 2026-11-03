# Adding a part kind

> **Check first:** most new hardware needs no Rust. The four kinds (`led`, `pixels`, `servo_part`, `button`) plus a shim cover lights, strips, anything a servo turns and any input. See [Adding new hardware](Adding-New-Hardware).

Only an output none of them can show, such as a display, needs a simulator change.

## Option A: a new output (no new `parts.toml` kind)

1. **`simulator/src/hw.rs`**: add a field to `HwState` for the new output, and reset it in `reset_for_run`.
2. **`simulator/src/pyhost.rs`**: add a `#[pyfunction]` to the `simhw` module that writes the field, like `set_pca` or `set_part`. Check every argument (type, NaN, size) and log a `[sim]` warning rather than raising or panicking.
3. **`simulator/src/scene.rs`** (or `parts.rs` for something part-like): draw it. **`ui.rs`**: add a readout if useful.
4. **`simulator/tests/simhw.py`**: add the same function to the pure-Python `simhw`, so the headless test keeps passing.
5. Write the shim that calls it.

## Option B: a new `kind` in `parts.toml`

Do Option A's steps where needed, plus:

1. **`simulator/src/config.rs`**: add a variant to `enum PartKind`, its name in `name()`, parsing and validation of any new fields, and a unit test.
2. **`simulator/src/parts.rs`**: spawn and update it (see how `PartKind::Button` is handled), and add it to the kinds `shim_target` accepts for the right `simhw` calls.
3. **`simulator/src/part_edit.rs`**: add it to the Parts panel's kinds (the *Add …* buttons), its description, default name, and its settings.
4. **`simulator/src/ui.rs`**: its value in the PCA9685 panel and **Droid status**.

## Then

1. Build and test:

   ```powershell
   .\simulator\build.ps1 -Test
   ```

2. Keep `cargo fmt` and `cargo clippy` clean (they run in CI).
3. Check it in the app: add the part with the mouse and the keyboard, save, restart, drive it from a shim, undo and redo.
4. Update the [README's Parts file table](https://github.com/Aiddy81/DD-L0-LA59-Animatronics#parts-file), the [shim guide](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/blob/main/simulator/docs/custom-shims.md#8-when-a-shim-needs-rust-changes) and [Adding new hardware](Adding-New-Hardware).

See [Simulator architecture](Simulator-Architecture) for how the pieces fit.
