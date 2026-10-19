# Issue #19 — Tunables in settings.toml, behaviours in modules

Branch: `feature/firmware-modules` (off `Documentation`).

- [x] Split `code.py` into `config.py`, `hardware.py`, `behaviours/{boot,breathing,motion,attention,leds}.py`
- [x] `settings.toml` with every tunable; defaults in `config.py` equal today's values
- [x] Simulator: firmware dir importable in strict mode, `os.getenv` reads `settings.toml`, heap meter counts all modules, headless stages modules, hot reload watches all firmware files
- [x] README (Tuning section, layout, deploy) and CLAUDE.md updated
- [x] Headless: baseline (old `code.py`) vs new — same checks, same seeded random values, same audio
- [x] `simulator/build.ps1` succeeds; app boots the split firmware (strict + board speed)
- [ ] Interactive app scenarios (nudge / idle / angry buttons) — user
- [ ] Hardware check — user

## Review
- Headless (python:3.13-slim, seed 1, same sounds): all 9 checks pass for old and new. Logged random values (angry window, idle waits, LED intervals) and the audio sequence match the old firmware; the only difference, the last LED interval, differs just as much between two runs of the *old* firmware (wall-clock jitter in `data_processing_led`).
- Heap peak: old ~25 KB, new ~24 KB of 130 KB (desktop estimate). App shows ~19 KB at boot.
- No `settings.toml` → same behaviour as with it (defaults match). Edited file → idle waits 10–20 s, and a quoted `BREATH_PERIOD_S` prints a warning and uses the default.
- Syntax error in `behaviours/leds.py` → traceback naming the file and line, run ends cleanly, no crash.
- Firmware modules run with the same gated builtins as `code.py`, so strict mode and `os.getenv` apply inside them too (plain imports would have bypassed both).
- Not changed: the blocking `time.sleep` in `data_processing_led` (pre-existing bug, follow-up); `.github/copilot-instructions.md` still says settings.toml is unused (off-limits).
