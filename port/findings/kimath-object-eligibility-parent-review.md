<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Object-bound matrix eligibility remains pending

Unit: `libs.kimath.include.math.application.pure.p001`. Parent source review of
object-mode checkpoint `46ddaa9a897c2a8e1a27b368c51170399e8b9783` on macOS arm64,
Python 3.13. No context reviewer is recorded and no golden is blessed.

## Fixed during review

- **FP-state writes:** `fp_sweep` treated displacement bytes as prefixes and inspected
  the instruction's final byte instead of its ModRM mode. It missed valid encodings
  such as `67 0f ae 10` and `0f ae 55 66` in an executable section outside `.text`.
  The length decoder now reports actual prefix/ModRM fields, and the scan checks state
  writes regardless of prefixes. Eight encoding controls and state-read/ordinary-code
  controls exercise this. The genuine retained oracle's 1,063,933 executable bytes
  still scan without errors or FP-mode writes.
- **Closure execution:** `--verify-closure` accepted any command returning zero,
  including `true`. It is now a boolean flag invoking only the repository's verifier
  against the installed image's lock/candidate and the exact expected identity.
  Failure and timeout remain failures; an arbitrary command argument is rejected.
- **Schema:** object mode omitted the original recursive schema calls. Those checks
  now run and enter the complete inventory, together with nested object-lock shape,
  type and scope checks. Float/boolean integer substitutions, unknown keys and
  missing emission sites reject.
- **Emission:** a generated-byte hash mismatch previously left the decision as PASS.
  It now makes the run fail and preserves prior output. A valid current-run emission
  replaces output atomically. Writer tests use explicitly synthetic reports and do
  not establish machine eligibility.

All 26 binary-free gate tests passed after these changes. No Rust arithmetic,
generated graph, original reviewed gate module, measured lock, corpus, golden or
unit status changed. These focused checks are not the 22 external-input controls
or a full image gate.

## Blocking execution-mapping objection

`elfbin.Elf` reads section tables and discards most program-header fields.
`linkproof.Linked` reads relocations from named sections. Object mode no longer
binds the complete linked binary, but it does not validate that these section bytes
are the bytes mapped/executed by the kernel and dynamic loader.

On retained genuine oracle SHA-256
`6b6629d886cda51f5471d57a3d4a932c06bc89657f0430a5ff7b71ddfc942c64`, changing only
`e_phnum` at ELF header offset 56 to zero gives SHA-256
`61c539891bcc3f2e448b5ef9db491ca24f0e63c308685f67b578fd5584e08fef`.
All currently inspected identity fields except the deliberately unbound file hash,
section/symbol tables, dynamic relocation facts, DT_NEEDED list and FP-sweep result
remain identical. This is an observed in-memory parser/proof-input probe. The full
object gate was not run locally because provider objects are absent; no mutant
binary was executed. The broader acceptance concern is inferred from the inspected
proof's exclusively section-based reads.

Before review clearance, validate ELF header/program-header bounds, executable/load
mapping, entry/interpreter and loader-consumed dynamic table identities. Include
independent negative controls for absent/changed program headers, changed mappings
and redirected dynamic metadata. Keep the named context review pending until this
is resolved, the pinned closure is re-verified, and the same oracle passes the full
gate and live comparison. Do not weaken the original whole-binary guard or rebind a
lock to hide these failures.

Local evidence: `/Users/karl/.t3/scratch/pr50-object-parent-review-20261124/`.
