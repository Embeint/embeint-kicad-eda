<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MD5 source and branch audit

Reading reference: `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
Candidate: `libs.kimath.src.md5_hash.application.pure.p001`.
The local finalizer record confirms all 15 definitions, physical span 1..289,
layer 0, pure classification and empty dependency/gap lists. Its raw SHA-256 is
`71c89c90c9df44c2ba2abb81a367c650c951fd058b1f3e7dd4e788b04c3120ee`.
Parent confirmed the identical committed record at PR #39 head
`a7ec77beffd3b2e5d60409423fa28b6c116c6e00`. That exact dependency is merged into
this topic, and only the owned record's port metadata/registration is changed.
`kicad_md5::md5_hash` is the real implementation module, with root re-exports.
Local matching metadata does not establish hosted CI or independent review.

All locations below are in `libs/kimath/src/md5_hash.cpp`. The supporting class
in `libs/kimath/include/md5_hash.h` supplies `IsValid`, `SetValid` and the actual
`Format(false)` default. No unavailable application call or deferred continuation
is introduced. There are no `UNPORTED` callers in this implementation.

| Upstream definition | Rust body and branches | Successful observable operation in the corpus | Remaining limitation |
| --- | --- | --- | --- |
| default constructor, line 30 | `Md5Hash::new` / `Default` / initialization | every `new` snapshot includes defined zero digest and false validity | checked live |
| copy constructor, line 36 | derived `Clone`, every field copied | every split clones a partial context; continuations finalize original/copy | checked live |
| destructor, line 44 | ordinary Rust drop; upstream body empty, no resource/effect to reproduce | every case destroys its real C++ states | no observable destructor result exists |
| assignment, line 48 | derived `Clone::clone_from`, context/digest/validity copied | partial/finalized assignment and independent mutation | checked live; reference self-assignment excluded for overlapping memcpy |
| `Init`, line 58 | `init` replaces all context fields, hash and validity | reset after partial and finalized work; empty finalization after reset | checked live |
| byte `Hash`, line 66 | `hash` validates uint32 length before `update` | empty, single/full/multiple blocks, all selected splits/strides, repeat no-ops | harness bounds one call to 65536 bytes; library accepts uint32 domain |
| integer `Hash`, line 72 | `hash_int` uses i32 little-endian bytes | 15 signed boundaries versus explicit byte overload; mixed continuation | Linux little-endian 32-bit int contract only |
| `Finalize`, line 78 | `finalize` mutates context, then stored digest, then validity | first/repeated finalization, hashing afterward, validity false before finalizing | checked live |
| equality, line 85 | `PartialEq` compares stored bytes only | equal/unequal, unfinished contexts, differing validity, pending updates | checked live |
| inequality, line 91 | default `PartialEq::ne`, logical inverse of byte equality | real C++ `!=` is independently serialized alongside `==` | checked live |
| `Format`, line 97 | `format`, `format_default`; both nibble arms and compact modes | all snapshots include compact, spaced and omitted-default calls; literal RFC strings | checked live; reference nibble offset intentionally preserved |
| `md5_transform`, line 125 | existing `md5::block_api::compress`, all four rounds / 64 steps | RFC strings, all byte values, 55/56/63/64/65, multiblock streams and continuations | compatible crate's compression effects checked live |
| `md5_init`, line 215 | `Default`: full zero buffer/counters and all four constants | new/reset states and their subsequent successful hashes | checked live |
| `md5_update`, line 228 | `update`: empty loop, partial fill, transform at 64, uint64 wrapping add | every two-way split, bytewise updates, copies across block boundaries, 512 MiB low-word carry | full uint64 wrap not practically enumerated |
| `md5_final`, line 245 | both padding branches, all zero fill/length bytes, final compression and LE output; preserves partial count | all remainders 0..63; <56 and >=56 branches; copied/repeated contexts; final-add low-word carry | full uint64 wrap not practically enumerated |

Every reference digest and validity snapshot comes from public methods of an
actually constructed `MD5_HASH`. `Format` is bijective over digest bytes despite
its unusual alphabet, so all digest bits are observed. Private buffer/counter/state
bytes are observed through subsequent real operations, not by exposing layout or
substituting a digest. No undefined/uninitialized object state is tested.

The `DBL_INT_ADD` macro implements addition to two unsigned 32-bit words. A wrapping
u64 models the same low-word carry and high-word wrap. The carry request reaches
the `md5_update` carry at a full block and, in a copied pre-boundary context, the
`md5_final` carry on repeated finalization. The 64-bit wrap is implemented but not
exhaustively streamed (it would require exabytes). MD5's modular round additions
are supplied by the existing crate.

The portable code's only caller-sized parameter is a slice. It checks conversion
to the upstream uint32 length before changing state. Internal buffer indices are
private and bounded: `data_len` starts at zero, update resets it at 64 and
finalization leaves it in 0..63. All input-sized slice operations use the minimum
of the remaining input and buffer capacity. There is no caller-controlled
recursion, global state, filesystem, socket, process, thread or unsafe code.

[VALIDATION.md](VALIDATION.md) records the tested code revision, successful local
gates and baseline evidence. The 5,972-case live run checked 67,617 actual digest/
validity snapshots and 9,631 equality/inequality events, with zero failures or
deferrals. This is a source-to-case branch audit, not an instrumented coverage
percentage or proof of exhaustive inputs. The owned unit is registered for the
existing live CI path. Its independent reviews and hosted head CI passed before
PR #42 merged into `rust`.
