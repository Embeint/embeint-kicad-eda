# KiCad 10.0.x compatibility matrix

This records which files written by the development fork can be opened by released KiCad 10,
what happens to files from KiCad 10 when the fork saves them, and how the MCP exports are kept
reproducible. It covers P1 item 4 of the [MCP support plan](MCP_SUPPORT_PLAN.md) and the
"Compatibility and maintenance" section of the [roadmap](EMBEINT_FORK_ROADMAP.md).

**Summary.** The fork (10.99) writes newer schematic, board, symbol-library and footprint-library
formats than KiCad 10.0.6. KiCad 10.0.6 refuses all four. **Files saved by the fork cannot be
opened in KiCad 10.0.x.** No backward compatibility is claimed. Files from KiCad 10.0.6 open in
the fork. Saving them is a one-way upgrade. Their content survives a fork save and reopen, except
teardrops, which the fork regenerates when it opens the board.

## Builds tested (2026-11-19)

| | Reference | Fork |
| --- | --- | --- |
| Application | KiCad **10.0.6**, release build (`kicad-cli version`: `10.0.6`; built Aug 28 2026, MSVC, wxWidgets 3.3.2, OCC 7.9.2) | `10.99.0-unknown` (`cmake/KiCadVersion.cmake`), release build |
| Platform | Windows 11 x64, `C:\Program Files\KiCad\10.0` | WSL Ubuntu development build (`~/kicad-live`) |
| Source | upstream installer | branch `mcp/kicad-10-compat`, C++ identical to `d3d5e00c40` |

Raw logs and generated projects are kept outside the repository and are not committed. The run
directories are named in the procedure below.

## Format versions actually written

Values are read from files each build saved (`check_compat.py versions`), not inferred from the
release number. The reference values come from KiCad 10.0.6 rewriting copies of its own demos
(`kicad-cli … upgrade --force`, and its `pcbnew` Python module for the project files). The fork
values come from the committed fixture and are confirmed in the source.

| File | KiCad 10.0.6 writes | Fork writes | Fork source |
| --- | --- | --- | --- |
| `.kicad_sch` | `(version 20260306)`, `generator_version "10.0"` | `20261106`, `"10.99"` | `eeschema/sch_file_versions.h` |
| `.kicad_pcb` | `20260206`, `"10.0"` | `20261031`, `"10.99"` | `pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.h` |
| `.kicad_sym` | `20251024`, `"10.0"` | `20261106`, `"10.99"` | `SEXPR_SYMBOL_LIB_FILE_VERSION` |
| `.kicad_mod` | root `(footprint …)`, `20260206` | root **`(kicad_footprint_lib …)`**, `20261031`, generator `kicad_footprint_editor` | `SEXPR_FOOTPRINT_LIB_FILE_VERSION` |
| `.kicad_pro` | `meta.version` **3**; design_settings 2, erc 0, net_settings 5, tuning_profiles 0, schematic 1 | **4**; design_settings 4, erc 1, net_settings 5, tuning_profiles 2, schematic 1 | `common/project/project_file.cpp` and the nested settings |
| `.kicad_prl` | **5** | **7** | `common/project/project_local_settings.cpp` |
| `sym-lib-table`, `fp-lib-table` | `(version 7)` | `(version 7)` | `common/libraries/library_table.cpp` |
| `.kicad_dru` | as authored (shipped demos use `(version 1)`) | `(version 2)` (API and rule editor) | `pcbnew/api/api_handler_pcb.cpp` |

Embedded files (the STEP model in a footprint, and the footprint's copy in the board) use the
`embedded_files` syntax in both builds. They are only readable if the file containing them is.

## Matrix A: fork-written files in KiCad 10.0.6

The input is the committed fixture, `compat/fixture/compat-fixture`, authored and saved through the
MCP client (`compat/build_fixture.py`). It is a two-sheet hierarchy (root divider, `Power` child
sheet with sheet pin and hierarchical label), with a project symbol library, a project footprint
library whose SOT-23 embeds a STEP model, both library tables, net classes, custom rules, a routed
board with a GND pour, and Embeint title blocks. Expected results are committed in
`compat/expected/reference-on-fork-fixture.json`.

| Fork file or operation | Result in KiCad 10.0.6 |
| --- | --- |
| `.kicad_sch`: ERC and the six schematic exports (svg, pdf, dxf, ps, netlist, bom) | **Refused**: `Failed to load schematic` (exit 3). The parser rejects any newer schematic version. |
| `.kicad_pcb`: DRC and the twelve board exports it offers | **Refused**: "created with a more recent version … upgrade KiCad to version 10.99 or later (file format dated 20261031 or later)" (exit 3) |
| `.kicad_sym` (`sym upgrade`) | **Refused**: `Unable to load library` |
| `.kicad_mod` / `.pretty` (`fp upgrade`), with its embedded model | **Refused**: `Unable to load library` (new root token and newer version) |
| `.kicad_prl` | **Silently downgraded**: any reference `kicad-cli` project command rewrites it from version 7 to 5 and drops entries it does not know (`constraint_shadows`, `subgrids`, `via_stitching` in the visible-items list). This happens even when the board is refused. |
| `.kicad_pro` (v4) beside reference-authored designs | Read without error. ERC, DRC and Gerber export succeed and the file is not rewritten by `kicad-cli`. The KiCad 10 GUI was not tested. |
| `.kicad_dru` `(version 2)` beside a reference-authored board | Parsed without error. DRC ran (clean) and Gerbers exported. Enforcement of each rule was not separately proven. |
| Library tables (`version 7`) | Same declared version and syntax in both builds. Not isolated: the fork libraries they name are refused. |
| MCP `dsn` and `idf` exports | Not available from `kicad-cli` 10.0.6; not compared |

The same results (sym and fp reads excepted) hold for the reference project after the fork has
saved it. Opening a design in the fork and saving it makes it unreadable to KiCad 10.0.6.

## Matrix B: KiCad 10.0.6 files in the fork

The input is built by `check_compat.py reference-fixture`. It takes the reference's
`complex_hierarchy` demo (hierarchy, project symbol and footprint libraries, library tables), adds
a footprint with an embedded STEP model and the rules file from the `vme-wren` demo, and has KiCad
10.0.6 rewrite every file: sch 20260306, pcb 20260206, sym 20251024, mod 20260206, pro 3, prl 5,
rules version 1. As a baseline, KiCad 10.0.6 runs ERC, DRC and every export on it successfully.
ERC is clean; DRC reports 68 footprint/symbol parity warnings that the demo already has. Expected
results are in `compat/expected/reference-on-reference-fixture.json`.

| Step | Result in the fork |
| --- | --- |
| Open project, schematic and board (project manager, IPC) | OK |
| Explicit save (schematic, board) | Writes sch 20261106, pcb 20261031, pro 4, prl 7. Rules file, library tables and libraries are untouched. In-memory content is identical before and after the save. |
| Restart and reopen the saved files | Hierarchy, every schematic item, nets and every board item are identical. The reopen leaves the files unchanged. |
| Teardrops | The fork **rebuilds teardrop zones when it opens the reference board**: 165 zones in the KiCad 10.0.6 file become 167, with new outlines. It writes their invisible border as `hatch none`, which reads back as a visible solid border. Teardrop zones store no UUID in either version. Their geometry is stable across the fork's own save and reopen. This is upstream 10.99 code (teardrop and writer sources match `master`). |
| Symbol and footprint libraries (fork `kicad-cli sym/fp upgrade`) | Rewritten to 20261106 / 20261031 with the `kicad_footprint_lib` root; the embedded model is kept |
| Library pin and graphic UUIDs inside schematic symbol definitions | Not stored in the file by either version (generated on load); excluded from comparison |

## Unsupported downgrade cases

- Opening fork-saved `.kicad_sch`, `.kicad_pcb`, `.kicad_sym` or `.kicad_mod` in any KiCad 10.0.x
  (tested on 10.0.6). There is no export-to-10.0 path.
- A project opened in the fork and saved: every schematic and board file is upgraded, so the
  project no longer opens in 10.0.x. Keep a 10.0.x copy before opening a shared design in the fork.
- Running KiCad 10.0.x on a fork project rewrites `.kicad_prl` to version 5 and discards
  fork-only local settings, even when the design itself is refused.
- Fork-only content (for example, design data introduced after 10.0) has no 10.0.x equivalent.
  Because the files are refused as a whole, no partial downgrade exists to lose it silently.

## Deterministic MCP export fixtures

`check_compat.py fork-exports` (via `compat/run_live.sh exports`) opens a copy of the committed
fixture in the fork and runs every `live_schematic_export` and `live_pcb_export` kind twice. It
normalizes each output and compares the normalized tree with `compat/golden/mcp-exports.json`.
It fails on an export error, a difference between the two runs, a golden mismatch, or any change
to the project files.

| Kinds | Comparison |
| --- | --- |
| sch: svg, pdf, dxf, ps, netlist, bom; pcb: gerbers (+ job file), drill, position, pdf, svg, dxf, ipcd356, stats, dsn, gencad, idf | Exact bytes after normalization |
| pcb: step | Structure: histogram of STEP entity types. OCC numbers products and assembly usages with per-process counters and orders colour entities differently on each export. |
| pcb: ipc2581, odb | Structure: XML element histogram, or ODB++ record-type histogram per archive member. KiCad (10.0.6 and the fork) emits these in hash order, and renumbers symbol and padstack tables per run. |

The normalization (`compat/formats.py`) removes only these volatile parts:

- creation and modification dates (ISO, slash, locale and ctime formats; PDF `D:` dates);
- the application version and generator strings;
- PDF `/ID`, `/Producer`, `/Creator` and cross-reference offsets;
- PostScript `%%CreationDate`/`%%Creator`/`%%Title`;
- STEP `FILE_NAME`;
- Gerber/drill `TF.CreationDate` and `TF.GenerationSoftware`, and the job file's `CreationDate` and `GenerationSoftware`;
- ODB++ `misc/info` save fields;
- absolute project and output paths, in POSIX, WSL and Windows (escaped) spellings.

Content changes are not normalized; `test_compat.py` checks this. The fixture's title-block date is
fixed so plotted pages do not change from day to day.

Results: all 20 kinds were deterministic within a session and matched the golden values in a fresh
session. Applied to KiCad 10.0.6's own outputs, the same normalization makes all 57 files
reproducible across two reference runs.

The structural comparison catches added or lost items, not moved coordinates. Golden values belong
to the fork build that produced them. When upstream changes a plotter or exporter, inspect the
difference and regenerate with `run_live.sh exports OUT --update`.

## Procedure

The reference half needs no development build (Windows, PowerShell):

```powershell
$K = "C:\Program Files\KiCad\10.0"; $C = "tools\live_schematic_mcp\compat"; $S = "<new scratch folder>"
python -I $C\check_compat.py reference-fixture --cli "$K\bin\kicad-cli.exe" --python "$K\bin\python.exe" --share "$K\share\kicad" --out $S\ref-fixture
python -I $C\check_compat.py reference --cli "$K\bin\kicad-cli.exe" --expect $C\expected\reference-on-fork-fixture.json --out $S\ref-on-fork-fixture
python -I $C\check_compat.py reference --cli "$K\bin\kicad-cli.exe" --project $S\ref-fixture\ref-fixture --expect $C\expected\reference-on-reference-fixture.json --out $S\ref-on-ref
python -I $C\check_compat.py reference-mixed --cli "$K\bin\kicad-cli.exe" --reference-project $S\ref-fixture\ref-fixture --out $S\ref-mixed
```

The live half runs in WSL with the shared build lock held, after `scripts/rebuild.sh`. Each step
starts KiCad on a disposable copy and stops only the KiCad it started:

```bash
compat/run_live.sh exports   OUT                     # MCP exports of the fixture vs golden
compat/run_live.sh roundtrip OUT /mnt/c/.../ref-fixture/ref-fixture   # open, save, restart, verify
compat/run_live.sh cli-libs  OUT /mnt/c/.../ref-fixture/ref-fixture   # fork CLI rewrites the libraries
compat/run_live.sh fixture   OUT model.step          # re-author the fixture (then review and copy it)
```

To check the fork-saved project in the reference, run `check_compat.py reference --project
OUT/<project>`. Each subcommand writes `report.json` into a new folder. `test_compat.py` (part of
`scripts/run_tests.sh`) checks the normalizers, the fixture headers and the coverage of the
golden and expected files offline.

Evidence for the results above (local scratch, `C:\Users\KarlVonRichter\.t3\scratch\kicad-10-compat`):
`ref-fixture-1`, `ref-on-ref-1..3`, `ref-on-fork-fixture-1..2`, `ref-on-fork-saved-ref-1..2`,
`ref-mixed-1`, `fork-fixture-5` (fixture authoring, ERC/DRC), `fork-exports-2` (golden),
`fork-exports-3` (fresh-session match), `fork-roundtrip-2`, `fork-cli-libs-1`.

## Fixture checks and limitations

- In the fork, the fixture has four ERC warnings and one DRC warning. The ERC warnings are
  `lib_symbol_mismatch` for the stock `Device` symbols, compared with the library copies in the
  development configuration. The DRC warning is a silkscreen clearance of 0.071 mm on the
  generated SOT-23. They are kept as fixture facts, not electrical sign-off.
- Only KiCad 10.0.6 on Windows x64 was tested as the reference. Other 10.0.x releases share the
  file versions only if their headers match. Rerun `reference-fixture` and compare.
- The KiCad 10.0.6 GUI was not exercised. The `.kicad_pro` and `.kicad_prl` behaviour of its
  editors may differ from `kicad-cli`.
- Rules files: only parsing by the reference is shown, not that each fork-written constraint is
  enforced identically.
- The round trip covers one reference design. Features it does not contain (variants, design
  blocks, simulation models, embedded fonts) are untested.
- No native file format was changed for this work.
