# kicad-kiplatform-sysinfo-shim

Native JSONL adapter for unit `libs.kiplatform.os.unix.application.pure.p001`: the
Unix `libs/kiplatform/os/unix/drivers.cpp` and `sysinfo.cpp` at KiCad pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`, ported in `rust/kicad-kiplatform-sysinfo`.
The C++ side is `port/oracle/shims/kiplatform_unix_p001.cpp`, which calls the genuine
pinned bodies linked from the audited `kiplatform-unix-reference` archive.

## Operations

`{"op":"valid_3dconnexion_driver_version"}` calls
`KIPLATFORM::DRIVERS::Valid3DConnexionDriverVersion()` and replies
`{"returned":false}`.

`{"op":"sysinfo","memory":M,"cpus":[C...],"gpus":[G...],"steps":[S...]}` decodes
fully initialized output arguments (every field is required, so no C++ uninitialized
scalar is ever observed), constructs a `SYSINFO`, runs the steps in order on that
object, destroys it and replies:

- `before`: the decoded `memory`, `cpus` and `gpus`;
- `steps`: per step, `step`, `returned` (the method's `bool`; absent for
  `reconstruct`), the full `state` after the step and `storage_unchanged`;
- `after_destroy`: the `state` after the final destructor and `storage_unchanged`.

Steps are `memory` (`GetMemoryInfo`), `cpu` (`GetCPUInfo`), `gpu` (`GetGPUInfo`) and
`reconstruct` (destroy the object and construct a new one). `storage_unchanged`
compares the address, length and capacity of the CPU and GPU vectors and of every
string they hold with a snapshot taken after decoding; only the equality is
reported, never an address.

Field encodings: `M` is `usage` (uint32) and `total_physical`, `free_physical`,
`total_paging`, `free_paging`, `total_virtual`, `free_virtual` (uint64). `C` is
`name`, `manufacturer` (hex bytes) and `number_cores`, `number_logical` (uint32).
`G` is `name`, `driver_version`, `manufacturer` (hex bytes) and `memory_size`
(int64 `long long`). Hex is lower case, even length; `std::string` values are byte
strings, so NUL and invalid UTF-8 round-trip exactly.

Bounds (both adapters): at most 64 CPU entries, 64 GPU entries and 64 steps; at most
4096 bytes per string.

## Refusals

Validation order: `op` (`expected string: op`, `unknown operation`); exact keys
(`unexpected or missing field`); `memory` shape and then each field in the order
above; `cpus` (`expected array`, `too many entries`, entry keys, then fields in
declaration order); `gpus` likewise; `steps` (`expected array`, `too many entries`,
`unknown step`). Integers outside the field's type, non-integer numbers and
non-numbers are `integer out of range: <field>`; `-0` is an accepted zero (nlohmann
stores it as a signed zero). The 67 controls in
`port/oracle/fixtures/kiplatform-unix-p001-controls.jsonl` are replayed on both
programs by `port/oracle/test_kiplatform_unix_p001_inputs.py`, each followed by a
successful lifecycle.

## Corpus

`generate_cases.py` writes 126 fixed-seed public cases (`--check` verifies them): the
driver call, construction/destruction alone, every single step and step pair on empty
and populated fixtures, field and length bounds, NUL/non-UTF-8/every-byte strings,
64-entry and 64-step requests and 48 splitmix64 fixtures. No case defers.
