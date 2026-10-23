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
