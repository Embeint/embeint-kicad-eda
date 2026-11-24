<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Rust-port preflight validation

Implementation starts from accepted `rust@1ac339f229d8ccb8ccb297f489493622903cc468`
and merges accepted `main@06f9f7fe8aa43f9fada5cb3d2889d5481d274437` to bring in
PR57's existing runner support. No open topic is included. Original runner
installation evidence remains unchanged in `validation.md`.

Local macOS source-resolution/admission checks pass eight test methods, including
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
