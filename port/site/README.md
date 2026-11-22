# Public Rust port progress site

Build from the repository root using Python 3.11 or newer. The generator uses only
the Python standard library; there are no added runtime or browser dependencies.
Source code is GPL-3.0-or-later.

```sh
python -B -m unittest discover -s port/site -p test_progress.py -v
python -B port/tools/progress.py --out _site
```

`--root PATH` reads a different checkout or fixture. `--out PATH` writes the static
HTML, CSS and `progress.json`; publish only that output directory. Builds are
deterministic: sorted inputs, no timestamps, no network requests, no live PR
queries and no data read from user designs. Existing output must carry the
generator's `.progress-site` marker. Rebuilding removes obsolete unit pages.
Do not commit generated output. To keep a local build outside this checkout,
pass an absolute temporary output path.

The implementation follows solidcore's progress generator, site and per-unit
schema at `7a588fa284765a83823293ae20b84da8356ab479` (SolidCraft trailer
`93d5492ef295a1124664d36e967f58a81a76d7d2`). KiCad retains its own plan's exact
live C++ oracle comparison; solidcore's later comparison/correctness policies do
not change KiCad's contract.

| Page | Public repository inputs |
| --- | --- |
| Overview | Unit status/line totals; `port/pins.toml`; extractor provenance |
| Units and individual unit pages | `port/units/<id>.toml`; cases, goldens and evidence; source ranges and claims |
| Dependencies | Hard/soft dependencies, layers; weighted cycle back-edges in `port/extractor/generated/units.json` |
| Inventory | `port/extractor/generated/{graph,inventory,units}.json`: file categories/coverage, wx classes, counts, source hashes, parse failures, uncompiled sources and limitations |
| Findings and upstream issues | Each `port/findings/*.md` and `port/upstream-issues/*.md`; evidence file links |

## Schema and interpretation

The solidcore core keys remain top-level TOML: `id`, `upstream`, `rust_module`,
`depends_on`, `soft_depends_on`, `est_lines`, `layer`, `tests`, `status`,
`claimed_by`, `claimed_at`, `notes`. IDs match file names. The five statuses are
`todo`, `claimed`, `partial`, `green`, `blocked`.

The extractor's version 1 extensions are `schema_version`, `wx_class` (`pure`,
`thin-replacement`, `UI`), `source_category` (`application`, `generated`,
`thirdparty`), `functions` (clang USRs), `source_ranges` (`path:start:end`, inclusive
1-based lines) and `coverage_gaps` (source paths). Extensions are shown separately;
source ranges link to source lines on `rust`. The Pages build runs
`python port/tools/validate_units.py` when the extractor has been integrated.
Its default mode validates honestly incomplete records; Pages does not add
`--require-complete` or a requirement to analyze every platform configuration.

Pins use `port/pins.toml`, schema version 1: `[kicad]` and `[solidcore]` each have
`url` and full `revision`; solidcore additionally records `solidcraft_revision`.
Missing pins are shown as unavailable, without inventing defaults.

All five extractor provenance booleans are displayed independently:
`inventory_complete`, `parse_complete`, `compile_database_complete`,
`all_configurations_complete`, `gate_complete`. The last is labeled **extractor
completeness**, not the plan phase 0 gate. A Linux WSL graph does not establish
Windows/macOS or inactive preprocessor branch coverage. Missing generated
artifacts are shown as unavailable, never inferred from the number of TOML files.
Available inventory is not synonymous with complete parsing or full coverage.

The plan phase 0 gate requires three units green against the live oracle in CI
and every generated unit file valid, with coverage gaps disclosed. This static
site does not execute or certify that gate. The overview heading is **Phase 0
verification requires live CI evidence**, including when there are no records or
stored evidence reports a pass. `phase0_gate` remains `"not_verified"`; current CI
is linked separately. No current pass is inferred from unit statuses, case/golden
counts, generated artifacts, an empty inventory or stored evidence.

Unit golden files are `port/goldens/<id>.jsonl`; case files are
`port/cases/<id>.jsonl`. Records are framed only by LF; CRLF and a final record
without LF are accepted. Literal U+0085, U+2028 and U+2029 inside JSON strings
remain part of the value or case identity. Bare CR does not frame a new record.
Counts exclude whitespace-only lines, and duplicate keys/case IDs or malformed
protocol envelopes fail the build. Requests are `{unit,case,input}`;
replies are `{unit,case,result}` or `{unit,case,error}`. `unported` and `deferred`
errors are counted as deferrals; all other error envelopes count as failures.
Legitimate upstream operation statuses belong inside `result`. Stored counts
are expectations, not runtime results. A green declaration with recorded
deferrals, coverage gaps or no tests is **reported as partial**; a green declaration
with failed replies is **reported as blocked**. The original declaration is
preserved on the unit page and in `progress.json`. Partial lines are always
counted separately from green lines. The graph reader supports both `graph.json`
and the extractor's canonical deterministic `graph.json.gz` artifact.

Evidence links come from `port/goldens/evidence/*.json`, `port/evidence/*.json`
and `port/evidence/*.md`. A file named `<id>.json` or beginning `<id>-` is linked
from that unit; the records page lists all evidence. These are recorded evidence
links, not a claim that current CI passed. Claims/notes can link to PRs through
`#123` or an explicit HTTP(S) URL.

`progress.json` has a site-specific version 1 schema: `summary` with status counts
and separate green/partial estimated lines, `phase0_gate = "not_verified"`,
`extraction` (available artifacts, completeness flags, path/back-edge counts;
missing values are null), and `units` (reported/declared statuses, layer and stored
case/golden counts). This does not modify the unit or harness schema.

## Publication boundaries

Only public QA data, demos, official libraries and generated cases belong in
these inputs. The generator does not scan arbitrary files, copy designs or
include JSON case bodies in the site. Source/evidence links point to this public
fork's `rust` branch. Local build paths and compiler diagnostic text in extractor
provenance are omitted; source identities, hashes and coverage diagnostics remain
visible. Symlinks cannot read outside the source root.

TOML, Markdown and JSON text is HTML-escaped. Markdown supports inline code,
HTTP(S) links and PR numbers; raw HTML and images stay text. There is no script,
remote asset or third-party browser dependency. A restrictive content security
policy is included, and pages work without JavaScript.

`.github/workflows/rust-port-pages.yml` builds and tests on every push to `rust`
and every PR targeting `rust`, without path filters. PRs upload an artifact only.
Deployment runs only from `refs/heads/rust`, with Pages write and OIDC permissions
limited to that job. All actions are pinned to commit SHAs; shared checkout and
Python pins match this repository. There is no settings mutation or Pages
auto-enablement step. The repository owner must enable GitHub Pages with the
GitHub Actions source and allow `rust` in the `github-pages` environment.
