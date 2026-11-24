<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation of 992f07f9b73e39c00a2df9a43477d8b8d51f5afb (integration merge)

The topic merged with official `rust` `9261e1705d5cd0d53abb2964cfbd0a3b461f4d42`
(kicad-math vector crate, PR 54). Merge commit tree
`6d0bb4d12c74e860be6883eec2eff3d7854ba725`; previous head
`9aa7e68054cdbb308c06818dc54bda75faf6ddaa` (tree
`24c41fdb52f59b9432820c6e9da39b81a5a69839`). Local WSL Ubuntu (Linux x86-64, GCC 15.2,
Rust 1.99.1), 2026-11-23. No rtree node source, case, golden or evidence byte changed in
the merge: the run re-validates the unchanged unit on the new base. Earlier validation
directories are historical and unchanged.

## Integration

The only conflict was `rust/Cargo.toml` `members`, where each side had appended its own
crates; the union keeps `kicad-math` in its official position, followed by
`kicad-rtree-node` and `kicad-rtree-node-shim`. `Cargo.lock` merged cleanly. Proofs:

- the old->new tree diff is exactly PR 54's seven files; the five `rust/kicad-math`
  blobs equal the official ones; the `Cargo.lock` hunks equal PR 54's;
- the official `rust`->new diff equals the topic's own changes: every file is
  byte-identical to the old head except the member line, and the `Cargo.lock` hunks are
  exactly the topic's two rtree packages;
- `cargo metadata --locked` resolves all 15 workspace members, and an unlocked offline
  resolution leaves `Cargo.lock` byte-identical (`scripts/lockcheck.sh`).

## Results

`scripts/final.sh` on a clean LF export with a freshly built, audited oracle:

| Check | Result |
| --- | --- |
| Oracle build and its 8 ctest checks | pass |
| fmt, clippy `-D warnings`, workspace tests | pass; 182 tests passed, 0 failed |
| wasm32-unknown-unknown portable crates (now with `kicad-math`) | pass |
| Policy tests, unit records, generator freshness | pass |
| Live `check --partial` | `green`, mode `partial`: 1512 compared, 0 deferred, golden matched |
| Strict phase 0 gate | COMPLETE, 3/3 live green pilots; one live partial unit |
| Refusal fixtures | 116/116 identical |
| ASan+UBSan adapter replay | 1512 cases and 116 refusals byte-identical, no diagnostics |

`check_goldens_policy.py --base` is valid against `9261e170...`, `7fe3bd2b...` and
`9aa7e680...`.
