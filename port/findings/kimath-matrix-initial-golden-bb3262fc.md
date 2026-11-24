<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Initial matrix/wide-integer baseline after exact eligible comparison

Unit: `libs.kimath.include.math.application.pure.p001`.

The strict rust-port gate failed because its math golden did not exist. The
implementation now agrees on all retained requests, including the 3,501 mixed
NaN cases and ten CPU-dispatch-sensitive angles. Native run 45380617555 at
`bb3262fc08f7d03303a22846bfcea2b54577c29a` first passed the full object/context
gate and exact emission in the locked image, then obtained 7,460 exact live
comparisons in both debug and release and successful refusal suites. It used
the same eligible oracle and debug Rust binaries for live propose/bless/check.

The new golden and immutable evidence are the actual generated artifact files.
All 7,460 entries are additions against official rust's empty unit baseline;
the corpus SHA-256 remains `c82021245968c5f9cdbf23b2c1b681257d4ba68836ad1d884b41bb77dc6ac287`.
Golden SHA-256 is `5b3e42d65eb6b6126acbbf5d45cfbdcc27d461c25558179a84aa0b1c5dd4525b`.
The approval pins the complete output bytes, case identities and order, and
rechecks live binary/provenance bindings. It grants no permission to replace a
future baseline or accept mismatches.

The unit is partial for the documented scalar/template/stream/reference scope.
Other locales, FP modes, compilers and second-host qualification remain outside
this evidence. Previous findings retain their historical revisions and failures.
No algorithm, valid input, tolerance, original finding or existing golden/evidence
is changed by this import.

Detailed records: `rust/kicad-math-shim/validation/2026-11-24-bb3262fc/README.md`.
