<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/core/include/core p001 shim

Native JSONL adapter for unit `libs.core.include.core.application.pure.p001`: the header
bodies of `libs/core/include/core` at KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`
(`profile.h`, `observable.h`, `union_find.h`, `filter_kruskal.h`, `minoptmax.h`,
`spinlock.h`, `sync_queue.h`, `arraydim.h`, `ignore.h`, `type_helpers.h`). The portable
port is [`kicad-core`](../kicad-core) (`kicad_core::include_core`); the C++ counterpart is
[`port/oracle/shims/core_include_p001.cpp`](../../port/oracle/shims/core_include_p001.cpp),
which compiles the unmodified pinned headers. Both adapters validate in the same order and
refuse with the same messages. The source audit and evidence are in [AUDIT.md](AUDIT.md).

The unit remains partial after CI: concurrent operations and nondefault ostream state
are unfinished, scoped timers require explicit finish instead of automatic drop, and
profile string allocation/queue element cloning can still allocate infallibly. Collection
reservation errors do not establish complete allocation-failure equivalence.

```text
python3 rust/kicad-core-include-shim/generate_cases.py [--check]
cargo test -p kicad-core -p kicad-core-include-shim
kicad-difftest compare --unit libs.core.include.core.application.pure.p001 --root . \
    --oracle <oracle-build>/kicad-oracle --rust rust/target/debug/kicad-core-include-shim
python3 port/oracle/test_core_include_p001_inputs.py <oracle> rust/target/debug/kicad-core-include-shim
cargo run -p kicad-core-include-shim --example adversary > rust/kicad-core-include-shim/adversary-weights.json
```

The corpus (`port/cases/libs.core.include.core.application.pure.p001.jsonl`) holds only
requests inside the reference's defined domain. Requests both sides must refuse are in
`port/oracle/fixtures/core-include-p001-controls.jsonl` with the expected refusal; the
oracle's ctest `oracle-core-include-p001-inputs` and the Rust tests replay them.
`.github/workflows/rust-core-include-live.yml` also replays the corpus in import-free wasm32
against the run's native C++ replies.

## Operations

Every input is an object with `op`. Strings are at most 64 KiB. Unknown or missing fields
refuse with `unexpected or missing field`.

| `op` | Fields | Result |
| --- | --- | --- |
| `minoptmax` | `type` (`int`, `long long`), `sets` (<= 64 of `{set: min/max/opt, value}`), `probes`, `defaults` (<= 64 values) | `states`: after construction and each setter: `Min/Max/Opt/PinnedOpt()`, `Has*`, `IsNull`, `Matches` per probe, `PinnedOpt( d )` per default |
| `union_find` | `count` (u64), `steps` (<= 100,000: `unite {a, b}`, `connected {a, b}`, `find {x}`, `find_compress {x}`, `reset {count}`, `size`, `component_count`) | `constructed`, per-step `results`, `components`, the private `parents` array. A count above 2^32 - 1 yields `{"exception": "length_error", "what": ...}` |
| `filter_kruskal` | `vertices` (<= 8,192), `less` (`weight`, `weight_id`, `weight_uv`), `edges` (`[u, v, w]`, <= 8,192) or `generate` (below), optional `unite` pairs applied first | `emitted` (edge ids in emission order), `returned`, `order` (the reordered span's ids), `components`, `parents` |
| `spinlock` | `steps`: `lock`, `try_lock`, `unlock`, `test` | per-step `results` |
| `sync_queue` | `receiver` (the `pop` target, kept across steps), `steps`: `push {value}`, `move_push {value}`, `pop`, `empty`, `size`, `clear` (`SYNC_QUEUE<std::string>`) | per-step `results` |
| `profile` | `clock` (<= 4,096 time points, \|t\| <= 2^62 - 1), `steps` (below) | per-step `results`, `clock_calls` |
| `observable` | as in [kicad-core-shim](../kicad-core-shim/README.md), including `throw` | `events` |
| `array_dim` | `kind`: `char[1]`, `int[3]`, `double[16]`, `pair[7]`, `char[1000]` | `value` |
| `to_underlying` | `type`: `int8`, `uint8`, `int16`, `uint32`, `int64`, `uint64`, `int`; `value` | `value` |
| `ignore_unused` | `value` string | `value` (unchanged) |

`generate` draws `std::mt19937( seed )` after `discard` outputs, as
`qa/tests/common/test_filter_kruskal.cpp` does: optional `seeds` pairs `(rng() % V, rng() % V)`
first, then `count` edges `(rng() % V, rng() % V, rng() % weight_range)` (or with `islands`,
`island = rng() % islands` and endpoints inside that island), then `weight_set` or
`weight_add`. Doubles are reported as the 16 hex digits of their bits.

### Profile scripts

Slots: four `PROF_TIMER`s (`timer`), two `LATENCY_PROBE`s (`probe`), two `PROF_COUNTER`s
(`counter`). Steps: `timer_new {timer, name, autostart?}` (omitting `autostart` uses the
default argument), `timer_default {timer}`, `timer_start`, `timer_stop`, `timer_msecs
{since_last?}`, `timer_since_start {duration, since_last?}` (durations `double_nano`,
`double_micro`, `double_milli`, `double_seconds`, `nanoseconds`, `microseconds`,
`milliseconds`, `seconds`), `timer_show`, `timer_to_string`, `timer_get_name`,
`scoped {duration, base: [timer steps on the SCOPED_PROF_TIMER's base]}` (result: the base
step results and the destructor's duration), `probe_new {probe, name, stages?}`,
`probe_reset`, `probe_checkpoint {name}`, `probe_add_timer {timer}`, `probe_to_string`,
`probe_delta_ms {a, b}`, `counter_new {counter, name?}`, `counter_inc` (`operator++( int )`),
`counter_count`, `counter_reset`, `counter_show`, `counter_set {value}` (sets the private
count). Every `high_resolution_clock::now()` takes the next scripted value; the oracle
links with `-Wl,--wrap=_ZNSt6chrono3_V212system_clock3nowEv` to supply them.

Refusals at exactly the undefined points: `timer_to_string` before any `Start()` of a timer
built with `autostart: false` (`indeterminate m_started`), `probe_delta_ms` whose difference
overflows, `lock` on a held spinlock (`lock would spin forever`: with one thread the
reference never returns), union-find indices and FilterKruskal endpoints outside the parent
array (`index out of range`, `endpoint out of range`, checked when the reference would read
them), and an observable `throw` through the catch block of an observable destroyed in its
own `notify_ignore`. Harness limits: `count too large`, `clock script exhausted`, the clock
value bound (so every time point difference a timer takes is defined), and the list sizes.
