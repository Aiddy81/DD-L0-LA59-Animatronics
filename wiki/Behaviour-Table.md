# Behaviour table

| Behaviour | Function (file) | Starts when | What she does | Sound | Stops when |
|---|---|---|---|---|---|
| Start-up | `boot_sequence()` (`behaviours/boot.py`) | Power-on or restart | Wings and eyes sweep in smooth waves. Logs `Boot sequence complete!`. | One random WAV from `sounds/`, just after | It finishes |
| Breathing | `ambient_breathing()` (`breathing.py`) | Always, after start-up | Wings rise and fall every `BREATH_PERIOD_S`; eyes glow between `EYE_MIN_BRIGHTNESS` and `EYE_MAX_BRIGHTNESS`. | — | Pauses while `pause_breathing` is set |
| Boop | `nose_boops()` (`motion.py`) | z changes more than `NUDGE_Z_MG` from rest | Two gentle wing flutters; the eyes shift between red and blue with the wings. Restarts the idle wait. | `noseBoops.wav` | It finishes |
| Angry | `angry_animation()` (`motion.py`) | `ANGRY_NUDGES` nudges inside one angry window, or z above `HARD_MOTION_MG` | Hard flaps, both eyes red. | `angry.wav` | It finishes |
| Attention | `attention_animation()` (`attention.py`) | Idle wait (`IDLE_MIN_S`–`IDLE_MAX_S`) ends, 50% chance | Five quick flaps (about 2½ s); eyes cycle through every colour. | `youWho.wav` | It finishes |
| Trench run | `trench_run()` (`trench_run.py`) | Idle wait ends, the other 50% | Flight moves for `TRENCH_RUN_S`: turns split the wings, climbs lift both, dives drop both, buffeting under load. Eyes glow orange, brighter on climbs; white LED flickers. | `trenchRun.wav`, looped | Time's up, or a nudge over `ANIMATION_NUDGE_Z_MG` |
| Thinking light | `random_led_scheduler()` → `data_processing_led()` (`leds.py`) | Every 5–60 s at random | White LED flickers for 2–6 s. | — | It finishes; skipped while `trench_active` is set |

All of these run as `asyncio` tasks started in `code.py`, except the boop, angry, attention and trench run, which are called by the motion and idle tasks. See [How the firmware works](How-the-Firmware-Works).

In the simulator, every one that takes no arguments has a button in the **Behaviours** panel.
