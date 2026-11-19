# Native Windows MCP qualification (2026-11-19)

Bounded evidence that the MCP adapter drives the native Windows build of Embeint KiCad
(`scripts/build_windows.ps1`) over KiCad's local named pipe. It is not a clean-machine or
installer qualification.

## Environment

- Windows 11 Pro 10.0.26320, Windows PowerShell 5.1, Python 3.12 (python.org) in `C:\kb\venv`.
- KiCad `10.99.0-unknown` built from `main` at `d3d5e00c40` with MSVC (Visual Studio 2026),
  installed in `C:\kb\install`; stock libraries linked from upstream KiCad 10.0.
- Test instance started by `run_kicad_windows.ps1`: isolated configuration in
  `C:\kb\live\config`, disposable project `C:\kb\live\projects\live-demo`, project manager.
- Endpoint observed: `ipc://C:\Users\<8.3 name>\AppData\Local\Temp\kicad\api.sock`
  (`\\.\pipe\...`); the user's temp directory was the 8.3 short spelling.

## Results

| Check | Result |
| --- | --- |
| Unit tests on Windows (`scripts\run_tests.ps1`) | 326 passed, 46 skipped (Unix-socket peers) |
| Unit tests under WSL, Python 3.14, separate venv | 369 passed, 3 skipped |
| `e2e_symbols`, `e2e_footprints`, `e2e_smoke`, `e2e_features`, `e2e_pcb`, `e2e_design` (`run_tests.ps1 -Live`) | all pass |
| `e2e_model_handoff` with `solidcraft-cli` (headless) | pass; embedded STEP byte-identical to the export |
| `e2e_persistence` seed, explicit save, KiCad restart, verify | pass |
| `e2e_registry_audit` (read-only, empty board) | pass |
| `diagnose.py --probe` | one `named_pipe` candidate, responding, project `live-demo` |
| MCP stdio via `run_mcp_windows.ps1`, minimal environment | place, undo, place, explicit save, restart, symbol present |

## Findings fixed on the way

- Net class commands compare against `GetProjectPath()`, which ends in `\` on Windows; the
  client appended `/`.
- `zstandard` (the pre-3.14 `compression.zstd` fallback) was missing from `requirements.txt`.
- The `.NET` temp path is the long spelling while KiCad uses `%TEMP%` as given (8.3 here);
  the launcher checks every spelling and `wait_ready.ps1` prints the exact endpoint.
  Correction (review, 2026-11-20): that launcher check never matched, because Windows
  PowerShell lists drive-letter pipe names without the `\\.\pipe\` prefix. The launcher
  now asks `instance_discovery.py`, and a unit test checks the refusal against a real pipe.

## Known limitations

- One KiCad per Windows account can serve the API: KiCad's `api-<pid>.sock` fallback tests
  for a file and never triggers for a pipe. The launcher refuses to start a second one.
- Deleting the library folder of the footprint open in the Footprint Editor makes wx show a
  modal "Unable to set up watch" error from the watcher thread (outside `wxLogNull`), which
  blocks the API until dismissed. The e2e tests avoid it; the editor is unchanged.
- A symbol search takes about 8 s with every library loaded (about 14 s through the
  install's junctions, which the launcher avoids); a first search right after startup can
  exceed a 60 s client timeout.
- Rebuilding installs over `C:\kb\install`, which fails while any KiCad started from it runs.

Raw logs were kept outside the repository.
