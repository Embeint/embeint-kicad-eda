<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Object-mode ELF execution mapping (addendum to the parent review)

Unit: `libs.kimath.include.math.application.pure.p001`. This addendum follows
`kimath-object-eligibility-parent-review.md`. The object context review stays PENDING with no
reviewer, no golden is blessed, and no full gate passes.

## The objection, confirmed on the whole gate

The parent changed only `e_phnum` (ELF header offset 56) of fresh oracle `6b6629d8…` to zero,
giving `61c539891bcc3f2e448b5ef9db491ca24f0e63c308685f67b578fd5584e08fef`. I ran the
unchanged object gate of `a7b0557` in its full phase on that file and on the genuine one:
- retained reviewed provider objects;
- the artifact's compiler inputs;
- GNU objdump 2.46.

Both runs produced the same row results (eligibility 25 pass, 3 fail, 1 not run; context 40/1;
arithmetic 44/0). The mutant was never executed. The gate therefore could not distinguish an
oracle whose program headers do not describe the bytes it proved.

## Change

`port/tools/kimath_nan/gate/elfexec.py` parses the raw ELF independently of `elfbin`. It then
proves, for the current build profile, that the section facts the object gate uses are the
facts the kernel and the dynamic loader use. Unknown or ambiguous layouts fail.

The checks fall into ten rows, all in the object-mode inventory:
- **Header:** the exact profile header, with the program-header table directly after it.
- **Program headers:** the profile's 13 program headers in their order, flags and
  alignment, with congruent, ascending, page-disjoint PT_LOADs.
- **Sections:** every allocated section lies in exactly one PT_LOAD at the same
  offset-to-address relation and with equal permissions, and no unallocated data is mapped.
- **Executable mapping:** exactly one executable mapping, holding only executable sections
  and zero padding.
- **Entry point:** `_start` in `.text`.
- **Special segments:** PT_PHDR, PT_INTERP, the notes, PT_GNU_PROPERTY, PT_GNU_EH_FRAME, a
  non-executable stack and PT_GNU_RELRO each equal their sections, and the interpreter is
  `/lib64/ld-linux-x86-64.so.2`.
- **Dynamic table:** PT_DYNAMIC equals `.dynamic`, and every DT_* value the loader reads names
  exactly the parsed section. This covers:
  - the string and symbol tables;
  - the RELA and JMPREL relocations, with sizes;
  - INIT/FINI, the init/fini arrays, GNU_HASH, VERSYM, VERNEED/VERNEEDNUM and PLTGOT;
  - DT_FLAGS = BIND_NOW and DT_FLAGS_1 = NOW|PIE;
  - RELACOUNT, which counts the leading RELATIVE relocations.
- **Dynamic relocations:** only types 8/1/6/5 in `.rela.dyn` and 7 in `.rela.plt`, at
  writable mapped places. COPY relocations go only to `.bss` or `.data.rel.ro`, and JUMP_SLOT
  only to `.got`.
- **Symbol versions:** a valid verneed chain with matching ELF hashes, and the 50 imports the
  unit uses bind the measured library and version. Two of them are typeinfo objects reached
  through COPY relocations.
- **Extension lock:** complete fields and pending status.

The measured profile is in a separately derived extension lock,
`gate/locks/object-elfexec.lock.json` (sha256 `ad149028…`, from reviewed oracle `0bac7567…`,
status DERIVED-PENDING-REVIEW, no reviewer field). `object.lock.json`, the original reviewed
gate modules and locks, arithmetic, workflows and goldens are unchanged.

## Results

- **Genuine oracles:** reviewed `0bac7567…` and fresh `6b6629d8…` pass all ten rows.
- **Whole integrated gate:**
  - the genuine fresh oracle keeps only its earlier input failures: no toolchain identity, no
    include tree, and the PENDING review;
  - the `e_phnum = 0` mutant now fails 8 ELF execution rows.
- **`elfexec_controls.py`:** 30 of 30 as expected. Both genuine oracles are accepted. These
  mutations fail their named rows:
  - **Program headers:** `e_phnum = 0`, a moved program-header table, PF_X removed from or
    added to a mapping, a remapped file offset, an overlapping mapping.
  - **Sections and code:** a `.text` section header pointing at bytes other than the mapped
    ones, a non-zero byte in executable padding, an entry point inside `.text` or outside the
    executable mapping.
  - **Interpreter and segments:** a changed or redirected interpreter, an executable stack,
    RELRO extended over `.data`, PT_DYNAMIC redirected or removed.
  - **Dynamic table:** a DT_RELA/RELASZ, JMPREL, SYMTAB, STRTAB or NEEDED redirection, an
    added DT_RPATH, DT_FLAGS_1 cleared.
  - **Relocations and versions:** an IRELATIVE relocation, a RELATIVE relocation retargeted
    at read-only data, a changed `memcpy` version, an extension lock with a non-pending status.
- **Object controls:** 21 of 22 as expected, with or without this change. The one miss
  already occurs on unchanged `1ca2db6`: the root's recursive object-lock schema now also
  rejects the synthetic REVIEWED fixture, so that control's "only the review row changes"
  expectation no longer holds. That is stricter, not weaker, but the expectation needs the
  root's decision.
- **Binary-free tests:** 34 of 35 pass. The one failure, `test_fixed_verifier_failure_is_not_a_closure_pass`,
  fails identically on unchanged `1ca2db6` under Windows Python 3.12 only: it expects `/` in
  the verifier path.

## Limits

- **One profile.** This validates the current GNU ld 2.42 / GCC 13.3 PIE profile as
  measured. Another linker layout fails until it is derived and reviewed.
- **Not a loader emulation.** The kernel and glibc behaviour is modelled by the checked
  rules, not executed. Lazy binding is excluded by BIND_NOW, and IFUNC, TLS, RPATH/RUNPATH and
  text relocations are rejected by the profile.
- **Shared libraries are outside this check.** Their identity remains the toolchain-closure
  question (image identity and `--verify-closure`), which is still required.
- **Still blocking:** the context review remains PENDING. The pinned closure must be
  re-verified and the same oracle must pass the full gate and the live comparison before any
  acceptance.
