<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kicad-kiplatform-unix-shim

JSONL adapter for unit `libs.kiplatform.os.unix.application.ui.p001`: the Linux bodies of
`libs/kiplatform/os/unix/{app,environment,io,printing,secrets}.cpp`, ported in
`kicad-kiplatform-unix`. It mirrors `port/oracle/shims/kiplatform_unix_ui_p001.cpp` field for
field, including validation order and refusal messages.

## Request

```json
{"unit": "libs.kiplatform.os.unix.application.ui.p001", "case": "<id>",
 "input": {"op": "<operation>", "args": {...}, "host": {...}}}
```

`args` are the body's arguments (`wxString`s as arrays of signed 32-bit `wchar_t` units, byte
strings as lower-case hex, doubles as the 16 hex digits of their bits). `host` scripts the
effects the body causes:

| Key | Meaning (default) |
| --- | --- |
| `libc` | rule of the scripted `wxConvLibc`: `ascii`, `latin1`, `utf8` or `fail` (`ascii`) |
| `filename` | rule of the scripted `wxConvFileName` (`utf8`) |
| `env` | variables `wxGetEnv` sees: name to units, or null for unset (none set) |
| `setenv` | what `wxSetEnv` returns (`true`) |
| `trace` | whether the `KICAD_PRINT` trace mask is enabled (`false`) |
| `replies` | per function, the queue of replies its calls consume in order |

A call whose queue is empty refuses the request (`missing reply: <function>`). Handles the
host returns are labelled `<kind>#<n>`, numbered in creation order across the case.

Operations: `trivial` (the bodies that make no call: `function` names them), `build_config`,
`app_init`, `app_set_current_thread_priority`, `app_terminate_with_code`, `env_init`,
`env_move_to_trash`, `env_is_removable_path`, `env_get_documents_path`,
`env_get_user_{config,data,local_data,cache}_path`, `env_get_system_proxy_config`,
`io_seq_f_open`, `io_duplicate_permissions`, `io_make_writeable`, `io_is_file_hidden`,
`io_timestamp_dir`, `io_flush_to_disk`, `io_mapped_file`, `secrets_store`,
`secrets_store_async`, `secrets_get`, `secrets_get_async`, `secrets_get_no_prompt`,
`secrets_delete`, `printing_print_pdf` (1 to 8 calls in one process, so `s_lastSettings`
carries between them) and `printing_reset`.

## Reply

`{"calls": [...], "returned": ..., ...}`: every recorded host call in order, the body's return
value and its observable outputs (out-parameters, the print settings left, the mapped file's
members). A thrown `std::runtime_error` is `exception` (its `what()` bytes), an empty
`std::function` invoked by `lookupFinished` is `"exception": "bad_function_call"`.
`app_terminate_with_code` runs the body in a child process (the oracle forks) and reports its
`exit_status`. A call into another unit's code is an `unported` reply:
`MAPPED_FILE::readIntoBuffer` (owned by `libs.kiplatform.os.common.application.ui.p001`) and the
documents-path fallback's `wxFileName::MakeAbsolute`.

## Refusals

Malformed requests and replies are `invalid_input` with the adapter's message. Inputs whose
reference evaluation is undefined are refused too, with `undefined: ...`: a signed overflow in
`TimestampDir`, a zero page dimension `draw_page` divides by when fitting, and the one-byte
`"%"` path for which `wxURI::Unescape` forms `end - 2` before its buffer. The controls are in
`port/oracle/fixtures/kiplatform-unix-ui-p001-controls.jsonl`, replayed against both programs
by `port/oracle/test_kiplatform_unix_ui_p001_inputs.py`.

## Files

- `generate_cases.py`: the fixed-seed corpus, the controls and `expected-replies.json`
  (`--check` verifies them; byte-identical on Windows and Linux).
- `tests/corpus.rs`: Rust-only counts, control refusals and decoded-value checks.
- `examples/corpus_parity.rs` and `wasm-test.mjs`: the corpus in import-free wasm32 against a
  run's native C++ replies (`app_terminate_with_code` cases are native only).
