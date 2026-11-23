<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation: libs.kimath.include.application.pure.p001

## Current status (rework after the Sol 6.1 review of 8c54103a7d)

**No current qualification.** The 2026-11-23 run below is superseded and
must not be used as qualification
([record](../../port/findings/kimath-header-hash-baseline-8c54103-superseded.md)):
its oracle adapter zero-filled `MMH3_HASH::blocks`, so all MMH3 cases compared
an invented initial state and 15 compared undefined reads. The committed golden
is that stale baseline, so CI's golden check is expected to fail until a
reviewed reblessing.

The rework removes the zero-fill. The adapter and the port track written bytes,
report unwritten ones as `??` and refuse undefined reads. The 15 cases are moved,
byte for byte, to `fixtures/` as refusal regressions
([reclassification](../../port/findings/kimath-header-hash-mmh3-unwritten-reads.md)).
It also adds the locale-aware `ToString` and 44 cases (1344 in the corpus).
Checks run so far, none of them Linux qualification:

| Check | Result |
| --- | --- |
| Rust `fmt`, `clippy -D warnings`, tests (Rust 1.99.1, macOS arm64) | see the PR's Tests section |
| All 1315 original requests through the corrected Rust shim | 15 refusals (the review's exact set and first reads); 321 MMH3 replies differ from the old golden only in `blocks`; 979 identical |
| Development smoke adapter: the real `kimath_header_hash.cpp` with the pinned `mmh3_hash.h`, `hash_128.h`, `hash.h`, `transform.h`, `md5_hash.cpp`, `transform.cpp`, built with Apple Clang/libc++ on macOS arm64 and a stub registry, `wx/debug.h` and `config.h` (scratch only) | equal to the Rust shim on every MMH3, `hash128` (including all 28 locale cases), TRANSFORM, MD5, `rotl` and `fmix64` case and on all 97 guards; `std-hash`/`hash-combine`/`hash-val` differ as expected because libc++'s `std::hash` is not libstdc++'s |

Remaining before any corrected golden: build the real oracle natively on Linux
x86-64 (to be assigned by the parent; the emulated Rancher container is no
longer used), run `kicad-difftest compare --partial` on the 1344 cases and
`check_guards.py` with both programs, then `propose`/`bless` with a reason
citing the findings above.

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
