<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Reject conflicting runtime relocation writes

Independent closeout review at `e819af8edf1b62df8e8f6b5d9a32f2df3a3d592d`
found an execution-proof gap. The ELF profile checked relocation types and writable
destinations, but did not reject two loader writes to the same address. The PLT
reader returned the first matching entry instead of proving uniqueness.

On the retained genuine oracle
`51b6e108774fa7b4b990aa8d7b23ff0ca45be3e6aa76f62d287c2141d4f0a585`,
replacing `.rela.plt[150]` at file offset `0xe1a0` with little-endian
`<QQq>(0x19cb88, 0x7b00000007, 0)` produces mutant
`c151b6dd68294bc8d452c5c36202a013159257fb0961a50321c9fb45e3e8fc97`.
A later sincos relocation then targets strlen's GOT slot. The previous structural
checks and link inventories passed despite the conflicting loader write.

The verifier now gathers write intervals across both `.rela.dyn` and `.rela.plt`
and rejects every overlap, including exact duplicates. COPY relocations use the
full dynamic-symbol size; other supported profile types write eight bytes.
Destination bounds cover that whole extent. The independent PLT reader likewise
requires one unique full-slot JUMP_SLOT write and rejects any conflicting write
from either table, including a partial overlap or a larger COPY range.

Read-only rechecks on macOS/Python3.13 reproduce the exact mutant hash. The genuine
oracle still passes all ten ELF checks and all four locked link inventories.
The mutant fails the relocation row, direct strlen PLT lookup and complete link
proof. No mutant or downloaded executable was run. Binary-free tests cover exact,
partial, cross-table and COPY overlaps, adjacent valid writes and malformed sizes.
The original measured locks and actual golden bytes are unchanged. Native
current-head gate verification must rerun; earlier generation evidence retains
its original source revision.

The same review found that global `compile_commands.json` ignore rules omitted a
captured file from the committed validation bundle. Its actual artifact bytes are
now explicitly tracked, matching the already recorded checksum. A clean-checkout
test checks every bundle member, and the parent also checks every member directly
from Git objects after committing. Existing native evidence is not rewritten.
