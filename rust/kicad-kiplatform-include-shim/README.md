# kicad-kiplatform-include-shim

Native JSONL adapter for unit `libs.kiplatform.include.kiplatform.application.pure.p001`:
the public headers `libs/kiplatform/include/kiplatform/{app,drivers,environment,io,secrets,
sysinfo,sysinfo_gtk,touchpad,ui,webview}.h` at KiCad pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`, ported in `rust/kicad-kiplatform` (and, for
`drivers.h`, `sysinfo.h` and `sysinfo_gtk.h`, in `rust/kicad-kiplatform-sysinfo`).

The C++ side is `port/oracle/shims/kiplatform_include_p001.cpp`, which includes the ten
pinned headers unchanged. The headers' out-of-line declarations have their bodies in other
units, none of which is linked; the inline `MAPPED_FILE` accessors and the `nullptr`
default arguments are observed through `port/oracle/reference/kiplatform_include_probe.cpp`,
which compiles the same pinned `io.h`, `secrets.h` and `ui.h` text with `namespace
KIPLATFORM` renamed and supplies recording bodies there. The probe defines no genuine
`KIPLATFORM` symbol (the live workflow checks its object with `nm`).

## Operations

- `{"op":"constants"}`: `CLOUD_SYNC_BUFFER_SIZE`, the `FILE_LOCK::STATE` and
  `NATIVE_DISPLAY_TYPE` enumerator values.
- `{"op":"value_init"}`: `FILE_LOCK` default- and value-initialized (`fd`, `state`);
  `TARGET_ATTRS` and `PROXY_CONFIG` default- and value-initialized; `TOUCHPAD_GESTURE{}`,
  `GPU_INFO{}`, `CPU_INFO{}` and `MEMORY_INFO{}`. The C++ objects are built in storage
  pre-filled with `0xa5`, so a member the initialization leaves alone would show.
  Default-initialization of the aggregates with scalar members leaves them indeterminate;
  it is undefined to read and is not requested.
- `{"op":"target_attrs","value":u32[,"captured":bool]}`: `TARGET_ATTRS{ value }` (the
  omitted member takes its default member initializer) or `TARGET_ATTRS{ value, captured }`,
  and a copy.
- `{"op":"proxy_config","host":U,"username":U,"password":U}`: a default `PROXY_CONFIG`,
  the members assigned, and the implicit copy.
- `{"op":"touchpad_callback","callback":"set"|"empty","gestures":[G...]}`: a
  `TOUCHPAD_GESTURE_CALLBACK` holding a recording target, or empty; each gesture is passed to
  it and the reply has what the target received or `bad_function_call`, plus
  `explicit operator bool`. `G` is `pan_x`, `pan_y`, `zoom_factor` (16 lower-case hex digits
  of the IEEE-754 bits, so signed zeros, infinities and NaN payloads round-trip) and
  `zoom_anchor` (`[x, y]`, int32).
- `{"op":"touchpad_handlers","steps":[{"action":"create"|"release","slot":0..7}...]}`:
  handlers derived from `TOUCHPAD_GESTURE_HANDLER`, owned through
  `std::unique_ptr<TOUCHPAD_GESTURE_HANDLER>` as `CreateTouchpadGestureHandler` returns them;
  every construction and destruction is reported in order (destruction through the base
  pointer runs the derived destructor because the base destructor is virtual). What is left
  is released in slot order.
- `{"op":"mapped_file","state":S,"bytes":hex}`: `MAPPED_FILE::Data()` and `Size()` on the
  members the pinned Unix constructor leaves: `empty_file` (fstat reported 0 bytes; `m_data`
  stays null), `mapped` (mmap of a non-empty file) or `fallback` (mmap failed and
  `readIntoBuffer` filled `m_fallbackBuffer`; an empty buffer, reachable when the file
  shrinks to 0 bytes between `fstat` and the read, has a null `data()`). The reply has
  `data` (hex, or null), `size` and `points_to` (`null`, `mapping` or `fallback_buffer`).
- `{"op":"default_arguments","function":F,...,"explicit":bool}`: calls one declaration with
  a defaulted parameter through the header, omitting the defaulted argument or passing it
  explicitly: `atomic_rename`, `commit_temp_file` (`wxString* aError = nullptr`; also
  `returned`, what the recording body returns), `open_unique_sibling_temp_file` (`aError`),
  `atomic_write_file` (`aError`; also `data`, the `aSize` bytes at `aData`),
  `store_secret_async` (`std::function<void( bool )> aDone = nullptr`) and
  `get_native_window_handles` (`int* aDisplayType = nullptr`). The reply has the string
  arguments received, the null/empty checks in call order (`flags`), and the return value.

`U` is a `wxString` as its signed 32-bit `wchar_t` units (the pinned wxGTK build), at most
4096. Bytes are lower-case even-length hex, at most 4096 bytes. At most 64 gestures or
handler steps. Integers outside the field's type, non-integer numbers and non-numbers are
`integer out of range: <field>`; `-0` is an accepted zero (nlohmann stores it as a signed
zero).

## Refusals

Validation order: `op`; exact keys; then each field in the order listed above, entries in
order and their fields in the order shown. The 219 controls in
`port/oracle/fixtures/kiplatform-include-p001-controls.jsonl` are replayed on both programs
by `port/oracle/test_kiplatform_include_p001_inputs.py`, each followed by a successful
request.

## Corpus

`generate_cases.py` writes 235 fixed-seed public cases (`--check` verifies them): the
constants and initializations, `TARGET_ATTRS` bounds with and without `captured`, empty,
boundary, long and short-string-boundary `PROXY_CONFIG` strings, 18 special double bit
patterns and int32 anchor bounds through both callback kinds, handler create/replace/release
sequences up to 64 steps, every `MAPPED_FILE` state up to 4096 bytes, every defaulted
parameter omitted and explicit, and 96 splitmix64 requests across the operations. No case
defers.
