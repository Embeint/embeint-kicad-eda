<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MATRIX3x3<double> NaN operand priority: derivation, checks and gate

`rust/kicad-math/src/generated.rs` evaluates the seven `MATRIX3x3<double>` arithmetic sites
of the pinned oracle as straight-line calls `op(dst, src)` on the explicit SSE2 NaN rules in
`rust/kicad-math/src/sse2.rs`. Pinned derivation identities: unit source
`8ef0c0e846ba8f190080ddf46c77373dcc0e4608`, upstream pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`, oracle `kicad-oracle` sha256
`0bac756736379961a9c3fa34ae2d549615231561650d6d36af6b05a403c1b4db` (build ID
`c8e721185408a24ddcd902bc5d7f9b107afa654f`), GCC `13.3.0-6ubuntu2~24.04.1`, `-O3
-fno-fast-math -ffp-contract=off`, candidate image
`sha256:8b70857a8ca585f2a9893a815b5afdd64b9fff9693257993bf02623f4f4a3597` (unqualified),
`generated.rs` sha256 `d24ff0f24a1fae55cc8e9eb85fa783282030083802d58eb0e332be331c6935af`.

## Two different checks

| | Stable graph checker | Machine eligibility (full gate) |
| --- | --- | --- |
| Tool | `check_generated.py` | `gate/gate.py verify --phase full`, then `gate/emit.py accept` |
| Needs | the repository only | a fresh oracle build, its compiler inputs, objdump, Docker |
| Proves | `generated.rs` is exactly the reviewed graphs in `trees/`, equal to the C++ modulo commutativity of `+`/`*`; the 136 witnesses reproduce; the 7 other nodes are output-equivalent | the oracle that is compared against is the one the graphs were extracted from (identities, compiler inputs, FP environment), how inputs reach and outputs leave each site (context), and a fresh extraction equal to the locked graphs; emission byte-identical to `generated.rs` |
| Status | runs in `rust-kimath-nan.yml` (`graph-check`) | **NOT RUN**: never passed anywhere; `frozen-reference` job (manual) |

The stable checker never establishes eligibility. A live difftest result for this unit
counts only for an oracle that passed the full gate.

## Files

| File | Content |
| --- | --- |
| `trees/<site>.trees.json` | Reviewed operation graph of each site, extracted from the pinned oracle disassembly: operation kinds, ordered dst/src operands, constants, output mapping. |
| `sites.json` | Site to generated function and public API, input order, outputs, the nodes proven output-equivalent, each trees file's SHA-256. |
| `check_generated.py` | The stable graph checker (below). |
| `gate/` | The generator gate, emitter and reviewed locks (below). |

## `check_generated.py`

`python -B port/tools/kimath_nan/check_generated.py` (numpy optional; with it about 30 s):

1. `generated.rs` has the reviewed SHA-256, and its parsed code is exactly the graphs in `trees/`.
2. Finite proof, commutativity only: every output graph is the C++ expression of
   `libs/kimath/include/math/matrix3x3.h` up to swapping the children of `+` and `*`
   (`Inverse` as `result * ( 1.0 / Determinant() )`; `operator*( S, M )` as `aMatrix *
   aScalar`). IEEE 754 addition and multiplication are bit-commutative for all non-NaN
   operands, so every non-NaN result is the source's.
3. The 136 witnesses in `rust/kicad-math/tests/data/nan_priority_witnesses.txt` cover exactly the
   observable nodes; each gives its genuine oracle output on the graph, and swapping only its
   node's priority gives the recorded, different output.
4. The seven nodes without a witness change no output when their priority is swapped:
   exhaustive over two leaf domains of 4^9 inputs each (each leaf a NaN with its own payload,
   a signed zero, an infinity or a finite value), with a positive control that the same
   domains detect every observable node's swap.

Negative controls run privately (each fails the script): a swapped NaN priority, a swapped
subtraction with consistently re-hashed trees, an equivalence claim for an observable node,
a changed witness bit.

### The seven output-equivalent nodes

- Determinant t3/t4/t5/t6 (nodes 4, 5, 9, 10): a swap matters only with two NaN children.
  Then m22/m12/m21/m11 is NaN, which makes d1 and p1 NaN; p1 wins the subtraction into s and
  s wins the final add, masking the changed payload.
- Determinant d3 (node 11): a swap changes its result only if t5 and t6 are different NaNs.
  If s is not NaN, every leaf feeding t5/t6 is not NaN, so their NaNs can only be the default
  NaN of an invalid operation. Otherwise s is NaN and masks d3.
- Inverse reciprocal `1.0 / det` (node 14): 1.0 is never NaN. Division operands are never
  swapped.
- Inverse r00 (node 15): when cof00 is NaN it wins through the determinant, so the reciprocal
  is exactly quiet(cof00) and the final multiplication sees identical quieted bits either way.
  Otherwise at most one multiply operand is NaN.

## `gate/`

The private generator gate, reviewed by Sol6.1 and the parent, brought in for hosted
validation. `gate.py` (schema, eligibility, context, scope, arithmetic), `symex.py` (bounded
symbolic executor of the arithmetic windows), `graph.py` (normalized graph fingerprint and
finite proof), `srcexpr.py` (source expressions from the exact header), `listing.py`,
`x86strict.py`, `elfbin.py`, `lockschema.py` (recursive typed lock schema), `contract.py`
(canonical site scope and generation contract), `handoff.py` (gate to emitter freshness
check), `emit.py` (emitter). Standard library only.

Provenance of the sources (byte-exact; none edited by hand here):

- All modules are the reviewed enforcement revision, with the parent's P3 corrections applied
  by the parent's own patcher (`tests/apply_parent_fixes.py`, ZIP sha256 `2198328d…`) to
  exactly reviewed inputs (`contract.py` `fb0bff99…`, `lockschema.py` `ebc90cb1…`,
  `handoff.py` `8c1ea3c7…`, `gate.py` `3274dba8…`). The corrections are recursive exact numeric
  types in the lock schema and canonical comparison, and the self-inventory bound to
  (category, check) pairs.
- One further fix in `gate.py` (sha256 `7468a6b3…`; overlay output `afed006a…`). The P3
  `lockschema.obj()` returns optional fields as a typed map, but `gate.schema()` still
  subtracted them as a set, so every real gate run raised `TypeError` before its first check
  (the parent's tests use a fixture, not `gate.schema()`). The fix subtracts `set(opt)` and
  validates any present optional top-level field into the existing row. Both maps are empty
  today, so the 30 schema rows and accepted locks are unchanged.
- `tests/test_parent_fixes.py` is the parent's 9 tests, verbatim. `tests/gate_inventory.original.py`
  is the reviewed `check_inventory` extracted unmodified. `tests/gate_inventory.py` is the
  current one with the four globals it reads. Run them with
  `PYTHONPATH=port/tools/kimath_nan/gate python -B -m unittest discover -s port/tools/kimath_nan/gate/tests -p 'test_*.py'`.
- `locks/eligibility.lock.json` (`011613b6…`) and `locks/graphs.lock.json` (`2e972c59…`) are the
  reviewed locks, unchanged. They bind the binary above, its build inputs and a manual context
  review of that exact binary (`context_review`; its citations name the private review
  records by path and SHA-256, which are not in the repository). Its `static_substitutes`
  name retained private listings and serve only `--phase static`, which therefore cannot run
  from the repository.
- `generated.rs`'s header names the prototype scripts it came from; `emit.py` reproduces those
  header bytes exactly (`generated_rs_sha256` in the graph lock).

Private evidence, not committed: with the overlay, the original 156 static controls still all
behave as expected, and the static gate is unchanged. It is `STATIC-PARTIAL`, never a pass:
schema 30, eligibility 41 plus 5 NOT RUN, context 41, scope 5, arithmetic 44. A callee
`padding` of `0.0` and a site consumer `count` of `3.0` each fail the real gate's schema stage.

## Object-bound mode (source checkpoint, pending review)

`gate/objgate.py` binds the code the unit executes rather than the whole linked oracle: the exact
adapter object, its compiler inputs, flags and toolchain closure, and a link proof
(`gate/linkproof.py`) that every reachable section and selected weak/COMDAT body in the linked
oracle is that compiler output after independently verified relocations. It translates the
reviewed context and graph checks into the current layout and requires the reviewed ordered
graphs. It is not wired into any workflow and cannot pass: its object context review is
PENDING with no reviewer. Modules, measured results, commands and gaps are in
`gate/OBJECT_MODE.md`.

## Frozen per-unit reference (workflow `rust-kimath-nan.yml`, job `frozen-reference`)

Manual dispatch with `frozen_reference: true`:

1. Builds the pinned measurement image twice with the reference tools at `33535308…`
   (`qualify.sh --rebuild-check`, as rust-port.yml's resistor migration does).
2. Seals the exact unit source `8ef0c0e…`, from the retained bundle when its URL and SHA-256
   are given, otherwise from a fresh Linux export.
3. Builds the oracle from that snapshot inside the image at the locked `/work/kicad-port`
   paths and writes its provenance.
4. Runs `gate.py verify --phase full` against the reviewed locks with the fresh binary, its
   compiler inputs, compile database, provenance and source manifest. It then runs
   `emit.py accept` and compares the emitted file with `generated.rs` byte for byte.

It proves the machine and site context binding only if every identity reproduces. It
publishes nothing and blesses nothing, and a fingerprint or identity change is a FAIL that
needs a new review, never an automatic rebind.

## Hard blockers

- The full gate has never passed. Each of these is expected to fail closed until resolved
  by review, not by changing the locks or guards:
  - **Bit reproducibility.** It is not shown that a rebuild reproduces oracle `0bac7567…`;
    the binary embeds its revision, paths and compile database.
  - **Source manifest.** The bound manifest (`26ee1995…`) was exported on Windows (it records
    `export_platform` and the Git version), so only the retained bundle can reproduce it.
  - **Image identity.** The lock names image `8b70857a…`; a rebuilt image may have another ID.
  - **objdump.** The host objdump (binutils 2.42 on the runner) differs from the reviewed
    listings' 2.46.
- The context review is a manual review of `0bac7567…`. Any other binary needs a new review.
- The candidate image is unqualified.
- No golden may be blessed for this unit until a full gate pass and a green live difftest are
  recorded for the same oracle.
- Other units that inline `MATRIX3x3<double>` arithmetic, and hash or Bezier code that reuses
  `sse2`, need their own measured sites.
