# Scalar angle libm needs a portable exact backend

Owner: `libs.kimath.include.geometry.application.ui.p001`.
Source: KiCad pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`, `eda_angle.h`.

The initial local implementation called Rust f64 tan/acos/asin/atan/atan2 directly.
On macOS arm64, a newly executed native JSONL shim was compared with the same code
compiled as import-free `wasm32-unknown-unknown`, release profile, Node v24.16.0 /
V8 13.6.233.17-node.49. The first mismatch was public case `unary-acos-7`,
`Arccos(-0.1)`. This is a Rust runtime portability observation, not a claim of
C++ parity or a confirmed KiCad defect. It proves that a host f64 call cannot be
advertised as an exact portable implementation merely because wasm compiles.

The final API replaces those runtime calls with typed `Unported` errors owned by
this unit, at exactly the missing call. The enclosing wrapper, radians conversion
and vector construction continuation remain implemented. Existing accepted glibc
sin/cos kernels are reused and stay active; vector axes/diagonals remain exact.
No result lookup, tolerance or narrowed valid-input domain is introduced.
`Round`'s pow calls are owned typed `libc::pow` deferrals at the first pow, which
C++ evaluates before KiROUND, so wiring KiROUND alone cannot expose host pow
results; exact portable pow must be qualified before that deferral is removed.

The full public corpus is now executed in import-free wasm against newly executed
native Rust replies, including every typed deferral. Hosted native exact C++
comparison remains a separate gate and enumerates every libm deferral rather than
excluding it. No golden is created. Next step: supply a reviewed pure Rust port
of the pinned reference runtime's tan/inverse functions and qualify both targets
on these same cases before removing each stub.

## Undefined-domain corpus correction

The initial draft corpus put four extreme-distance queries into the successful
corpus: `poly-13-14-2`, `poly-13-14-10`, `poly-14-15-2`, `poly-14-15-10`.
Code inspection of pinned `SEG::SquaredDistance` shows that its signed i64 dot/norm
intermediates overflow on these inputs. They are retained verbatim in the separate
`geometry-controls.jsonl`; they are not excluded or blessed. The native adapter
checks the actual distance-call domain and individual i64 intermediates before
invoking the unmodified body. The Rust path currently stops earlier at unported
`SEG::Square`, so each control explicitly records that deferral separately from
the C++ undefined-input refusal. The three signed-i32 controls remain refusals in
both adapters. This is source-derived evidence, not an observed sanitizer result.
