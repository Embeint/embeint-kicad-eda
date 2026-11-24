<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Provider placement equivalence and execution-profile hardening

Unit: `libs.kimath.include.math.application.pure.p001`. This follow-up does not
sign the pending object context review or authorize a golden.

## Observed layout difference

The full image gate at `1ca2db6938c7888c97cbb9be9516eb421cec5dfd`, Actions run
45360270172, verified the installed closure and the actual object-to-linked-byte
proof. Its provider inventory differed only for the hot `oracle::REGISTRY::Add`
body. The retained reviewed `0bac7567…` object places it at `[2176,2859)`;
the captured current object places it at `[2192,2875)`.

Both 683-byte bodies hash to
`08e82d20644aa7c01829527ec2e2c8780cf6c6c7b46eb459ad5ee5e16fbc7347`.
All 17 symbolic relocation descriptors match, including the four references to
the unchanged cold fragments. Only the range in the digest preimage differs:
the historical digest is `957958ce09109e7b4d0e868fbb49b13634ff920c0e38774e924d2d11fee3274a`,
while the current placement gives `1a1c909f5dc39540a7a7f6f60b2e89f44caa1c13e5e3479e382540aac7683c3c`.

`provider_layout.py` reconstructs the historical digest preimage from these
measured facts, checks it against the unchanged original lock, and recognizes
only a same-sized range change. Every code byte, section name and symbolic
relocation must remain identical. The ordinary link proof still reconstructs
every relocation and checks its target and actual linked bytes. The report
retains both ranges and digests; no provider inventory entry is omitted.

The committed preimage was produced by the tool with the captured current
canonical facts and `--historical-range 2176:2859`. This reconstruction succeeds
only because its canonical JSON hashes to the existing historical digest.
Neither original measured lock nor generated Rust changes.

## Execution-profile review fixes

The separately derived ELF execution extension retains its measured bytes and
pending status. Recursive validation now rejects floats or booleans in integer
fields, unknown nested fields and missing import inventories. The version-table
parser rejects truncated or overlong auxiliary chains instead of accepting an
early terminator, and bounds version strings inside `.dynstr`.

On macOS arm64/Python 3.13, the captured native oracle passes all ten execution
checks after these changes. All provider digests match the original inventory
after the single proven placement equivalence. These are read-only artifact
checks, not execution of the Linux binary or final-head image qualification.

The artifact uploader now includes hidden files: the prior capture inventory
listed hidden source files that upload-artifact's default omitted. The next
capture must verify its entire inventory after download. Context review and a
full gate plus live comparison on the same oracle remain required.
