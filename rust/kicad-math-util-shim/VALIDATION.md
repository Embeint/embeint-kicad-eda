<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation evidence

Implementation base: `eb4cd4724ff6f5fa2119c86248c1a4beb4847451` on official
`origin/rust`; native source pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`.

Local validation on macOS arm64, rustc/cargo 1.96.1, capped at `-j2`:
20 tests across kicad-math and the shim passed in both debug and release.
`cargo fmt --all --check`, scoped clippy `--all-targets -- -D warnings`, locked
wasm32 `cargo check -p kicad-math`, `validate_units.py`, deterministic generator
check and `git diff --check` passed. Debug and release binaries each passed 411
refusals, 7 explicit host deferrals and 418 successful continuation calls. All
10,656 arithmetic reply lines were byte-identical between Rust debug/release:
SHA-256 `db9fe1117b870099b21d457e108643017a91a819cf5eec665fcd3bf637b688de`.
These local checks test the implementation tree committed with this document;
a subsequent evidence entry will bind reruns to a full commit SHA.
No local C++ result is claimed: this workstation has no wx-config; the audited
oracle intentionally builds on Linux. No Windows/WSL process or full KiCad build
was started.

Hosted Linux live arithmetic comparison and native controls are pending on this
PR's head. Registration uses the existing `rust-port.yml`/partial-unit path.
Expected fixture sizes (not claimed runtime passes): 10,656 real arithmetic
results, 411 refusal controls, 7 explicit logging transport deferrals and 418
successful post-control continuations. The script also supports debug/release
comparison when those binaries are built; its availability is not execution evidence.

The unit stays partial even if every arithmetic comparison succeeds. wx logging,
MSVC/fallback profiles and downstream vector/header wiring remain unqualified.
There is no golden. See AUDIT.md for the per-symbol evidence/gap table.
