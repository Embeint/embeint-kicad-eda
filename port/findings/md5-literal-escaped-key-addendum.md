# MD5 nonduplicate escaped marker-key coverage

Independent review of PR #42 at `c5ec29e70d` closed the reserved-object runtime
P2 but found one P3 evidence gap. The three nonduplicate regressions named
`reserved-escaped-id`, `reserved-escaped-byte` and `reserved-escaped-unit` have
plain marker keys. Their escaped quotes occur in VALUES, not KEYS. The existing
escaped duplicate is refused before object conversion and does not cover these
ordinary-object placements. The corresponding historical nonduplicate-key
coverage claim in the 65d5b0a9 evidence is withdrawn here.

The original 199 fixtures and their immutable evidence remain unchanged.
Source `da17dc54dc` appends three literal escaped-key cases and successful
recovery. Raw key spellings and decoded object keys are asserted, and default
Python and Rust checks require the complete expected refusal envelopes. Both
real adapters pass all 203 replies; the old helper fails all three new refusals.

Fresh fmt, Clippy, 11 shim tests, native/wasm builds and all 5,973 registered
cases pass. Runtime production bodies, shared helper/comparator, unit fields,
cases/goldens and old evidence are unchanged. The retained C++ oracle and exact
runner were identity/provenance-verified and reused; their old source-export
representation is preserved rather than relabelled. No new C++ build is claimed.

Evidence is in `rust/kicad-md5-shim/validation/2026-11-22-da17dc54/`.
The bounded correction awaits independent re-review; final hosted checks,
parent merge decisions and phase-zero activation remain separate requirements.
