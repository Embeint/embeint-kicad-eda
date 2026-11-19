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

## Bundled MCP (delivery gate 1, MCP part)

`build_windows.ps1 -McpOnly` froze the committed tree at `a75a6af75f` (clean) and installed
`C:\kb\install\mcp\embeint-kicad-mcp.exe` (74 MB with its Python 3.12 runtime and notices) after
`check_package.py` passed. Identity reported by `--version`, `live_mcp_capabilities` and
`serverInfo`: server `0.1.0+ga75a6af75f`, application `10.99.0`, schema
`214ce118ab3b9bfa24fc06818000fb6443c8a4340976875bcc0569f96d9b5115`, 112 tools, 145 commands.

Every step below ran with an environment of only the Windows essentials (PATH =
`System32;Windows`), and the MCP client came from a separate venv holding only the `mcp`
package, so neither the repository's venv nor pynng or protobuf were reachable:

| Step | Result |
| --- | --- |
| `--version` | identity above, `build_schema_sha256_matches: true` |
| `--diagnose` with no KiCad | `default_missing`, action names the API preference and `KICAD_API_SOCKET` |
| `live_schematic_documents` with no KiCad | tool error in 0.02 s: no KiCad editor is listening, nothing sent |
| Disposable project: place R77, undo, place, `live_schematic_save` | undo restored the sheet; file unchanged until the explicit save |
| Close KiCad (project manager closed cleanly), reopen, new MCP session | saved R77 present |

The source-checkout MCP passed the same unit and live suites on this branch (unit 328 passed,
47 skipped; all seven e2e suites including SolidCraft; WSL unit 372 passed, 3 skipped).

Not qualified here: a clean machine (this host has Python and Visual Studio), an installer,
code signing, and the KiCad side of a full `build_windows.ps1` run. That run stopped at
`cmake --install` because a user's KiCad started from the install held `kigal.dll`; nothing in
the install was replaced. The KiCad test instance was started by `run_kicad_windows.ps1` from
the checkout for its isolated configuration and disposable project; the MCP did not use the
checkout.

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
