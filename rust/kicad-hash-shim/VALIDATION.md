<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation: libs.kimath.include.application.pure.p001

## Pending: per-site NaN order and runtime grouping (static and offline only)

This revision changes the two behaviours behind the 41 failures below. **No live oracle has run
on it.** What was checked, on retained genuine artifacts only:

| Check | Result |
| --- | --- |
| Derivation | `BEZIER<double>` operand order read from the candidate oracle's adapter object (sha256 `a6b793f5...`, oracle `5e0a8c36...`, GCC 13.3 candidate image); libstdc++ grouping path read from `libstdc++.so.6.0.33` (`1fd75fe7...`) and `.6.0.35` (`5bb0d213...`) |
| Independent Python model vs retained candidate replies | 600/600 `BEZIER<double>` calls; the old left-operand model reproduces exactly the 38 failures |
| Patched shim (native Windows x86-64) vs retained replies | 2641/2641 candidate and 2641/2641 GCC 15 corpus replies; 10884/10884 adversarial and 3/3 minimal GCC 15 replies; debug and release byte-identical |
| Refusals | `check_guards.py` 441/441 (debug, release, shim side only); fixtures 15/15 and 325/325 |
| wasm32 (Node 22, WSL) | every corpus reply reproduces the native digest `8567687a05ee7d4d` |
| Supplementary sensitive requests (1983) | Rust equals the Python model on all; not yet run against an oracle |
| `check_reference_bindings.py` | bound for the candidate object with either measured runtime; refuses the GCC 15 binary, an unrelated adapter object, an unmeasured runtime (even one whose grouping path passes the structural diagnostic) and a non-libstdc++ library |

Not run: any live oracle comparison of this revision, `check_goldens_policy.py`, the Linux
workspace build. `hosted_validation.sh` can measure a freshly audited oracle; any candidate
image it runs in remains subject to separate reference qualification.

The ordinary gate at `ca40b6420a` ([run 45343887968](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45343887968))
stopped at `stale evidence cases_sha256` before building the oracle. The separate read-only
`rust-header-hash-live.yml` workflow now builds the pinned oracle on Ubuntu 24.04, checks its
actual runtime and site bindings, and runs all live/profile/guard/wasm checks. It produces
diagnostic artifacts only: it cannot replace the ordinary gate or bless a golden. An unknown
runtime or compiled-site drift still fails. No successful fresh run is claimed here.

Fresh [run 45345332013](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45345332013)
at `bf85dbbc25` passed both 2,641-case live comparisons, 441 guards and 1,983
sensitive requests per profile, actual wasm and all eight oracle CTests. All 78
compilations passed the strict FP audit. The adapter object was byte-identical to
the measured `a6b793f5...` object, and linked libstdc++ matched its measured hash.
The binding guard now requires that full object identity as well as its site
signature; matching arithmetic alone cannot qualify changed non-FP effects.

The workflow's explicit manual `generate_golden` option can now run live
propose/bless/check after those checks succeed. Its one-time approval guard
reconstructs every proposed line from the real old golden and refuses any bytes
other than the independently audited output SHA-256
`1ef5a58c552ee2898575b7563807e4a8cf5325eae78d226244e7ca58623e08d9`.
Ordinary PR runs never generate a golden; the workflow never publishes one.
Generation and commit of matching evidence remain pending.

## Previous status: 2026-11-23, clean commit 9bf572d4 (native WSL x86-64)

**Not green and no corrected golden.** 2600 of 2641 corpus cases agree exactly with
the live oracle, with no deferral. 41 fail on two open questions that need a parent
decision. The committed golden is still the superseded 8c54103 baseline
([record](../../port/findings/kimath-header-hash-baseline-8c54103-superseded.md)),
so `check` and the base-relative golden policy fail, as they must until a reviewed
reblessing. Files: [`validation/2026-11-23-9bf572d4`](validation/2026-11-23-9bf572d4).

### Identity

- Tested commit `9bf572d4a34645ec4885f4005aaa7413096b789f` (tree `cf4dd1083da74101b57938d83fb68a61ab949955`).
  It is PR #51 with official `rust` `2f01eaa1d9d47d051c5e562984df56f94b103fe5` merged
  (includes #52, #53 and #54) and the recovered edits of `0bd49991` reviewed and corrected.
- Platform (`platform.txt`): WSL2 Linux 6.18 x86-64, Ubuntu 26.04.1, GCC 15.2.0, distribution
  `libstdc++6 16-20260322`, Rust 1.99.1, Node 22.22.1. Native; no emulation or container.
- Source: `git archive` of the clean `HEAD` with process-local `core.autocrlf=false` and
  `core.eol=lf` (`export-full.ps1`). The pinned reference has 0 files with CR (archive SHA-256
  `2925d325a428b8ec2c6663c514327f11473b75869e6fd5eaeb37e2893117dd1d`, 915 files,
  `overlay_dirty_at_export` false).
- Oracle: `port/oracle/build.sh` (own work root, shared build lock, `-j8`), all 8 oracle ctests
  passed. The FP and source audit ran as part of the build. `oracle-provenance.json` is the
  binary's own `--provenance`.
- Executable SHA-256 (`sha256.txt`, the binaries themselves, not wrappers): `kicad-oracle`
  `705f856d…`, debug `kicad-hash-shim` `8046a1db…`, release `0e119fdc…`, `kicad-difftest`
  `00078346…`, `wasm_parity.wasm` `d15cd107…`. Corpus SHA-256 `5464c930…`.

An earlier attempt on the same commit exported the pinned reference with CRLF line
endings (archive `7863f078…`, Windows `core.eol`). That attempt was discarded and is
not evidence. Its comparison counts were the same.

### Results

| Check | Result |
| --- | --- |
| `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace` | clean; clean; 45 suites, 218 tests passed, 0 failed |
| Live comparison, debug and release shims (`kicad-difftest compare`) | 2641 cases: 2600 compared exactly, 0 deferred, 41 failed (`compare-debug.json`, `failing-cases.txt`, `failures.txt`); debug and release replies byte-identical |
| Guards (`check_guards.py`), debug and release | 441 requests (incl. the 15 MMH3 unwritten-read refusals and 325 undefined `BEZIER<int>` requests), both programs give every expected reply, 0 failures |
| MMH3 audit of all 1315 original requests (`port/findings/kimath-header-hash-mmh3-audit/audit.json`) | 979 unchanged, 321 `blocks-only` (invented `00` -> `??`), 15 refused, 0 other |
| wasm (`check_goldens_policy.py --wasm`; `wasm_parity` in Node) | every portable crate builds for `wasm32-unknown-unknown`; all 2641 replies computed in wasm equal the native digest `099fd02b786fb398`, NaN bits included |
| Generator | regenerates the corpus and both fixtures byte for byte |
| `validate_units.py` | all unit records and source accounting valid |
| Other registered units on the same oracle | R-tree node (partial) 1525/1525, MD5 5973, resistors 1050, IEC 60664 14190: all compared exactly, green against their goldens (`other-units.txt`) |
| `check_goldens_policy.py --ci --require-phase0` | fails at this unit (`hash128-locale-one-two-zero`, the first failing case in corpus order); sorted before the other units, so the policy stops there (`ci-phase0.log`) |
| `check --partial` against the committed golden | fails (stale 8c54103 golden) |
| `check_goldens_policy.py --base origin/rust` (Windows checkout) | fails: `stale evidence cases_sha256` (the 8c54103 evidence no longer matches the corrected corpus) |

The 41 failing cases:

- 38 `BEZIER<double>` cases where two distinct NaN payloads meet in one operation
  ([record](../../port/findings/kimath-header-hash-bezier-two-nan-payloads.md);
  `nan-fit.py` and `nan-fit.txt` reproduce the fitting on this run's oracle replies);
- 3 `hash128-locale-*` cases with a zero group after a positive group, which the host's
  libstdc++ 16 runtime handles differently from the GCC 15 template and the standard
  ([record](../../port/findings/kimath-header-hash-tostring-locale.md);
  `grouping-probe.cpp` and `grouping-probe.txt`).

Corpus (`port/cases/<unit>.jsonl`, 2641): the 1300 original non-refused requests
(byte-identical), the first review's 44 additions and the second review's 1297:
`bezier` 1280 (`f64`: 15 constructor, 196 `PointAt`, 195 `Split`, 209 `SubCurve`; `i32`: 10,
240, 151, 264), `bezier_poly` 13 and 4 long-grouping `hash128-locale` cases.

## Superseded: 2026-11-23 run (8c54103a7d)

Historical, superseded. Run 2026-11-23 on implementation commit `7cc11f2e83528fa3d655aa67db528f15fe8c1183`
(base `rust` `535a17e39cc77452ba3377a5afb4170c14e9ab81`). Files are in
[`validation/2026-11-23-7cc11f2e`](validation/2026-11-23-7cc11f2e).

### Platform and its limits

- Oracle: `port/oracle/prepare.py` on the clean commit (`overlay_revision`
  `7cc11f2e83…`, `overlay_dirty_at_export` false, 915 pinned files), then
  `ORACLE_JOBS=1 bash port/oracle/build.sh` in a disposable `ubuntu:24.04`
  `linux/amd64` container (GCC 13.3.0, Ubuntu 24.04.5) on an arm64 Mac through
  QEMU user emulation. The audit verified 76 compilations with the strict FP
  flags; all 8 oracle ctests passed. `kicad-oracle` sha256
  `6e65e38510f5f3ca34f08b9ebd970afbbe4e0edfef2bef389ca4dc3f2fff99e3`; full
  provenance in `oracle-provenance.json`.
- Rust: `rustc` itself segfaults under that emulation, so the shim was
  cross-built on the Mac with Rust 1.99.1 for `x86_64-unknown-linux-musl`
  (static, linked by `rust-lld`) and executed inside the same container. Debug
  sha256 `15377b90…`, release `2874a24e…` (`container.log`).
- `kicad-difftest` ran natively on the Mac and invoked both programs through
  the `docker exec -i` wrapper scripts stored here; the executable hashes in the
  reports and baseline evidence are those wrappers' hashes
  (`b723f41c…` oracle, `44d2a535…` Rust).
- Emulated x86-64 is not a native host. The comparison is all integer and byte
  work (no floating-point arithmetic beyond bit-pattern hashing), and the hosted
  CI run of the same commit on native `ubuntu-24.04` x86-64 compared all 1315
  cases live before stopping only at the then-missing golden (run 45085161090).

### Results

| Check | Command | Result |
| --- | --- | --- |
| Live comparison, x86-64 debug shim | `kicad-difftest compare --unit … --oracle oracle-wrapper.sh --rust rust-wrapper.sh` | 1315 cases, 0 failures, 0 deferred (`compare-rust.json`) |
| Live comparison, x86-64 release shim | same, release binary | identical Rust output sha256 `e94d5e88…` (`compare-rust-release.json`) |
| Live comparison, native arm64 macOS shim | same, host debug binary | identical Rust output sha256 (`compare-native-arm64.json`) |
| Baseline golden | `kicad-difftest propose` then `bless` with this provenance | `port/goldens/<unit>.jsonl` (oracle output sha256 `e1b95812…`), evidence `port/goldens/evidence/<unit>-baseline.json` |
| Golden check | `kicad-difftest check` | 1315 cases green (`check-x86_64-debug.json`) |
| Golden policy | `python3 port/tools/check_goldens_policy.py --base origin/rust` | evidence valid |
| Malformed requests | `python3 rust/kicad-hash-shim/check_guards.py ORACLE RUST` in the container | 71 requests, both programs give the expected complete reply, 0 failures |
| Workspace | `cargo +1.99.1 fmt --all --check`; `clippy --workspace --all-targets -- -D warnings`; `test --workspace` | clean; all tests pass except 4 `embeint-mcp` socket tests that fail on this Mac because the worktree path exceeds `SUN_LEN` (unrelated, they pass in hosted CI) |
| Portable wasm | `python3 port/tools/check_goldens_policy.py --wasm` | `kicad-hash` builds for `wasm32-unknown-unknown`; `kicad-hash-shim` is `portable = false` |
| Unit records | `python3 port/tools/validate_units.py` | valid |

Case groups (IDs in `port/cases/<unit>.jsonl`): `std-hash-*` (28), `hash-combine-*` (333)
and `hash-val-*` (323) arities 0..3 including all 144 two-type and 125 three-type
combinations, `hash-combine-each-*` (8), `hash128-*` (21), `mmh3-*` (336: every addData
length 0..49, addDataV1 0..40, 12 stream splits, int slots after partial bytes,
digest/reset continuation, three large streams up to 1 MiB, five `len` wraps, 16
direct private-helper sequences, all 16 `hashTail` lengths, 150 fixed-seed random
sequences), `rotl-*` (214, all 63 counts), `fmix64-*` (13), `md5-valid-*` (9) and
`transform-*` (30).

The `status` field in the reports is the comparison status. The unit remains
`partial` because the five Bezier entries are not implemented (see AUDIT.md);
no case is deferred because no case exercises them.
