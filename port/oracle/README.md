<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# KiCad unit oracle

`kicad-oracle` is a Linux JSONL process linked to KiCad's **own** `kimath`, `core`
and Clipper2 targets. Their source and CMake files are exported unchanged from
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. No application targets or QA support
library are needed. The standalone CMake adapter leaves the normal root build
unchanged. It supplies wxWidgets/Boost, KiCad's global include directory and the
upstream feature checks/configuration template.

The only shipped registration initially is `example.kimath-rescale`. It calls
the compiled `rescale<int>` specialization in `libs/kimath/src/math/util.cpp`.
It establishes the build pattern, not a pilot unit, unit claim or port completion.
No unit records, cases, goldens or Rust workspace files are maintained here.

## Hosted Ubuntu build

Use `ubuntu-24.04`, with these apt dependencies (no KiCad package, display server,
OpenCascade, SWIG, protobuf, Git libraries or full KiCad build):

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends \
    g++ gcc cmake ninja-build python3 rsync git libboost-dev libwxgtk3.2-dev
python3 port/oracle/prepare.py --output-dir "$RUNNER_TEMP/oracle-stage"
ORACLE_JOBS=2 bash port/oracle/build.sh \
    "$RUNNER_TEMP/oracle-stage" "$PWD/port/oracle" "$HOME/kicad-port"
"$HOME/kicad-port/oracle-build/kicad-oracle" --provenance > oracle-provenance.json
"$HOME/kicad-port/oracle-build/kicad-oracle" < port/cases/<unit>.jsonl > actual.jsonl
```

Fetch enough Git history to contain the pin before exporting it. With
`actions/checkout` use `fetch-depth: 0`, or explicitly fetch that commit from the
Embeint fork. `prepare.py` works from the checkout and verifies that the exact
commit object exists. It uses `git archive`; it never silently uses the fork's
current reference implementation. The archive contains only the dependency
source/include closure (822 files at this pin), not all KiCad. The shims remain a
fork overlay copied by rsync. `prepare.py` requires no third-party Python package.

The build compiles all sources in the upstream `kimath` archive, six `core`
sources and three Clipper2 sources; the static linker pulls only needed objects.
The final compile database has 57 translation units, including the example,
driver and separate dispatch regression executable. Adding a
shim does not build an editor. Some geometry objects refer to code in `common`:
a shim reaching those symbols needs an explicitly reviewed dependency extension,
not mock implementations. Pure `transform`, `math/util` and `geometry/distribute`
can use this closure, subject to the extractor's final unit boundaries.

Plan a 5-minute hosted build budget initially (dependency installation additional).
This is a budget estimate, not a hosted timing claim. Local WSL build/test evidence
and actual measured cold-build timing are recorded in [validation.md](validation.md).
The hosted job must build the binary and execute the unit cases on every run.
Stored oracle outputs alone never establish a live oracle CI pass.

Start with no binary cache. A compiler cache can use this key structure:
`oracle-v1-<runner-image>-<arch>-<gcc-version>-<wx-version>-<boost-version>-<pin>-<sha256(port/oracle/**)>`.
Include `-fno-fast-math,-ffp-contract=off` in the key's fixed version contract.
Cache compiler objects with `ccache` only after auditing the launcher support;
the initial audit intentionally accepts the compiler directly, without launchers.
If caching the entire dedicated work root instead, use an exact key including
the compiler/package identities and absolute source/build paths, no fallback
restore key. The mandatory source/flag audit still runs. Do not cache the lock.

## Windows checkout and WSL

Run Git/export in Windows, where the worktree's Git metadata is valid:

```powershell
python port/oracle/prepare.py --output-dir C:/Users/KarlVonRichter/.t3/oracle-stage
wsl -d Ubuntu -- bash /mnt/c/path/to/checkout/port/oracle/build.sh `
    /mnt/c/Users/KarlVonRichter/.t3/oracle-stage /mnt/c/path/to/checkout/port/oracle
```

Replace `/mnt/c/path/to/checkout` with the actual worktree path. The script copies
only the overlay into `~/kicad-port/oracle-src/overlay` and extracts the pin into
`~/kicad-port/oracle-src/reference`, then builds in `~/kicad-port/oracle-build`.
Its optional third argument selects a different dedicated **Linux** work root.
It refuses `~/kicad-live`, `/mnt` build roots, `/` and `$HOME`.

Every build acquires `mkdir ~/kicad-port.build-lock` and releases only its own
lock on exit. A busy lock exits 3; retry after the owner finishes. Never remove
another worker's lock. `ORACLE_JOBS` defaults to 8 and accepts only 1 through 8;
use 2 on small hosted runners. No KiCad application is started, stopped or installed.

GCC is the default. `CC=clang CXX=clang++` is supported when Clang has a working
C++ standard library/toolchain. The WSL image's Clang 21 initially selected an
incomplete GCC 16 installation and could not find `libstdc++`; GCC 15 worked.
A working Clang invocation can select the installed GCC toolchain with
`CXXFLAGS=--gcc-install-dir=/usr/lib/gcc/x86_64-linux-gnu/15`.
Use a separate work root when changing compilers; CMake caches compiler identity.

## Protocol

With no arguments, read one strict JSON object per stdin line and write one
compact JSON object per stdout line, flushing each reply immediately. EOF exits
0. The final request need not end with a newline. No startup banners or build
messages go to the oracle's stdout. Requests are:

```json
{"unit":"example.kimath-rescale","case":"half","input":{"numerator":1,"value":1,"denominator":2}}
```

Reply:

```json
{"unit":"example.kimath-rescale","case":"half","result":1}
```

`unit` and `case` must be nonempty strings; `input` must be an object. The
envelope has exactly those three keys. Duplicate keys (including nested ones),
trailing garbage, blank lines and invalid JSON are refused. Successful results
are compared exactly as JSON, with no tolerance. Non-finite shim results are
execution errors; they are never silently serialized as `null`. A future shim
needing non-finite values must specify an exact string encoding in its contract.

A registered adapter can explicitly throw `oracle::UNPORTED(upstream, unit)`:

```json
{"unit":"request-unit","case":"case-id","error":{"kind":"unported","upstream":"symbol","unit":"dependency-id"}}
```

Only that kind means deferred. Ordinary errors have
`{"unit":...,"case":...,"error":{"kind":"...","message":"..."}}` with kinds
`invalid_request`, `invalid_input`, `unknown_unit`, or `execution_error`.
After a successful parse, malformed envelopes preserve parsed string identities
when possible, otherwise the identity is `null`. Parse failures (including duplicate
keys, numeric overflow and excessive nesting) return both identities as `null` and
message `invalid JSON or duplicate key`. The transport permits at most 127 open
object/array containers, including the root and empty containers, matching the
existing Rust request `serde_json::Value` bound. Scalars do not add a container
level. Both container-start events are checked before extracting identities,
including values in identity and unknown fields. See the native request boundary
regressions in [the difftest guide](../../rust/kicad-difftest/README.md).
These are failures, never successful or deferred cases.
The process continues after a per-line refusal; the harness must inspect **every**
reply, not just exit status. Process startup/I/O failure exits 1 and writes to
stderr; unsupported CLI arguments exit 2. A crash or missing reply is a failure.

`--list` writes one JSON array of `{unit,upstream}` registrations and exits.
`--provenance` writes one JSON build record and exits. These are separate CLI
modes, not extra JSONL records mixed into a case stream.

The example accepts exactly `numerator`, `value`, `denominator`, JSON integers
in [-1000000,1000000], a nonzero denominator and `abs(numerator*value) <= 2000000000`.
These bounds keep the upstream integer specialization's arithmetic and result
conversion defined. They are example input validation, not replacement math.

## Adding a measured unit

1. Wait for the extractor's final unit ID and source ranges. Read its upstream
   functions at the pin, including defaults, special branches and dependencies.
2. Add `shims/<unit>.cpp`, including `registry.h` and the pinned library headers.
   Implement only input validation, conversion, **calls into KiCad** and output
   serialization. Do not copy or reimplement the reference algorithms.
3. Implement `void Register<Name>(REGISTRY&)` in namespace `oracle`; add its
   forward declaration and call in `registry.cpp`. `REGISTRY::Add(id, upstream,
   function)` rejects duplicate/empty registrations. CMake discovers shim source
   files, while explicit registration avoids static initialization/order effects.
4. Keep inputs within the actual upstream preconditions; malformed fields return
   `invalid_input`. `Integer()` checks signed/unsigned ranges before narrowing.
   Use separate operations or shims for multiple functions and document the
   operation schema beside the unit's cases. Never change an upstream default
   or drop an argument silently.
5. Put shared cases in `port/cases/<id>.jsonl` under their assigned owner. Run
   the oracle on the exact same requests as Rust; the goldens worker compares
   complete identities/results/errors and checks missing/extra records. Preserve
   integer nanometres and exact comparison. No status changes follow from merely
   registering a shim.
6. Re-run `build.sh`. Pure kimath units use the same build command. If linking
   needs `common` or other application libraries, extend the pinned source
   closure and actual CMake targets explicitly and remeasure CI feasibility.
   Do not substitute fake reference functions to make the link succeed.

`oracle-dispatch-test` uses synthetic callbacks solely in a separate test
executable to verify deferred errors, ordinary failures and duplicate registration.
They are never registered in `kicad-oracle`.

## Provenance and FP audit

All pinned library objects, third-party reference code and overlay objects compile
with `-fno-fast-math -ffp-contract=off`; the executable link gets the same flags.
`oracle-audit` runs before compilation on every build, including warm builds.
It hashes every exported source/header/CMake file against the Git-export manifest,
rejects extra source files in the closure, inspects every compile database entry
and all generated build/link commands, and requires the actual `libkimath.a` link.
Conflicting flags are rejected even when later overridden. The guard rejects
`-Ofast`, fast/unsafe/associative/reciprocal/finite-only math, contraction enabled,
unsafe FP models, LTO, hidden response files and compiler escape options.

`oracle-provenance.json` is embedded in the binary (`--provenance`) and records the
full pin, reference manifest digest/count/path, overlay revision/dirty-at-export
state and file hashes, compiler versions, exact compile/build/link commands,
wxWidgets version and generated configuration hash. `source-manifest.json` beside
the reference contains each source file's SHA-256. `binary.sha256` records the
built executable separately to avoid a circular embedded binary hash.

System wxWidgets, C/C++ runtimes and libm are distribution libraries: their own
build flags are not controlled here. No KiCad reference library is taken from an
installed package or existing development build. Archive and manifest hashes are
integrity checks tied to `prepare.py`'s Git export, not signatures for untrusted
third-party archives. Retain provenance plus the manifest with CI results.

All new code is GPL-3.0-or-later. Linked/header dependencies are GPL-compatible:
KiCad core/kimath (GPL-2.0-or-later or GPL-3.0-or-later as marked per file), KiCad
R-tree (GPL-3.0-or-later), Clipper2 and Boost (BSL-1.0), nlohmann/json (MIT), and
wxWidgets (wxWindows Library Licence, LGPL with linking exception). The compiler
runtime uses the GCC Runtime Library Exception; the system C library/libm are LGPL.
Build tools are not shipped in the oracle. No additional vendored dependency is added.

The structural models were read at solidcore
`7a588fa284765a83823293ae20b84da8356ab479` (SolidCraft trailer
`93d5492ef295a1124664d36e967f58a81a76d7d2`): `AGENTS.md`,
`plan/units/geom.matrix.toml`, `crates/goldens/src/case.rs`, the `geom_matrix`
runner, `crates/goldens/README.md`, and `harness/plan/progress.py`.
That pin no longer contains a C++ difftest/shim builder; its current policy uses
Rust goldens and the C++ as a reading reference. This oracle follows KiCad's
binding exact C++ comparison plan, not solidcore's later tolerance/policy changes.
