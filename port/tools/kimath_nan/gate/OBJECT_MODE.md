<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Object-bound eligibility mode (source checkpoint, pending review)

**Status.** This is a source checkpoint, not reference acceptance:
- no acceptance path is active;
- `rust-kimath-nan.yml` is unchanged and does not run this mode;
- the object lock is derived and unreviewed;
- the object context review is PENDING with no reviewer, so the gate's decision cannot be
  PASS.

It replaces nothing in the reviewed gate. `gate.py`, its modules and both reviewed locks are
byte-identical, and the new code imports them.

## Why

The reviewed gate binds eligibility and context to one whole linked oracle (sha256
`0bac7567…`). Every rebuild changes that binary, because it embeds its revision, paths and
compile database. The adapter object does not change. The PR50 hosted rust-port build (run
45349355098, artifact 14899782401) compiled `kimath_math.cpp.o` to exactly
`97c5f4527c06760a405a5c2f11b99e1ed1524c9b14c8893cbfdc694f580feeb8`, the object of the
reviewed build. It did so on another host, under other paths, at another revision and with 78
compilations, while its oracle (`6b6629d8…`) differs entirely. This mode therefore binds the
code the unit executes.

## Modules

| File | Purpose | Depends on |
| --- | --- | --- |
| `relobj.py` | Strict ELF64 x86-64 relocatable-object reader: sections, symbols, RELA, COMDAT groups | stdlib |
| `x86len.py` | x86-64 length decoder; every boundary matched the reviewed objdump listings of DoubleOps and Inverse | stdlib |
| `linkproof.py` | Link proof (below) | `elfbin`, `relobj`, `x86len` |
| `objgate.py` | Object-mode gate: `derive` (measure the lock once on the reviewed build) and `verify` | `gate`, `contract`, `emit`, `graph`, `srcexpr`, `symex`, `listing`, `elfbin`, `linkproof`, `relobj`, `x86len` |
| `locks/object.lock.json` | Derived object lock (sha256 `8202a8af…`), status `DERIVED-PENDING-REVIEW` | — |
| `object.externals.json` | Input to `derive`: the one external outside every object (`__dso_handle`, address-only) | — |
| `review/object-context-review.PENDING.md` | What the pending review binds and what a reviewer must judge | — |
| `objcontrols.py` | The 22 object-mode controls | `objgate` and the above |
| `objcontrols.inputs.json` | SHA-256 of every external genuine input the controls need | — |
| `tests/test_object_lock.py` | Binary-free checks (run by `graph-check`) | `objgate` |

## What `verify` binds

- **Object:** the exact adapter object (SHA-256 and `.comment`).
- **Source and flags:**
  - its four compiler inputs, as the reviewed Git blobs;
  - its compile command with only the build root normalized;
  - every compilation strict with no forbidden FP/ISA flag, and the adapter compiled exactly
    once; the whole-build compilation count is not bound;
  - the include order, with no earlier directory shadowing the headers; this needs the
    compiler-input tree, and every earlier include directory must exist in it.
- **Toolchain closure:**
  - the exact `image-identity.json` bytes (`c3756860…`), naming input lock `c4d1c9c0…`,
    `g++` 13.3.0, glibc 2.39 and `libstdc++.so.6.0.33`;
  - re-verification of the installed closure on the build machine (`--verify-closure`, i.e.
    `verify_image.py` inside the image);
  - image IDs are not bound: identical rebuilds get different IDs.
- **Linked oracle identity fields:** ELF64 x86-64 executable, baseline ISA, reviewed
  `DT_NEEDED` and `.comment`. The whole-file hash is not bound.
- **Link proof** (`linkproof.py`). From the object's only global entry (`RegisterKimathMath`),
  every section the code reaches is checked exactly against compiler output, transitively and
  across objects. No byte is masked, and no target fact is supplied by the caller.
  - Each section is placed by its symbols in the linked symbol table (locals only within the
    object's own FILE group), or by the address every reference implies (all must agree).
  - Each relocation field is rebuilt from a target whose identity the proof establishes:
    - PC32/PLT32: S + A - P;
    - GOTPCRELX: unrelaxed, with the slot identity checked, or GNU ld's local `mov`→`lea`
      relaxation;
    - R_X86_64_64: an R_X86_64_RELATIVE dynamic relocation with the same addend, or a
      GLOB_DAT/64 relocation naming the import;
    - merged strings and constants: checked by content.
  - A weak/COMDAT body is followed to the copy the linker actually kept. Seven reachable
    bodies come from other objects (`main`, `iec60664`, `md5`, `registry`, `resistors`). Each
    must equal its provider's relocated bytes exactly and is bound by a digest that is
    independent of addresses.
  - Code from other objects outside COMDAT is bound per function. Unrelocated calls to other
    functions of the same section are fixups only after the decoder locates the field and
    the linked target is that exact named function and offset.
  - Imports: exact `.plt.sec`/`.plt.got` stub and JUMP_SLOT/GLOB_DAT identity.
  - The lock binds 33 home sections, 58 units from other objects, 50 imports and one
    external.
- **FP environment:** no MXCSR/x87-control write in any executable byte of the whole linked
  oracle (decoder sweep, plus the objdump scan in the full phase), and no FP-environment
  symbols.
- **Context.** The reviewed context bindings and site windows are translated into the
  current layout through proven units: prologue bindings, frame, 14 callees, the `operator==`
  dispatch target, and the windows and branches. The mechanical checks are re-run: sources
  unwritten, no write through the request pointer, path instruction count. Path digests are
  taken over the OBJECT's bytes, and callee identities come from the link proof. The object
  context review must be signed for this object and link digest; it is PENDING.
- **Arithmetic:** the reviewed ordered graph extraction re-runs at the translated addresses
  and must equal the reviewed graph lock, with finite proofs. Drift fails; there is no
  rebinding.
- **Scope and emission:**
  - the complete site inventory and check inventory;
  - emission only from a full-phase PASS in the same run, and only if the bytes hash to
    `d24ff0f2…`.

## Measured results (private, before this checkpoint)

| Oracle | Phase | Eligibility | Context | Scope | Arithmetic | Decision |
| --- | --- | --- | --- | --- | --- | --- |
| reviewed `0bac7567…` | full (GNU objdump 2.46) | 27 pass, 1 not run (closure re-verify) | 40 pass, 1 fail (review PENDING) | 5 | 44 | FAIL |
| fresh `6b6629d8…` | full | 24 pass, 3 fail, 1 not run | 40 pass, 1 fail (review PENDING) | 5 | 44 | FAIL |

On the fresh build, the 3 eligibility failures are inputs the artifact lacks or that a runner
build cannot provide:
- no toolchain identity (it is a runner build, not the image);
- no include tree for the shadowing check.

Controls (`objcontrols.py`): 22/22 as expected. The 156 controls of the reviewed gate are
unchanged and still all as expected.

## Commands (repository root)

```bash
# binary-free tests (also run by the graph-check job)
PYTHONPATH=port/tools/kimath_nan/gate python -B -m unittest discover -s port/tools/kimath_nan/gate/tests -p 'test_*.py'

# the 22 controls: ROOT must hold exactly the inputs of objcontrols.inputs.json (checked by SHA-256)
python -B port/tools/kimath_nan/gate/objcontrols.py --inputs ROOT --objdump objdump --out objcontrols-report.json

# verify a build (providers: the other kicad-oracle objects plus link-order.txt listing all
# objects, kimath_math.cpp.o at its link position; compiler-inputs: compiler-input-matrix3x3.h,
# -vector2d.h, -vector3.h, -kimath_math.cpp and compile_commands.json)
python -B port/tools/kimath_nan/gate/objgate.py verify --phase full --objdump objdump \
  --oracle kicad-oracle --object kimath_math.cpp.o --providers PROVIDERS --compiler-inputs CI \
  --toolchain-identity image-identity.json --oracle-src KICAD_PORT_ROOT \
  --locks port/tools/kimath_nan/gate/locks --out report.json [--verify-closure "<verify_image.py command>"]

# re-derive the object lock from the reviewed build (reproduces 8202a8af... byte for byte with GNU objdump 2.46)
python -B port/tools/kimath_nan/gate/objgate.py derive --phase full --objdump objdump \
  --oracle REVIEWED/kicad-oracle --object REVIEWED/objects/kimath_math.cpp.o --providers PROVIDERS \
  --compiler-inputs CI --toolchain-identity REVIEWED/image-identity.json \
  --externals port/tools/kimath_nan/gate/object.externals.json --locks port/tools/kimath_nan/gate/locks --out object.lock.json
```

The controls and the derivation were run with GNU objdump 2.46. Other versions may print
listings differently; the listing parser then fails closed.

## Gaps and blockers (stated, not worked around)

- **No reviewer.** The object context review is PENDING, so no run can PASS. It must be
  performed and signed by a named reviewer; this checkpoint does not do that.
- **Missing CI captures.**
  - The image build must supply its `image-identity.json`, the gate must run inside the image
    with `--verify-closure`, and CI must capture the provider objects with the link order and
    the include tree.
  - The current rust-port artifact has only the adapter object.
  - The controls use the reviewed build's provider objects. That is sound, because the proof
    requires exact equality of the linked bodies with those bytes, but it is not CI capture.
- **Static phase.** It needs the retained private listings named in the reviewed lock
  (`static_substitutes`). Those files are not in the repository, and a static report is never
  a PASS. The full phase and `derive --phase full` need only objdump.
- **`.eh_frame` is not link-verified.** The linker rewrites it, so unwind metadata is bound
  only through the object hash.
- **Code from unrelated units runs in this unit.** Selected COMDAT bodies from other objects
  are bound by digest, so a change to how another unit instantiates them fails this mode until
  review.
- **Scope.** GCC 13.3 with the fixed source, pin and flags and the reviewed ordered graph.
  Any other compiler output changes the object hash or the graph and fails.
