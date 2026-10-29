# Simulator: simhw part calls — stage 3 of #65

Branch: `feature/simulator-parts-stage3`, stacked on `feature/simulator-parts-stage2` (PR #89, on #87; neither merged). Stage 3 only: no drag and drop (stage 4). No new crates.

- [x] `hw.rs`: `part_kinds` (name → kind, left to shims), `None` until parts.rs publishes it for the run; `part_warned`; both reset per run. `PartValue`'s `allow(dead_code)` removed (`Rgb`/`Pixels` now come from simhw)
- [x] `parts.rs`: `reload_parts` publishes the kind table on every new run (unchanged and invalid files too); `shim_set_part` / `shim_set_pixels` / `shim_part_input` with one warning per run per name, capped at 128 names; 5 unit tests
- [x] `pyhost.rs`: `set_part`, `set_pixels`, `part_input` take `PyAny` and check types, NaN, sizes (iterables read no further than 1025 items); wait (GIL released, ≤2 s) for the run's kind table so code.py can't beat the first frame
- [x] `PartKind::name()` in `config.rs`, used by the PCA9685 panel too
- [x] `tests/simhw.py` mirrors the calls; `tests/shim_dir.py` (already in CI) runs the guide's `neopixel` and `touchio` shims straight from `custom-shims.md`, plus bad-input checks
- [x] `custom-shims.md` steps 1, 4, 8, Example B note, new Example D; README "Driving parts from a shim", custom shims, not-simulated and tests notes
- [x] Docker `CHECK=1` (fmt, clippy -D warnings), `--all-targets` clippy, 7 scenarios pass; 96 unit tests (Windows test exe run from `dist\`)
- [x] Running app (scratch repo with the guide's parts and shims): ring chases blue on Lola and Primitive, turns red while the touch pad is held (logged, no boop), unknown `D11` warns once per run, again once after R
- [ ] Commit, push, PR (stacked on #89)

## Review
- **Kinds live in the Bevy world, calls on the Python thread.** `reset_for_run` runs before the next frame reloads the parts, so a call at import time could see no parts. The table is `None` until published for this run and the calls wait for it, rather than keeping last run's table (stale after editing the file).
- `set_pixels` takes 0–255 (what `neopixel` holds, brightness applied by the shim); `set_part` brightness is 0–1 and an `led`'s `(r, g, b)` is 0–255 to match.
- A part with a `pca`/`pin` source refuses shim writes with a warning: `follow_sources` would overwrite them every frame.
- Worktrees have no `PropMaker_Feather/sounds/` (gitignored), so the real firmware stops at boot when run from a worktree's `dist\`; the Docker scenarios cover it.

# Simulator: parts file — stage 2 of #65

Branch: `feature/simulator-parts-stage2`, stacked on `feature/simulator-parts-stage1` (PR #87, not yet merged). Stage 2 only: no new `simhw.set_part` / `set_pixels` / `part_input` (stage 3) or drag and drop (stage 4). No new crates.

- [x] `simulator/parts.toml` (not `[[part]]` in sim.toml): parts fail one at a time instead of rejecting the whole wing/eye config; stage 4 can rewrite it without wiping sim.toml's comments; restart-on-save vs sim.toml's live apply stay separate. `parts.example.toml` committed; no `parts.toml`, so the default droid is unchanged
- [x] `config.rs`: `PartCfg` gains `pca`, `pin`, `anchor`, `full_duty`; `deny_unknown_fields` on `PartCfg`/`HingeCfg`; `PartCfg::check()` validates name, numbers, counts, sources, hinge
- [x] `parts.rs`: `parse_parts` (each entry on its own, `[sim]` warning per skipped one, duplicates, 64-part cap); reload when a run starts (`HwState::runs`); `follow_sources` (PCA duty/pulse, PWM/digital pin); anchors by node name, then the wing/glow components both models have; glTF parts wait for `SceneInstanceReady`
- [x] `hw.rs` pin table (`pins`, `button_pins`); `pyhost.rs` `set_pwm_pin(…, frequency)`, `set_pin` record every pin, new `pin_input`; `pwmio`/`digitalio` shims, scenario proxy and `tests/simhw.py` mirrored
- [x] `ui.rs`: Auto-reload also watches `parts.toml`; PCA9685 panel lists parts (name, value, kind, source); a spare channel shows its part's name
- [x] README "Parts file" section; `custom-shims.md` simhw table
- [x] Docker `CHECK=1` (fmt, clippy -D warnings) + `--all-targets` clippy; 7 scenarios pass; 91 unit tests (Windows test exe run from `dist\`)
- [x] Running app with the example file plus a bad entry, on Lola and Primitive: flap follows ch 0 (same angle as R wing), tail light follows the white LED pin, wing lamp rides the right wing lit by main R, ring on the band, button on D5 pressed/released with no boop; bad entry logged and skipped; saving the file restarts and applies; with Auto-reload off, Restart applies
- [x] `digitalio` button read checked in Docker (pull-up/pull-down, no button = unchanged)
- [ ] Commit, push, PR (stacked on #87)

## Review
- **Value column was cut off** at the default panel width (long pin names like `EXTERNAL_NEOPIXELS`); columns are now name · value · kind · source.
- **Pin buttons are written every frame**, not only on press, so a released button on a pull-up input reads `True` from the start of a run and after a restart.
- Parts anchored to Lola-only nodes (`head`, …) are skipped on the primitive model with a warning, by design; the five shared names work on both.
- `ddl0-sim` isn't listed for the computer-use tool, so captures used PrintWindow with `SetCursorPos` + a relative `mouse_event` nudge for clicks.

# Simulator: generic part types — stage 1 of #65

Branch: `feature/simulator-parts-stage1` off `Documentation`. Stage 1 only: no parts file, pin/PCA sources, `simhw` calls or panel listing (stages 2–4). No new crates.

- [x] `config.rs`: `PartCfg` (flat: kind-specific fields are ignored by other kinds), `PartKind`, `PixelLayout`, `PartShape`, `HingeCfg`; `WingCfg::elevation_deg` delegates to `HingeCfg`
- [x] `hw.rs`: `HwState::parts` (name → `PartValue`), cleared by `reset_for_run`
- [x] `scene.rs` / `droid.rs`: shared `ServoTrack`, `servo_frame`, `glow_emissive`, `shine`, `lamp_material`, `lamp_light` (no behaviour change)
- [x] `parts.rs` (new): `Parts` resource, spawn under `DroidRoot`, LED/pixels glow, servo part slew + hinge, button press/release with no boop
- [x] Docker `CHECK=1` build (fmt, clippy -D warnings), `--all-targets` clippy, 7 scenarios pass, 83 unit tests (Windows test exe against `dist\`)
- [x] No parts: app unchanged on Lola and Primitive (captures)
- [x] Local demo (removed before commit): one part of each kind on both models; LED cycles colour, ring chases, flap swings (clamping is covered by unit tests), button press/release logged with no boop, head click still boops, orbit from the button works, parts rebuilt on model switch
- [x] Commit, push, PR into `Documentation` (#87)

## Review
- **Button stuck down (found in the demo).** The cap sinks while pressed, so the pointer is no longer over it when the mouse button comes up and `Pointer<Released>` goes to the shell instead. The release is now a system (`release_held_button`) that lets go of the held button once the left mouse button is up, wherever the pointer is.
- **Synthetic drags don't orbit with `SetCursorPos`**: the camera reads raw mouse motion, so capture scripts need relative `mouse_event` moves.
- `PartCfg` is a flat struct (kind-specific fields ignored by other kinds) rather than a tagged enum, so each `[[part]]` in stage 2's file only lists what it needs and serde defaults stay simple.
- CI still doesn't run `cargo test` (Windows cross target); the tests were run as a Windows exe from `dist`.

# Simulator: UI refresh part 5 — 3D viewport overlays, mini-toolbar, transitions (#80)

Branch: `feature/sim-ui-viewport`, stacked on `feature/sim-ui-palette` (PR #85, not yet merged). PR 5 of 5. No new crates. Room light moves into the mini-toolbar too (user's choice).

- [x] Step a — `overlay.rs` (new): wing angles, eye RGB, white LED duty, accel arrow; egui painter on the 3D view after `orbit_camera`; values from the same helpers as the PCA panel and Plot; palette tokens only; `Cmd::Overlay`, prefs `overlay_*`
- [x] Step b — mini-toolbar in the Viewport tab (`dock.rs`): Front/Side/Top/Eyes/Reset, Light, Screenshot, Record, Overlays; through `tip`/`Cmd`; Camera and Capture groups leave the main toolbar (3 groups left)
- [x] Step c — transitions: camera tween in `orbit_camera` (real clock, shortest yaw, a drag/scroll takes over), panel open/close fade (`dock::Fades`), `prefs.animations` (palette + View menu, ✔ in palette), `animation_time = 0` when off
- [x] Unit tests: overlay rows, projection, tween, fades, prefs round-trip (73 pass, Windows test exe run against `dist\`)
- [x] README: 3D view toolbar, overlays, camera glide, animation setting; toolbar groups and palette list updated
- [x] Docker build (locked), `CHECK=1`, `--all-targets` clippy; 7 scenarios pass
- [x] Captures (SendInput + PrintWindow): overlay values equal the PCA9685 panel on the same frame; Dark/Light/High contrast readable; bar by mouse, by F6 → Tab → Enter, and from the palette (✔ rows); toolbar clicks never boop; camera mid-glide at 120 ms with animations on, already there with them off; panel tick clears and fades; overlays and animations-off survive a restart; Screenshot toast; Light menu
- [ ] Commit, push, PR (stacked on #85) with #80's Done-when checklist
- [ ] With a real mouse: watch a panel fade in and out (too quick to catch reliably with window captures), Record start/stop from the bar — user

## Review
- **Pointers were 52 px low (user spotted it).** Bevy 0.16's `Camera::world_to_viewport` already adds the viewport's offset in the window, and the overlay added the dock tab's offset again. Checked in `bevy_render-0.16.1` and fixed `to_egui`. The check was the arrow's start: the camera looks at that point, so it must sit at the view's centre.
- **Wing pointers aimed at the hinge**, which on Lola sits at the wing's edge, plus a guessed offset. Anchors are now the size-weighted middle of the part's mesh bounding boxes (`part_centre`), so they land on the shell and follow it as it moves.
- **Accel arrow showed the held reading**, which a Nudge never changes. It now uses `last_accel`, the value the firmware read and the Plot shows.
- **Labels overlapped in Top view** (both eyes line up); a label now moves down past any it would cover. The accel label had no plate and was unreadable on the white head.
- **Mini-toolbar wrapped early:** an `egui::Area` starts from last frame's size, so `horizontal_wrapped` wrapped at its own old width. It now wraps at the view's width.
- "▾" isn't in egui's bundled fonts (drawn as a box); the menus are plain *Light* / *Overlays*, like *Options*.
- Clicking a dock tab's title doesn't give it keyboard focus; F6 does (PR 4's design), then Tab reaches the bar.

# Simulator: UI refresh part 4 — command palette and keyboard navigation (#80)

Branch: `feature/sim-ui-palette`, stacked on `feature/sim-ui-docking` (PR #84, not yet merged). PR 4 of 5; no overlays, mini-toolbar or transitions. No new crates.

- [x] Step 1 — `ui.rs`: `Cmd` covers every toolbar, View-menu and detached-bar action; keyless rows in `SHORTCUTS` (labels for panes/themes/lights/speeds/layouts from their enums); Shortcuts window lists only keyed rows; one dispatcher (`CmdCx::run`) — toolbar/menu clicks queue a `Cmd`; shared `cmd_enabled`
- [x] Step 2 — `palette.rs` (new): Ctrl+K palette per egui context (works in detached windows), filter, Up/Down/Enter/Esc, disabled rows greyed, saved layouts and layout files listed at run time
- [x] Step 3 — keyboard navigation: Tab/arrows (egui built-in, checked in 0.31.1 source), F6 / Shift+F6 between dock tabs with keyboard focus on the tab button; `theme::focus_ring` from palette tokens, keyboard-only (`:focus-visible` style); egui_dock `*_with_kb_focus` tab styles
- [x] Unit tests: no duplicate binds; every pane/theme/light/speed/built-in layout has a row with a label; filter ranking; `step_tab` wraps through every tab (65 tests pass, Windows test exe run against `dist\`)
- [x] README: command palette and keyboard navigation
- [x] Docker build (locked), `CHECK=1` (fmt, clippy -D warnings), `--all-targets` clippy; 7 scenarios pass
- [x] Captures (SendInput + PrintWindow): palette runs Screenshot (toast), Layout: Debug, Restart, Theme ×4, detach; typing in palette or log search fires no shortcuts; Space on a focused button pauses once; Tab/arrows/F6 with the ring in Light, Dark, High contrast, Follow system; palette in a detached Log window (Larger UI → 110%, Dock back)
- [ ] Commit, push, PR (stacked on #84; #82–#84 still open)
- [ ] Interactive with a real keyboard: Nudge/Shake from the palette with code.py running (this worktree has no `sounds/`, so code.py stopped at boot here; scenarios cover the firmware) — user

## Review
- **Shortcuts died after one Tab (pre-existing, exposed by keyboard nav).** `read_cmds` skipped keys while `ctx.wants_keyboard_input()`, which egui 0.31 defines as *any* widget focused, not just text fields. With mouse use nothing kept focus, so it never showed. Now `text_field_focused` checks for a `TextEditState` on the focused id.
- **Palette list showed two rows.** Inside the modal a `ScrollArea` gets almost no room and falls back to its 64 px `min_scrolled_height`; set it to the list's max.
- **Shift+F6 before F6 in the table:** egui's key match ignores extra modifiers, so a plain-F6 bind would also take Shift+F6.
- Typing "detach" didn't find the row labelled "Move the focused panel to its own window"; labels now use the word people type (Detach…, Dock back…).
- `FindWindow` is ANSI by default, so a detached window's em-dash title can't be found by name; enumerate the process's windows instead.

# Simulator: UI refresh part 3 — dockable, detachable panels and layouts (#80)

Branch: `feature/sim-ui-docking`, stacked on `feature/sim-ui-toolbar` (PR #83, not yet merged). PR 3 of 5; no command palette, keyboard navigation, overlays or transitions. Crate: `egui_dock =0.16.0` (egui 0.31 release; confirmed with the user over `egui_tiles =0.12.0` because it has floating windows, close buttons and tab menus built in).

- [x] Step 1 — `Cargo.toml`: `egui_dock =0.16.0` (serde; the dock state round-trips through TOML, so no `serde_json`); `dock.rs` (new): `Pane`, `PaneCx`, `draw_pane`; Motion/PCA/Log bodies extracted from `ui.rs`; dock replaces the side and log panels; Viewport pane sets the camera viewport and gates orbit/boop
- [x] Step 2 — floating, tab context menu (Float / Detach / Close), View menu panel ticks, focus mode (F11), dock style from palette tokens
- [x] Step 3 — detached OS windows (Bevy secondary `Window` + own egui context), close/Dock back re-docks, theme and UI scale per window
- [x] Step 4 — `layout.rs` (new): layout in prefs (dock, floating rects, detached windows, main window position), named layouts (Default/Debug/Demo + user), Reset, export/import (`dist\layouts\`), validation, off-screen recovery
- [x] README: docking, View menu, focus mode, layouts, export/import, per-window UI scale
- [x] Docker build (locked); `cargo fmt --check`, `clippy -D warnings` (also `--tests`) clean; 7 scenarios + headless checks pass
- [x] Captures: dock/float/detach/re-dock/hide/show every pane; focus mode; restart restores layout, floating and detached windows, scale; layouts switch; export/import round-trip; Reset; off-screen pulled back; Light/Dark/High contrast readable; boop only through the viewport
- [ ] Commit, push, PR (stacked on #83)
- [ ] Interactive with a real mouse: drag tabs to every edge, move a floating panel and a detached window between monitors, Ctrl+= / - / 0 in each window, the theme picker — user

- [x] Follow-up from the user: Behaviours and Scenarios as their own panels (out of the toolbar); plot tickboxes as a 3×2 / 2×3 grid; drag a tab out past the window edge to detach it (floating panels were stuck at the edge)

## Review
- **Tear-off:** egui_dock hands `on_tab_button` a stand-in widget while a tab is dragged, so the drag is found by rebuilding egui_dock 0.16.0's tab-button ids (pinned with `=`). egui keeps the drag alive while the pointer is outside the window; letting go there detaches the tab under the pointer (`GetCursorPos`), slid onto that monitor allowing for the title bar. Docking back never adds a tab over the 3D view.
- **UI scale never stuck (a PR 1 bug).** bevy_egui calls `set_pixels_per_point` every frame from `EguiContextSettings.scale_factor`, which overwrote `ctx.set_zoom_factor`. The scale now lives in that per-window setting, which is also what makes per-window scale possible.
- **Two vsync'd windows cut the frame rate to ~19 FPS.** Detached windows use `AutoNoVsync`; the main window's vsync still paces the app (~60 FPS with a detached window open).
- **The 3D view's hover never registered** inside an egui_dock tab (the dock's tab-body widget wins egui's hit test), so orbit and click-to-boop were dead from step 1 until the end. Hover is now pointer-in-rect plus top layer, and an orbit drag belongs to the view only if it started there, so dragging a tab across it doesn't spin the camera.
- **egui_dock 0.16 never fills `WindowState.screen_rect`**, so floating panels' places weren't in the serialised dock. They're read from egui's area memory and saved separately (`floats`).
- Saved detached windows open on the first `Update`, not at `Startup`; a startup-spawned window logged bevy_egui's "Failed to retrieve egui node data".
- A window capture (PrintWindow) of a window behind another can return a stale frame, which looked like the main UI had vanished. Re-capture before believing a blank window.
- The Import list was a third menu level and was squeezed to a sliver at the screen edge (the user caught it); the files are listed inline now.
- Once, with synthetic input, Log and Plot ended up split and the camera top-down, which fits Bevy missing a mouse release while focus moved between windows. Not reproduced; worth watching with a real mouse.


# Simulator: UI refresh part 2 — grouped toolbar, status bar, toasts (#80)

Branch: `feature/sim-ui-toolbar`, stacked on `feature/sim-ui-theme` (PR #82, not yet merged). PR 2 of 5; no docking, command palette or overlays.

- [x] `ui.rs`: toolbar as five groups (Run, Speed, Options, Camera, Capture) + ⚙ View menu (theme, UI scale) and Shortcuts; groups fold into menus (Options, Capture, Camera, Speed) when narrow, then into one More menu; icon glyphs from egui's bundled fonts; every control keeps its `tip` text and `SHORTCUTS` key
- [x] `ui.rs` / `main.rs`: status bar (run state, sim time, speed, behaviour, heap, FPS) from the same sources as the panels; shared `heap_colour`; `FrameTimeDiagnosticsPlugin`
- [x] `toast.rs` (new), `capture.rs`, `hw.rs`, `ui.rs`: toasts for screenshot saved, firmware/shims reloaded, traceback (click shows it in the log); palette tokens only
- [x] README: toolbar groups, status bar, notifications; renamed buttons
- [x] Docker build (locked); `cargo fmt --check`, `clippy -D warnings` (also `--tests`) clean; 7 scenarios + headless checks pass
- [x] Captures (window capture + posted input): toolbar at 1800/1300/1000/820 px; More > Capture > Screenshot works; status bar vs panels; all three toasts in Light, Dark, High contrast; traceback click switches to Log, scrolls and highlights
- [x] Commit, push, PR (stacked on #82)
- [ ] Interactive with a real mouse: hover tooltips in folded menus, theme picker inside the ⚙ View menu, a real traceback scrolled far up the log — user

## Review
- Run alone is ~550 px, so at ~800 px even four folded menus didn't fit and the row pushed View/Shortcuts off-screen (and stopped the Behaviours row wrapping). Groups that still don't fit now go into one More menu, and the right-hand buttons drop their text. Layout decisions use widths measured in the full form, so dropping text can't make it flip back and forth.
- The status bar was first declared after the side panel, so it only spanned the left part and FPS was cut off on narrow windows. It is declared before the side panel now.
- `●` isn't in egui's bundled fonts (rendered as a box); the toast marker is painted instead. The State panel's `→` chain separator has the same problem (pre-existing, not changed here).
- Current behaviour skips the LED scheduler's chain, which otherwise ties with the wing behaviours.
- Saved screenshots are the 3D view only, so an open menu doesn't spoil them.
- Unit tests in `hw.rs`/`ui.rs` compile under `clippy --tests` but can't run: the crate's tests need the Windows target (and a Linux build stops at `alsa-sys`).
- A window size saved far off-screen (65k px tall) crashes the app at start; found by accident while testing, out of scope here (PR 3 handles off-screen windows).


# Simulator: UI refresh part 1 — style, themes, UI scale, tooltips (#80)

Branch: `feature/sim-ui-theme` (off `Documentation`). PR 1 of 5 from the issue; toolbar/status bar/toasts, docking, palette and overlays follow in later PRs.

- [x] `theme.rs` (new): `ThemeChoice`, `Palette` tokens (Okabe-Ito), Light/Dark/High contrast, `apply` (style, type scale, focus ring), `ActiveTheme`, `ThemePlugin` (system theme via registry + `WindowThemeChanged`, zoom)
- [x] `prefs.rs` / `main.rs`: `theme`, `ui_scale` saved; plugin wired in
- [x] `ui.rs`: all `Color32` literals → palette tokens; Theme combo + UI scale on toolbar; Ctrl+= / - / 0 in `SHORTCUTS`; shared section header; tooltips with shortcut keys from `SHORTCUTS`
- [x] `scene.rs`: clear colour and floor follow theme (× room light)
- [x] README: themes & UI scale, remembered prefs, tooltips
- [x] Docker build (locked); `cargo fmt --check`, `clippy -D warnings` clean; headless scenarios pass
- [x] Captured Light, Dark and High contrast (Dim, plus High contrast in Bright and Dark rooms): panels, log, plot and 3D readable; an old prefs file without the new keys loads
- [ ] Interactive: follow-system flipping live, scale slider and Ctrl+= / - / 0, theme and scale restored after restart, tooltips — user

## Review
- egui draws a selected button's text in `selection.stroke`'s colour. The first draft set that to the accent, so in Dark and High contrast the selected Light/Model/Log buttons were accent-on-accent and unreadable. It's the text colour now; the focus ring comes from `widgets.active`.
- High contrast's floor at `#202020` showed bright white speckle in the Dim room only (not Bright, not Dark, not the Dark theme at `#2E3036`). Using `#2E3036` there too removed it. Not traced further into Bevy's lighting.
- The UI scale reset button said "100%" right next to the slider's own "100%" value; it's "Reset scale" now.
- The slider applies the new scale on release; zooming mid-drag moved the slider out from under the cursor.
- Toolbar rows wrap (`horizontal_wrapped`), since the larger type would push controls off a narrow window. PR 2 rebuilds the toolbar properly.
- Plot x/y/z accel lines have their own colours instead of z-bright, x/y-dim; README updated to match.


# Simulator: shortcuts window (#77), audio panel (#58), deploy to CIRCUITPY (#61)

Branch: `feature/sim-audio-deploy-shortcuts` (off `Documentation`). #14 left alone (user's choice).

- [x] #77 `ui.rs`: one `SHORTCUTS` table drives the key handling and a Shortcuts window (button, `?`/F1, Esc); ignored while a text field has focus
- [x] #58 `wav.rs` (new, std-only) + `audio.rs`: `NowPlaying` + level envelope, `AudioMix` volume/mute (saved in prefs), `wav_info`, `scan_sounds` (missing `play_wav` files, unplayable formats)
- [x] #58 `shims/audiomixer.py` + `audiocore.py` + `pyhost.rs` + `tests/simhw.py`: report mixer format; `ValueError` on a mismatched sample, like CircuitPython 9.2
- [x] #58 `ui.rs`: Audio panel (now playing, meter, Stop, mute, volume, sound list with Play, problems)
- [x] #61 `deploy.rs`: find `CIRCUITPY` by volume label (`windows-sys` 0.61.2, already in the tree via winit), plan only changed files, never `lib/`/`boot_out.txt`, `code.py` last, background copy
- [x] #61 `ui.rs`: Deploy button + confirmation window; results in the sim log
- [x] README: shortcuts bullet, audio panel, deploy button, setup step, unit-test note
- [x] Docker build (locked); `cargo fmt --check`, `clippy -D warnings` clean; unit tests (wav 10, deploy 5, settings 10) + headless checks + 7 scenarios pass
- [x] Interactive (posted keys + window capture): F1 opens Shortcuts, Esc closes, N still boops; boot normal; Deploy and Shortcuts buttons present
- [ ] Interactive: `?` with a real keyboard, `?` typed in log search doesn't open it, Audio panel (meter, mute, volume, Play, a renamed WAV flagged) — user
- [ ] Deploy to the real board, confirm the copy, second deploy says up to date — user

## Review
- `?` arrives from bevy_egui as Shift+Slash (keys are mapped by physical position), never as `Key::Questionmark`, so the table binds Shift+Slash, plus Questionmark and F1.
- The mixer's `ValueError` text was checked against CircuitPython 9.2's `audiosample_must_match` (the board runs 9.2.8): `The sample's sample_rate does not match`, etc. The first draft used older wording.
- The headless stand-in WAVs were mono, which the mixer now rejects as the board would; they're stereo now.
- Deploy only blames log mode for a write-protected drive (Windows error 19). A plain "access denied" keeps the OS wording, since it can mean other things.
- Deploy never deletes files that are only on the board, and `boot.py`/`log_off.py` are deployed too (the issue's list left them out; log mode needs `boot.py`).
- One commit for all three issues: they share `ui.rs`, `prefs.rs` and the README, so splitting by hunk wasn't worth the risk.
- Computer use still can't find the sim window; checks were done by posting keys to it and capturing it with PrintWindow.

# Simulator: event markers (#49) + servo speed limit (#51)

Branch: `feature/sim-event-markers-servo-speed` (off `Documentation`).

- [x] `hw.rs`: `MarkerKind`, pending `markers`, `mark()`, `trigger_idle()`, `servo_speed`
- [x] `pyhost.rs`: mark WAV starts and restarts/scenarios; `trigger_idle()`
- [x] `ui.rs`: mark nudges/shakes/boops; stamp markers in `record_history`; draw VLines + labels; Events checkbox (saved via `plot_hidden`)
- [x] `config.rs` / `sim.toml`: `[servo] sec_per_60deg = 0.12` (FS90 at 4.8 V)
- [x] `scene.rs`: `slew()` + tests, `ServoSlew`, `slew_servos` (virtual time), `drive_wings` uses slewed angle; sustained-lag warning
- [x] `ui.rs` / `prefs.rs` / `main.rs`: "Servo speed limit" toolbar checkbox, remembered
- [x] README: FS90 in hardware list, plot markers, servo speed limit
- [x] Docker build (locked); `cargo fmt --check`, `clippy --all-targets -D warnings` clean; slew tests pass natively (5); headless checks pass (7 scenarios)
- [ ] Interactive: nudge/idle markers, Events toggle, wings ease, pause mid-move, warning at slow `sec_per_60deg` — user

## Review
- Whatever causes a marker queues it in `HwState` (boop/nudge/shake, `play_wav`, `trigger_idle`, `start_with`). `record_history` then timestamps it on the plot's own clock, so markers line up with the samples. While the clock is paused they wait, which is also when the firmware gets to act on them.
- The servo slew runs in virtual time and is always tracked. The toolbar toggle only picks what the 3D wings show, so the lag warning works either way. The plot and the PCA9685 panel still show the commanded angle.
- `LAG_SUSTAIN_S` is 0.4 s, not the planned 0.3 s. A full 180° snap takes 0.36 s at 0.12 s/60°, and a single snap must not warn.
- The slew logic is in plain functions (`slew`, `ServoTrack::follow`), so its tests run natively. The Windows-target tests only compile in the image, since it has no Wine.
- Computer use still can't find the sim window. A PrintWindow capture confirmed both checkboxes and a clean boot with no lag warnings. The nudge/idle markers and the warning at a slow `sec_per_60deg` are left for the user.

# Simulator: hot-reload sim.toml (#47) + more capable plot (#48)

Branch: `feature/sim-toml-reload-plot` (off `Documentation`).

- [x] `config.rs`: `SimConfig::read` (Ok(None) missing / Err on parse); `load` built on it
- [x] `ui.rs` `hot_reload`: watch sim.toml mtime, replace `SimConfig` + `hw().board`, log reload or warning (keep old config)
- [x] `main.rs`: pass start-up sim.toml mtime; `sim.toml` header comment
- [x] `egui_plot =0.32.1` (last release on egui 0.31); one egui in `cargo tree`
- [x] `ui.rs` plot: 5 s–5 min window, Freeze, zoom + Reset, hover readout, per-series checkboxes, small eye R/G/B, min/max decimation
- [x] `prefs.rs`: plot window + series visibility remembered
- [x] README updated
- [x] Docker build succeeds (locked); `cargo fmt --check` and `clippy -D warnings` clean; headless checks pass (7 scenarios)
- [ ] Interactive: sim.toml edits live / broken TOML keeps old; plot controls; scenarios — user

## Review
- Everything already read `Res<SimConfig>` and `hw().board` live, so a reload is just replacing them. It isn't gated on Auto-reload, because nothing restarts.
- The plot x axis is run time (s), not "seconds ago", so a zoomed or frozen view doesn't scroll. Following = setting the bounds each frame; any user change to the bounds turns following off.
- Three plots share one x axis through `link_axis`; the mouse wheel is left to the panel (`allow_scroll(false)`), and Ctrl+scroll / right-drag zoom.
- Adding `egui_plot` made cargo re-pick `windows-sys` 0.61 for one existing crate; I restored 0.52 by hand so the lock only adds `egui_plot` (`--locked` passes).
- Couldn't drive the GUI from here (computer use can't find the sim window), so the interactive checks are left for the user.

# Simulator: boop strength (#44) + accelerometer plot (#45)

Branch: `feature/sim-boop-strength-accel-plot` (off `Documentation`).

- [x] `scene.rs`: boop strength from hold time (0.1 s → 0.3 g, 1 s → 2 g), direction from the hit normal, `[sim] boop … g from …` log
- [x] `hw.rs` / `pyhost.rs` / shim: `last_accel` (clipped as read) and `simhw.accel_range`
- [x] `settings.rs`: `saved_value(key)`
- [x] `ui.rs`: accel band in the plot with nudge / animation / hard / clip lines
- [x] `tests/simhw.py`, `docs/custom-shims.md`, README updated
- [x] Docker build succeeds; `cargo fmt --check` and `clippy -D warnings` clean; headless checks pass (7 scenarios)
- [ ] Interactive: tap / hold / side click / clip test / scenarios — user

## Review
- Strength comes from `Pointer<Click>::duration` and direction from the mesh-picking hit normal, so no new input tracking was needed; the orbit-drag guard still applies.
- The plot records what the firmware actually read (`next_accel`, clipped like the shim), not the held value, so a 2 g range visibly flattens a shake below the hard line.
- Thresholds come from settings.toml / config.py defaults via the Settings state, so editing one moves its line.
- `build.ps1` doesn't run fmt/clippy; ran `docker/build.sh` with `CHECK=1` for those.

# Simulator: scenario checks in the Docker build (#60)

Branch: `feature/headless-build-checks` (off `Documentation`).

- [x] `Dockerfile`: `python3` listed explicitly (was only there through the base image)
- [x] `build.sh`: runs `tests/headless.py` after staging `dist/`; failure exits 3
- [x] `build.ps1`: exit 3 → "Scenario checks failed", says dist\ is still built
- [x] README (Tests and CI) updated
- [x] `build.ps1 -RebuildImage` succeeds with all 26 checks and 7 scenarios PASS; `cargo fmt --check` and `clippy -D warnings` clean
- [x] Broken scenario (`boop.toml` expecting a sound that never plays) fails the build with `FAIL scenario boop`, exe still staged; reverted
- [x] No `__pycache__` left in the checkout

## Review
- Hooked into `docker/build.sh`, not `build.ps1`, so CI's build job runs the same checks with no `.github/` change.
- Checks run after staging `dist/`: broken firmware fails the build but still leaves a simulator to debug it in.
- Exit code 3 is reserved for "checks failed", so `build.ps1` can tell it apart from a compile error.
- Adds ~13 s to a build; Debian's Python 3.11 is enough (`tomllib`), so no new download.

# Simulator live State panel (#39)

Branch: `feature/simulator-state-panel` (off `Documentation`).

- [x] `sim_runtime.py`: `_State` snapshot at 10 Hz real time: hardware.py's `asyncio.Event`s, task await chains, `motion_sense`'s nudge count / angry window and the idle `_wait_for` deadline, read from coroutine frames; cleared when the run ends
- [x] `hw.rs` / `pyhost.rs`: `FwState` + `simhw.set_state`
- [x] `ui.rs` / `prefs.rs`: State section (event badges, two countdown bars, task list), open state saved
- [x] `tests/simhw.py` + `headless.py`: 8 new state checks
- [x] Every int/float local in each task's firmware frames shown under the task (`func.name` for called functions), so new counters need no simulator change
- [x] Docker build succeeds; `cargo fmt --check` and `clippy -D warnings` clean in the container
- [x] `headless.py` passes: all 25 checks and 7 scenarios (firmware unchanged)
- [x] README updated
- [ ] Interactive: events flip during a boop, count 1→2→3 then angry resets it, idle countdown restarts on nudge, Trigger idle → "not waiting" + attention/trench task — user

## Review
- The count, window and idle wait are locals in the firmware's coroutines, so they're read from `cr_frame.f_locals`; the idle deadline comes from the simulator's own `_wait_for` frame, so it's exact. Firmware untouched.
- Deadlines are sent in virtual time and the UI counts down from the clock, so bars move smoothly between snapshots even at high speed.
- Events are discovered from `hardware.py`, not listed; a renamed firmware local only blanks its row.
- Tasks asyncio starts for itself (wait_for's inner `Event.wait`) are hidden; chains stop at the first non-firmware coroutine.

# Simulator Settings panel for settings.toml (#46)

Branch: `feature/simulator-settings-editor` (off `Documentation`).

- [x] `settings.rs` (std only): discover `_int("KEY", default)` in config.py with help from the comment above; read settings.toml like `os.getenv`; line-based edits that keep comments, order, line endings and other keys; write via tmp + rename
- [x] Calibration keys (`SERVO2_*`, `BREATH_SERVO2_*`) read-only, filtered out again at save
- [x] `ui.rs`: Settings section (value, default, Reset, Save, Revert, pending/changed/invalid colours); re-read on disk change in `hot_reload`; open state in prefs
- [x] 10 unit tests pass (`rustc --test src/settings.rs` in the dev image), incl. no-op edit leaves the real settings.toml byte-identical
- [x] Docker build succeeds; `cargo fmt --check` and `clippy -D warnings` clean in the container
- [x] `headless.py` passes: all checks and 7 scenarios (firmware unchanged)
- [x] README updated
- [ ] Interactive: edit `BREATH_PERIOD_S`, Save → restart with faster breathing, `git diff` shows one value; Reset; locked rows — user

## Review
- Logic lives in `settings.rs` with no egui/bevy, so its tests run with plain `rustc` (the crate itself only cross-compiles for Windows).
- Keys come from config.py, not a list in Rust, so a new tunable appears with no simulator change.
- Reset writes the default value rather than deleting the line, so the file's comments for that key stay.
- Only discovered keys are read or shown, and only integers are written, so nothing else in settings.toml (e.g. secrets someone adds) is displayed or touched.

# Simulator 3-axis accelerometer input (#43)

Branch: `feature/simulator-3-axis-accel` (off `Documentation`).

- [x] `hw.rs`: `nudge_dir`, kept across restarts with `accel_g`
- [x] `ui.rs`: nudge/shake are held + direction x delta (0.3 / 2.0 g); Direction picker, x/y/z held sliders, Rest button
- [x] `scene.rs` / `pyhost.rs`: head click and scenario steps pass a direction
- [x] `sim_runtime.py`: `axis` key for nudge/shake steps; log line names a non-default axis
- [x] `tests/simhw.py`: relative nudge/shake with a direction
- [x] `scenarios/sideways-nudge.toml`: x/y nudges are ignored, z nudge boops, no angry
- [x] README updated
- [x] Docker build succeeds; `cargo fmt --check` and `clippy -D warnings` clean in the container
- [x] `headless.py` passes: all 18 checks and 7 scenarios, including `sideways-nudge`
- [ ] Interactive: Direction picker, held sliders and Rest in the app — user

## Review
- The issue assumed the firmware senses nudges from any direction; since PR #35 it reads only z. Firmware left alone: the simulator now shows ±X/±Y nudges being ignored, as on the robot.
- Nudge and shake are relative to the held reading, so they work from a sideways or tipped pose. From rest on +Z they give 1.3 g and 3 g as before, so existing scenarios and tests are unchanged.
- A −Z shake from rest reads −1 g and only counts as a nudge, because hard motion is `abs(z) > 2.5`. Documented, not changed.
- In `sideways-nudge` the tilt itself is a z change, so it boops once before the sideways nudges.

# Simulator scripted scenarios (#40)

Branch: `feature/simulator-scenarios` (off `Documentation`).

- [x] `sim_runtime.py`: `_Scenario` engine (TOML steps + expectations), simhw proxy that records audio/PCA/white LED, polled each loop pass
- [x] `hw.rs` / `pyhost.rs` / `simhw.py`: `nudge`, `shake`, `set_accel`, `trigger_idle`, `set_scenario`
- [x] `ui.rs`: Scenarios row (dropdown, Run scenario, PASS/FAIL badge)
- [x] `simulator/scenarios/`: boot, breathing, boop, angry-nudges, angry-shake, idle
- [x] `headless.py` runs every scenario after the existing checks; `--scenario NAME`. All 18 checks + 6 scenarios pass
- [x] Negative checks: a missed `by` FAILs (exit 1), an unknown action is an ERROR, a SyntaxError in a firmware module FAILs the scenario
- [x] Docker build succeeds; `cargo fmt --check` and `clippy -D warnings` clean in the container
- [x] README updated
- [ ] Interactive: each scenario from the dropdown passes in the app, badge + hover, Restart clears it — user

## Review
- One engine, in `sim_runtime.py`, shared by the app and headless, so they can't drift. Rust only passes the file in and shows the verdict.
- The issue's one-line `[[step]] t = …` isn't valid TOML; one key per line instead.
- Added `start = "<log line>"`: at 40x, Python's imports eat ~7 virtual s before boot, so absolute times would vary by machine.
- A gentle nudge is ignored while a boop animates, so `angry-nudges` spaces its nudges 5 s apart.
- `breathing` is its own scenario (timed from boot complete) so the boot sequence's own wing moves can't satisfy it.
# Simulator behaviour buttons (#38)

Branch: `feature/simulator-behaviour-buttons` (off `Documentation`).

- [x] `sim_runtime.py`: discover no-argument `async def`s in `behaviours.*`, minus `code.py`'s `create_task` targets (parsed with `ast`) and `_` names; run one on request with `pause_breathing` set
- [x] `hw.rs` / `pyhost.rs`: list, request and running state; `simhw.set_behaviours`, `take_behaviour_request`, `set_behaviour_running`
- [x] `ui.rs`: Behaviours toolbar row, greyed out while one runs
- [x] Headless test: 3 new checks (buttons found, background loops excluded, boot_sequence button re-runs boot); all 18 pass
- [x] `build.ps1` succeeds; `cargo fmt --check` and `clippy -D warnings` clean in the container
- [x] README updated
- [ ] Interactive: row shows 7 buttons, each runs, breathing pauses/resumes, buttons grey out, Restart/Stop clears the row; existing scenarios — user (computer use can't reach the sim window)

## Review
- Found today: attention_animation, boot_sequence, data_processing_led, settle, angry_animation, nose_boops, trench_run. `settle` is a helper; renaming it `_settle` would hide it (firmware change, user's call).
- Background loops are found by parsing `code.py`, not by watching tasks, so the row doesn't change while boot runs.
- A button only clears `pause_breathing` if it set it, so it never cuts short a pause a firmware trigger owns.
- No `trench_active` special case: a button-started trench run doesn't hold off the LED scheduler (documented).

# Simulator UI polish (#50, #52, #56, #59)

Branch: `feature/simulator-ui-polish` (off `Documentation`). One commit per issue, one PR.

- [x] #50 PCA9685 grid: servo angle for ch 0/1, colour swatch per eye
- [x] #52 Log: search, Firmware / [sim] toggles, red tracebacks, Clear / Copy / Save
- [x] #56 Camera views (Front, Side, Top, Eyes, Reset), Screenshot, Record frames
- [x] #59 Collapsible sections, saved prefs (`ddl0-sim.prefs.toml` next to the exe), shortcuts, Pause
- [x] `build.ps1` succeeds; `cargo fmt --check` and `clippy -D warnings` clean in the container
- [x] Headless test passes (all 16 checks)
- [x] Interactive: boot, breathing, N → boop, S → angry, I → attention; Space pause freezes sim time; camera views; log search, red SyntaxError traceback, Save; Screenshot; Record (22 frames in ~2 s); folded section restored after restart
- [ ] Copy button — not checked (no clipboard access in the session) — user
- [x] README updated

## Review
- Copy needs bevy_egui's `manage_clipboard` feature, which adds `arboard` / `clipboard-win` to Cargo.lock. No existing crate versions changed.
- Pause needed a Python-side change: while paused the scheduler sleeps 50 ms per pass, or animations that yield with `asyncio.sleep(0)` would spin a core against a frozen clock.
- `docker/build.sh` used to `rm -rf dist`, which would have wiped the prefs and captures on every build; it now keeps those two.
- Once saved, the Model choice overrides `[model] kind` in sim.toml (documented). Delete the prefs file to fall back.
- Shortcut keys are consumed, so a button focused by a click doesn't also fire on Space.

# Trench run idle animation (Top Gun: Maverick)

Branch: `feature/trench-run` (off `Documentation`).

- [x] `behaviours/trench_run.py`: random manoeuvres (tight turns split the wings, climbs lift both, dives drop both), smoothstep transitions, buffet flutter (~7 Hz, stronger under load), orange afterburner eyes, white LED flicker; runs `TRENCH_RUN_S` (300 s)
- [x] `idle_attention()`: 50/50 attention or trench run; a nudge aborts the run and `motion_sense` keeps `pause_breathing` until its boop / angry ends
- [x] `trench_active` event: LED scheduler skips or stops its flicker while the run owns the white LED
- [x] `TRENCH_RUN_S` in `config.py` / `settings.toml`; README behaviour, tuning and sound tables
- [x] Headless test: accepts either idle animation, checks a trench run is aborted by the next nudge, second idle trigger at 100 s; silent stand-ins for sounds missing locally
- [x] Headless passes (4 runs: trench picked in 2); forced-trench copy: aborts on nudge, and with `TRENCH_RUN_S = 8` completes and resets
- [ ] Interactive sim (Trigger idle now until the trench run plays; nudge mid-run; full run at fast-forward) — user
- [ ] Add `sounds/trenchRun.wav` — user (missing file crashes the firmware when the run starts)
- [ ] Hardware check: servo strain at the buffet frequency — user

## Review
- Wings swing 20–90° on both servos (left mirrored, as in `angry_animation`): the span boops (20–70) and angry (30–90) already use. First version used the attention ranges, which gave the left wing only 20° of travel; user asked for more pronounced moves. No calibration changed.
- Sound loops a short `trenchRun.wav` (full length won't fit on CIRCUITPY) and stops when the run ends or is aborted.
- On abort the afterburner is cleared (eyes off, white LED idle), because a boop only resets some eye channels; the small eye would otherwise stay orange.

# Breathing ignores `pause_breathing`

Branch: `bugfix/breathing-pause` (off `Documentation`).

- [x] Root cause: `ambient_breathing()` never checked `pause_breathing`, so while boop / angry / attention ran it kept writing wings (ch 0/1) and eye LEDs every 0.1 s, interleaved with the animation's writes
- [x] Fix: each tick, if `pause_breathing.is_set()` sleep 0.1 s and skip; `t` is held, so breathing resumes from the same point in its cycle
- [x] Headless test passes in the `ddl0-sim-dev` image (all 13 checks)
- [x] Probe (scratch, not committed): tag each ch 0/1 write by calling function, count breathing writes inside each animation
- [ ] Hardware check — user

## Review
- Before: 682 breathing wing writes inside animations (angry 66 + 68, boop 132 + 126 + 258, attention 32).
- After: 0 inside any animation. The probe still shows 2 (ch 0 + ch 1 at 79.29 s) because it merges two back-to-back boops into one span; those writes land in the gap after the first boop ended and before the second boop's sound started (79.3 s), when the pause is correctly clear.
- On resume the wings step from the animation's end pose to breathing's held position in one tick. Before the fix that same step happened on the next breathing tick anyway.

# nose_boops() / angry_animation() slower than written (follow-up to #17)

Branch: `bugfix/motion-speed` (off `Documentation`, after PR #26).

- [x] Measure in the headless sim at board speed (estimated `[board]` figures, #20 not run yet), first to last wing write:
  - `angry_animation`: ~4.0 s vs 2.4 s written (3 flaps × 2 strokes × 0.4 s), ~1.7x slow
  - `nose_boops`: ~7.9 s vs 4.8 s written (2 boops × 2 strokes × 1.2 s), ~1.65x slow
- [x] Root cause: same as #17. Fixed step counts (20 / 60 per stroke) with `sleep(duration / steps)`, plus 2 PCA9685 writes (~4 ms) and scheduler overhead per step
- [x] Both now share a clock-driven `sine_flaps()`; same `duration` (per stroke), angles, end pose and eye-colour logic
- [x] Headless test passes
- [ ] Interactive sim (nudge → boop, hard shake → angry, Board speed on) — user
- [ ] Hardware check — user; re-check once #20 has real `[board]` figures

## Review
- After: angry 2.42 / 2.44 s, boops 4.80–4.84 s. ~6.4 ms per frame, so ~62 frames per angry stroke and ~187 per boop stroke (was 21 / 61).
- Trajectory kept exactly, including the snap between strokes: each stroke ends where it started (min → max → min, then max → min → max), so the wing jumps min ↔ max at every stroke boundary, as before.
- Eye colour after a boop still comes from the last stroke's end angle (`angle_max`, so full blue / dim red), as before.
- Not changed: `ambient_breathing` never waits on `pause_breathing`, so it keeps writing the wings ~10x/s during these animations (and attention). Pre-existing; worth its own issue.

# Issue #17 — attention_animation() much slower on the robot than written

Branch: `bugfix/attention-speed` (stacked on `chore/tidy-code`, PR #24).

- [x] Measure in the headless sim at board speed (estimated `[board]` figures, #20 not run yet): 9.31 s for 5 flaps = 1.86 s/flap vs 0.2 s written
- [x] Root cause: fixed step count (61/flap) with fixed sleeps; every step also costs 8 PCA9685 writes (~16 ms), so the sleeps were a small fraction of the real time
- [x] Drive the animation from `time.monotonic()` so each flap takes `duration` whatever the write cost; flap duration 0.5 s (user's choice)
- [x] Headless test passes
- [ ] Interactive sim (Trigger idle now, Board speed on) — user
- [ ] Hardware check — user; re-check once #20 has real `[board]` figures

## Review
- After: 2.54 s for 5 flaps = 0.51 s/flap, ~23 frames per flap (~22 ms each).
- Frames yield with `asyncio.sleep(0)`: the 8 writes already take longer than one 50 Hz servo period, so sleeping more would only cost smoothness.
- Not changed: `nose_boops` (60 steps × 2 writes) and `angry_animation` (20 steps × 2 writes) have the same fixed-step pattern, so they also run slower than written, though less badly.

# Issue #18 — Tidy code.py: duplicate import and unused task variable

Branch: `chore/tidy-code` (off `Documentation`).

- [x] Duplicate `import random` — already gone after the module split (#19)
- [x] `code.py`: drop the unused `play_wav_task` variable (the startup sound task is still created)
- [x] `code.py`: remove commented-out `move_s1` / `move_s2` tasks
- [x] Remove legacy commented `nose_boops` (`behaviours/motion.py`) and `attention_animation` (`behaviours/attention.py`)
- [x] Headless test passes in the `ddl0-sim-dev` image
- [ ] Hardware check — user (no behaviour change expected)

## Review
- Headless: all 12 checks pass; startup sound (`008.wav`) still plays, boop / attention / angry unchanged.
- Kept the servo notes at the bottom of `code.py` and the commented alternatives inside live functions: they're the user's.
