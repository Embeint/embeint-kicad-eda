# kicad-kiplatform-shim

Native JSONL adapter for unit
`libs.kiplatform.os.unix.application.thin.replacement.p001`
(`libs/kiplatform/os/unix/policy.cpp`), the Rust side of the difftest. The C++ side
is `port/oracle/shims/kiplatform_policy.cpp`, which calls the unchanged pinned
`policy.cpp` compiled into `kicad-oracle`.

## Owned behaviour

| Upstream symbol | Rust | Unix body |
| --- | --- | --- |
| `PBOOL KIPLATFORM::POLICY::GetPolicyBool( const wxString& aKey )` | `kicad_kiplatform::policy::get_policy_bool` | `PBOOL::NOT_CONFIGURED` |
| `std::optional<std::uint32_t> KIPLATFORM::POLICY::GetPolicyEnumUInt( const wxString& aKey )` | `kicad_kiplatform::policy::get_policy_enum_uint` | `std::nullopt` |

Both keep their key argument and ignore it, as the reference does. `PBOOL` is a
minimum co-port from the header unit
`libs.kiplatform.include.kiplatform.application.thin.replacement.p001` (recorded in
its notes); `GetPolicyEnum<T>` and the Windows/macOS policy sources are not ported.

## Key boundary contract

The key is the accepted `wxString` replacement, `kicad_map_helpers::WxString`
(unit `libs.core.include.core.application.thin.replacement.p001`, merged in rust),
reused rather than duplicated: the pinned wxGTK build stores `length()` signed
32-bit `wchar_t` units (`wxUSE_UNICODE_WCHAR`, `WCHAR_MIN < 0`, statically asserted
in the C++ adapter). Both functions take `&WxString` and never read it, so every
key is in the defined domain: empty, NUL-bearing, lone or reversed surrogates,
values above U+10FFFF and negative units down to `INT32_MIN` are all accepted and
never refused.

## Protocol

Input: `{"op": "get_policy_bool" | "get_policy_enum_uint", "key": [int32, ...]}`
with at most 4096 units, encoded as the accepted map_helpers adapter encodes a
`wxString`. Validation order, identical on both sides: exactly the fields `op` and
`key`; `op` a string; `key` an array of at most 4096 elements; each element a JSON
integer in `-2147483648..=2147483647` (integer `-0` is unit 0); then the operation
name. Both adapters build the key from every unit (`wxString( units.data(),
units.size() )` / `WxString::from_units`); the C++ adapter also checks that the
`wxString` retained every unit before calling the reference. The 4096-unit bound
is a harness limit, not a reference refusal. The Rust adapter's only other
refusal, an allocator failure building the key, corresponds to `std::bad_alloc`
and is unreachable at that bound.

Result: `{"key_length": <units>, "value": "NOT_CONFIGURED" | null}`. `key_length` is
an adapter check that the whole key reached the call; `value` is the reference
result (the `PBOOL` enumerator name, or the optional as a number or `null`).

## Corpus

`generate_cases.py` writes 36 cases (17 keys plus a literal `-0` key, each sent to
both functions): the three public keys of `include/policy_keys.h`, empty and NUL
keys, non-ASCII, surrogate, beyond-Unicode, `INT32_MAX`, `INT32_MIN`, `-1` and
mixed-sign units, and one 4096-unit fixed-seed splitmix64 key. It also writes 25
malformed-request controls (`port/oracle/fixtures/kiplatform-policy-controls.jsonl`),
run by `port/oracle/test_kiplatform_policy_inputs.py` with a successful
continuation after each; they are refusal controls, never reference results.
`--check` verifies both files.

## Local evidence

WSL Ubuntu host, g++ 15.2.0, wxWidgets 3.2.9, rustc 1.99.1, Node 22.22.1; the
overlay was this branch's working tree before its implementation commit. This is
not the ubuntu-24.04 hosted reference; hosted CI is the authority.

- `kicad-difftest compare --partial`: 34 cases, 34 compared, 0 deferred, 0 failures,
  0 unprocessed; oracle and Rust output SHA-256 both
  `1b6cff9ea321934aa63423e52f11ce492344fd9d46729119f01af9557e511820`.
- Controls: 23 refusals with continuations on each binary, byte-identical replies.
- Import-free wasm32 replay of all 34 against the fresh C++ replies passes.
- `nm -C kicad-oracle` defines both `KIPLATFORM::POLICY::GetPolicyBool(wxString const&)`
  and `GetPolicyEnumUInt(wxString const&)` (`T`); the adapter object carries
  `R_X86_64_PLT32` call relocations to both, and `compile_commands.json` compiles the
  pinned `libs/kiplatform/os/unix/policy.cpp`.
