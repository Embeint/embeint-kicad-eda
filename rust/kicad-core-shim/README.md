<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/core p001 shim

Native JSONL adapter for unit `libs.core.application.pure.p001`: `libs/core/base64.cpp`,
`observable.cpp`, `profile.cpp` and `version_compare.cpp` at KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The portable port is
[`kicad-core`](../kicad-core); the C++ counterpart is
[`port/oracle/shims/core_p001.cpp`](../../port/oracle/shims/core_p001.cpp), which calls the
reference functions compiled into the oracle's pinned `core` archive. Both adapters
validate in the same order and refuse with the same messages. The source audit and
branch evidence are in [AUDIT.md](AUDIT.md).

```text
python3 rust/kicad-core-shim/generate_cases.py [--check]
cargo test -p kicad-core -p kicad-core-shim
kicad-difftest compare --partial --unit libs.core.application.pure.p001 --root . \
    --oracle <oracle-build>/kicad-oracle --rust rust/target/debug/kicad-core-shim
python3 port/oracle/test_core_p001_inputs.py <oracle> rust/target/debug/kicad-core-shim
```

The corpus (`port/cases/libs.core.application.pure.p001.jsonl`) holds only requests inside
the reference's defined domain. Requests both sides must refuse are kept separately in
`port/oracle/fixtures/core-p001-controls.jsonl` with the expected refusal; the oracle's
ctest and the Rust tests replay them. `.github/workflows/rust-core-live.yml` also replays
the corpus in import-free wasm32 against the run's native C++ replies.

## Operations

Every input is an object with `op`. Byte strings are lowercase hex. Unknown or missing
fields refuse with `unexpected or missing field`.

| `op` | Fields | Result |
| --- | --- | --- |
| `encode_length`, `decode_length` | `len`: u64 | `value`: u64 (`size_t` modular arithmetic) |
| `encode`, `decode` | `input` hex (<= 64 KiB), optional `output` hex prefix, optional `repeat` 1..65536 (input repeated; total <= 1 MiB) | `output`: hex of the prefix plus the appended bytes |
| `parse_version` | `text` (<= 64 KiB) | `components`: int array |
| `compare_versions` | `a`, `b` | `value`: bool |
| `running_micro_secs` | `tv_sec` i64, `tv_nsec` 0..999999999: the scripted `CLOCK_MONOTONIC` reading | `value`, `clock_calls`, `clock_id` |
| `latency_probe` | `name`, `start` (ns), `checkpoints`: <= 256 of `{name, timestamp, timer, delta}` (`delta`: 16 hex digits of the double) | `text`: `LATENCY_PROBE::to_string()` |
| `observable` | `steps` (below) | `events`: per top-level step, `calls` and the `state` snapshot |

Refusals: non-terminating version strings (`version string does not terminate`, see
[the upstream issue](../../port/upstream-issues/core-version-parse-nonterminating.md)),
signed overflow in the clock arithmetic, overflowing time point differences, and
malformed fields.

### Observable scripts

Eight observable slots (`obs` 0..7, each a `UTIL::OBSERVABLE<OBSERVER>`), sixteen link
slots (`link` 0..15, each a `UTIL::LINK`), eight observers (`observer` 0..7, or `null`
for `nullptr`). Steps: `new`, `copy {obs, from}` (copy construction), `drop`,
`subscribe {obs, observer, link}` (`Subscribe`, the returned link moved into the slot),
`subscribe_unmanaged`, `unsubscribe`, `notify {obs, event, reactions}`,
`notify_ignore {..., ignore}`, `enter_iteration`, `leave_iteration`,
`on_observers_empty`, `link_new` (`LINK()`), `link_token {link, obs, observer}`
(`LINK( impl_, observer )`), `link_move {link, from}` (move construction),
`link_assign {link, from}` (move assignment, including self-assignment), `link_reset`,
`link_drop` (destruction). `reactions` are `[{observer, steps}]`: whenever that observer
is notified by this notification, its steps run (nesting up to depth 8).

The snapshot lists each slot's `impl` (a label in first-seen order over observables then
links; `null` for a null `shared_ptr`), `size()`, each link's `operator bool`, private
`token_` and `observer_`, and each live `IMPL`'s `observers_`, `iteration_count_`,
`is_shared()` and `use_count()`.

Undefined in the reference, refused (or skipped and logged with `guard: true`): adding to
a list an active `Notify` loop is ranging over, `unsubscribe` with a null `impl_`. Script
limits: no `drop` of an observable inside its own notification, base operations
(`enter_iteration`, `leave_iteration`, `on_observers_empty`) only at top level, 2,048
steps, 100,000 executed steps and callbacks.

The C++ adapter runs the real `OBSERVABLE<T>` template. That template is a header body
owned by `libs.core.include.core.application.pure.p001`; the Rust driver
(`src/observable_driver.rs`) plays its role with the same base primitives and is harness
code, not a port of it.
