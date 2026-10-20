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
