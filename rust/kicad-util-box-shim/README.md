# Utility/BOX2 JSONL adapter

Owns `libs.kimath.include.math.application.ui.p001` at KiCad pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. Implementation lives in kicad-math;
this host crate supplies I/O. The paired adapter calls real pinned C++ APIs in
`port/oracle/shims/util_box.cpp`. See [AUDIT.md](AUDIT.md) for every body group,
default, continuation, dependency owner, qualification limit and evidence.

Requests use the shared `{unit,case,input}` envelope. Inputs reject missing/extra
fields. Integers use exact JSON integers; floating values are raw lowercase IEEE
hex strings (16 digits for f64, eight for f32). Results preserve those encodings.
Input or undefined-arithmetic refusals are distinct from typed `unported` replies.

Scalar operations: `round` (`type,a,target,quiet`), `round_default` (`type,a`),
`checked_cast` (the long long->int specialization), `sign`, `isqrt`, `ct_sqrt`,
`ct_sqrt_helper` (`lo,hi`), `clamp` (`lo,hi`), `equals` (`b,epsilon`; null means
omitted default), `rescale` (`b,d`), `abs`, and `hypot` (`b`). `ll` preserves C++
long long's distinct type identity; `i64` names Linux int64_t=long. Rounding covers
source integer widths through u64, f32/f64, and return widths through i64/u32/ll.
C++ round_default invokes the actual omitted template/quiet arguments.

Box inputs use `op:box`, `type:i32/i64/f64`, `kind`, and `a`. Box descriptors have
`mode:default/maximum`, `at` with `pos`, `new/raw/center` with `pos,size`, `corners`
with `pos,end`, or `compute` with `points`. Raw descriptors use actual setters,
so inverted boxes and initialization state are observable. BOX2I size components
are i64, not i32. Operation kinds follow the audit; `p/q` are point arguments,
`b` is a second box, `s` supplies paired deltas/sizes, `n` is a scalar, `angle`
uses f64 degree bits, and `radius/width` are i32. Mutating Normalize/Merge/Inflate
also observe the actual returned reference. Circle/segment/rotated overloads are
all invoked in C++, with explicit dependency deferrals on Rust's missing calls.

Minmax inputs use `op:minmax,kind,a,b,p`, where `a` has four i32 edges. `b` supplies
another box, endpoint, or arc midpoint depending on kind. Other owned helpers use
`safe_parts`, `safe_box`, and `round_box`. The generator is the canonical complete
set of schema examples; it writes inputs only, never answers.

`generate_cases.py --check` verifies the deterministic public corpus and separate
UB controls. `compare_controls.py` requires both executables to refuse every UB
control and process a succeeding later request. `check_caller_regressions.py`
requires all accepted passing vector/geometry cases to remain exactly compared,
with their original input objects. No golden or tolerance is used.

The scoped workflow rebuilds the full audited oracle, compares this unit and both
rewired callers, then embeds this revision's newly executed native JSON replies
in an import-free wasm runtime replay. It does not use checked-in answers.
