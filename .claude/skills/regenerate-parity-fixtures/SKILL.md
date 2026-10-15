---
name: regenerate-parity-fixtures
description: Regenerate the golden parity fixtures in fixtures/parity/*.json from the Python reference app (Orb of Many Functions). Use only when a fixture must deliberately change.
---

# Regenerate parity fixtures

Golden fixtures in `fixtures/parity/*.json` are produced by `tools/parity/generate.py`. Run it from PowerShell, at the repo root, with the reference repo's venv:

```powershell
& C:\Users\anton\Documents\projects\orb-of-many-functions\.venv\Scripts\python.exe -I -B tools/parity/generate.py C:\Users\anton\Documents\projects\orb-of-many-functions
```

- Regenerate only deliberately. Explain every diff in the commit message.
- The reference repo is read-only. Never modify it.
- Rust tests read the fixtures via `CARGO_MANIFEST_DIR/../../fixtures/parity`. Run `cargo test` afterwards.
