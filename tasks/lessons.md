# Lessons

## Motion sensing (PR #35, Oct 2026)

- **Measure the robot before tuning thresholds.** The Feather is mounted sideways: at rest z reads ~0.3–0.4 g (gravity mostly on x), and it shifts with how the droid sits (0.83 g seen once). Thresholds that assume z = 1 g are wrong. Rule: compare against a measured resting value, never a fixed absolute.
- **The simulator can't show self-triggering.** It has no servo vibration, so every threshold passed headless tests and still looped on the robot. Rule: treat simulator passes as "doesn't crash"; motion tuning needs a robot log (`log_mode`, see README "Logging on the robot").
- **The droid shakes itself.** Wing snaps at the end of animations, breathing, and the trench run's buffeting all move the accelerometer, most strongly on x/y. Rule: read only z, wait `SETTLE_S` after any wing animation before re-measuring rest, and use a higher limit while attention or the trench run owns the wings.
- **The serial console can't be watched with the wings running** (no data connection while the servos are powered). Rule: debug hardware behaviour via `log.txt`. Before using COM6, ask the user to stop the VS Code serial monitor, since only one program can hold the port.
