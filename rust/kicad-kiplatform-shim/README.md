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

## Local evidence (host toolchain, not qualification)

Tested commit `c61c07592b64dec3b21b9e3d50e79695b0dce407` (accepted rust
`98a12c1123` merged), staged from raw Git blobs (`core.autocrlf=false`,
`core.eol=lf`; all 916 pinned reference files and 3,939 overlay files verified
byte-identical to their blobs) into a dedicated WSL tree, oracle built with
`build.sh` at `-j8` under the shared build lock.

Toolchain: WSL Ubuntu, g++ 15.2.0, glibc 2.43, wxWidgets 3.2.9, CMake 4.2.3,
rustc 1.99.1, Node 22.22.1. This is **not** the GCC 13 / glibc 2.39 (ubuntu-24.04,
wxWidgets 3.2.4) reference environment; no qualification result is claimed here.
Hosted CI and reference-environment runs are the authority.

- Pinned `libs/kiplatform/os/unix/policy.cpp`: SHA-256
  `85d8a7f03cf8cf15b6e7007cbb8c72d6c1b04d11c061b68c83b2d828d505cd75`, equal to the
  pin's blob and the source manifest (archive SHA-256
  `d0bef2ffab07a3a2a9847824e80d879fc8523349503c2da5fbbe28197c08441f`);
  `compile_commands.json` compiles it into `kicad-oracle`.
- `nm -C kicad-oracle` defines `KIPLATFORM::POLICY::GetPolicyBool(wxString const&)` and
  `GetPolicyEnumUInt(wxString const&)` (`T`); the adapter object has
  `R_X86_64_PLT32` call relocations to both.
- Oracle ctest: 14 of 14 pass, including `oracle-kiplatform-policy-inputs`.
- `kicad-difftest compare --partial`: 36 cases, 36 compared, 0 deferred, 0 failures,
  0 unprocessed; oracle and Rust output both SHA-256
  `c79a22a5f355b31b9c51058cb7765be4c57483c3284f3035dcc0abdd34c0abe5` (cases
  `1f8df908d1743f965ee4b83dd222751096bcaa68d30aac5117a66019dc4697f6`). No golden
  was requested or blessed.
- Refusal controls: 25 on each binary, each followed by a successful
  continuation, replies byte-identical across oracle and Rust.
- Import-free wasm32 replay of all 36 cases against this run's fresh C++ replies
  passes.
- Rust: `cargo fmt --check`, `clippy --all-targets -D warnings`, tests
  (`kicad-kiplatform` 1, shim corpus 2, `kicad-map-helpers` 7) and the wasm32
  check pass on this host and on Windows.
