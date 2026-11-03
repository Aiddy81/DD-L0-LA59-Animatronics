# PyO3 security upgrade: #113, #114

Branch `bugfix/pyo3-0.29` off `Documentation`. Dependabot alerts #1 (high, GHSA-36hh-v3qg-5jq4) and #2 (medium, GHSA-chgr-c6px-7xpp) affect `pyo3 < 0.29`; the simulator was on `=0.25.1`. The owner approved the breaking crate upgrade.

- [x] Issues #113 and #114, one per alert
- [x] `Cargo.toml`: `pyo3 = "=0.29.3"` (latest 0.29 patch); `generate-import-lib` dropped (a no-op in 0.29, which links `python3.dll` with raw-dylib)
- [x] `Cargo.lock` updated in the container: only the 5 pyo3 crates changed; `indoc`, `memoffset`, `python3-dll-a` and `unindent` were dropped
- [x] `pyhost.rs`: `prepare_freethreaded_python` → `Python::initialize`, `with_gil` → `attach`, `allow_threads` → `detach`, `downcast` → `cast` (×2)
- [x] `build.ps1 -Test`: build passes with no warnings, every headless scenario check passes, 154/154 unit tests pass
- [x] `cargo fmt --check` and `cargo clippy -D warnings` clean
- [x] `ddl0-sim.exe` starts and loads `python3.dll` / `python313.dll`
- [x] Commit, push, PR into `Documentation`

### Review
- The simulator never called either vulnerable API, so this clears the alerts; the code wasn't exposed.
- 0.30.1 is out. We stopped at 0.29.3, the line that has the fix, to keep the change small.
- Dependabot closes the alerts only once this reaches `main`.
- The GUI was only checked as far as starting and loading Python; behaviour is covered by the headless scenarios.
