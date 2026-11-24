<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Rust-port preflight validation

Implementation starts from accepted `rust@1ac339f229d8ccb8ccb297f489493622903cc468`
and merges accepted `main@06f9f7fe8aa43f9fada5cb3d2889d5481d274437` to bring in
PR57's existing runner support. No open topic is included. Original runner
installation evidence remains unchanged in `validation.md`.

Local macOS source-resolution/admission checks pass five test methods, including
raw event/ref guards, push-SHA pinning despite branch advancement, exact existing
repository manual resolution and remote-row ambiguity refusal. These are control
tests, not execution on Proxmox or numerical parity evidence.
`actionlint` 1.7.12 passed on the new preflight and all five changed/integrated
workflows. The preflight's block scripts passed `bash -n`; whitespace checks
passed. No numerical cases, goldens, compiler policies or hosted test commands
were changed.

At initial implementation, actual cold/warm Proxmox runs, hosted comparison and
separate review are pending. The final report must distinguish these outcomes
and record the tested source/workflow revision, runner, queue/execution duration,
cache state, resource limits and failed or cancelled attempts. No numerical
qualification or speedup is claimed before those runs.

## Proxmox execution

Run 45399478228 (push of `e410d4e7b8bc398fa4be81a3c5754f6cb8c7b040`, cold cache)
ran on `proxmox-kicad-isolated-01` with cgroup `memory.max` 2684354560,
`memory.swap.max` 0, `cpu.max` 200000 100000 and `pids.max` 512. Formatting,
clippy, workspace tests and the wasm32 build passed. `validate_units.py` was
killed by the memory limit (exit 137): `graph.json.gz` decompresses to
2438877733 bytes and the tool's peak resident size on macOS arm64 is about
5.2 GB. The step now runs only on hosted fallback runs, and Proxmox runs record it
as deferred to `Rust port progress pages`, which validates units on every `rust`
PR and push. The runner's limits are unchanged.
