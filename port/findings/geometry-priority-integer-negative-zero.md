# Grid priority JSON integer negative zero

Owner: `libs.kimath.include.geometry.application.ui.p001`, Rust JSONL transport.

A final input audit on fa65cc300ec45ac806332dc1ecdfbad75989a1d5 found that priority
`-0` returned `invalid_input: priority out of range` on the Mac Rust shim.
The C++ adapter accepts it: nlohmann parses it as signed integer zero, which
passes the checked unsigned-priority reader. This expectation is source-derived;
the new literal controls execute both genuine native and Rust adapters in CI.

The fix follows the accepted numeric-util shim's unsigned reader: as_u64, then a
checked nonnegative as_i64 conversion. Floating spellings remain refused. The
four integer-zero comparisons and two floating-input refusals retain literal
request text, so Python JSON parsing cannot silently normalize -0 before replay.
They also execute in import-free wasm. The successful geometry corpus is unchanged.
