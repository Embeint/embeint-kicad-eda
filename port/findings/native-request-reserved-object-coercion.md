# Preserve ordinary objects in native requests

Independent MD5 review reproduced this at PR #42 head
`cfed960cc9d7c4638c0cf6d8042d336e43a405c0`, using the unchanged shared request
helper from merged PR #45. This is a native transport defect, separate from the
portable MD5 algorithm or the exact result comparator.

```json
{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"object-id","input":{"steps":[{"op":"new","id":{"$serde_json::private::Number":"0"}}]}}
```

The actual C++ oracle refuses the object-valued ID with `invalid_input` and
`integer out of range: id`. Rust instead constructs state 0. The same marker in
byte data hashes a zero byte. `$serde_json::private::RawValue` can also coerce
ordinary objects into strings, arrays or envelope identities. Invalid marker
contents can incorrectly turn valid JSON into a null-identity parse refusal.
The independent review retained 14 complete request/reply reproductions. IEC's
subsequent adapter fixtures independently confirmed the shared defect.

The request helper validated an `Exact` tree, then deserialized the original
text as `serde_json::Value`. With the selected arbitrary-precision and raw-value
features, that generic deserializer recognizes serde's internal marker keys in
user objects. Rejecting those legal keys would also disagree with C++.

Source fix `50e3f24874ea430770d59e10aa3db095dec4f757` constructs objects, arrays,
strings, booleans and null directly from the validated tree. Only validated
number tokens enter `serde_json::Number` parsing. The explicit request budget
still accepts at most 127 simultaneously open containers, including empty
containers. Finite overflow-fallback checks, integer `-0`, full unsigned-64,
underflow and original number-token representation remain intact. Encoded JSON
in a string stays a string. Result parsing and comparison are unchanged.

## Local qualification

The private Linux copy used Rust/Cargo 1.99.1, GCC 15.2.0, the actual shared build
lock and at most eight build jobs. The pinned west workspace was verified before
Cargo. Commands completed successfully at the source fix above:

```sh
python3 port/tools/bootstrap_west.py --check
cargo fmt --manifest-path rust/Cargo.toml -p kicad-difftest --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-difftest --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-difftest -j8
python3 -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v
```

All 15 Rust integration tests passed: two CLI/process tests, three exact-protocol
tests, seven request tests and three actual C++ transport tests. All seven Python
policy tests passed. Synthetic phase-gate tests are not project qualification.

The new request test covers both marker keys with valid/invalid numeric-looking
strings, encoded JSON strings, nonstring values, nested objects and additional
keys. The new actual C++ fixture test sends 120 complete envelope requests across
input roots, nested values, both identities, extra fields and object roots. Its
registry is empty, so accepted envelopes yield `unknown_unit`; it does not run
MD5, IEC or resistor algorithms. C++ input observations travel as escaped JSON
strings and are checked through the unchanged exact parser, avoiding the same
Value coercion in the test observation path. Existing numeric, nesting, escaped
duplicate-key and comparator regressions also passed.

No dependency pin, lockfile, portable code, case, golden or immutable baseline
changed. Hosted CI, independent review and each pilot's actual adapter adoption
remain required. This finding does not accept the extractor graph or phase-zero
gate.
