<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Existing-runner setup evidence — 2026-11-24

This infrastructure change starts from product `main` at
`04ad4449043a8af3f6d8530deb9f2124e08c450b`. It does not import the open
reference-environment, hash, arithmetic or golden changes from port PRs.

## Inventory and access

Authorized SSH and the read-only Proxmox cluster inventory found these existing
build machines. Values below are configured virtual resources; ballooning means
guest-observed memory can be smaller. No VM was created, resized or restarted.

| Existing VM | Configured resources | Existing registration | Disposition |
| --- | --- | --- | --- |
| 101 `runner-htf-v2-02` | x86_64, 4 vCPU, 7000 MiB, 60 GiB disk; Ubuntu 26.04 guest | `Embeint/Embeint-HTF-V2`, `proxmox-htf-v2-02` | Additional resource-limited Ubuntu 24.04 KiCad container; original registration preserved |
| 102 `runner-personal-01` | x86_64, 4 vCPU, 7000 MiB, 60 GiB disk; Ubuntu 26.04 guest | `Karl-VonRichter/open-cad`, `proxmox-personal-01` | Preserved; guest reported only 2 GiB RAM at inspection |
| 106 `runner-infuse-01` | x86_64, 6 vCPU, 12 GiB, 300 GiB disk | No repository runner exposed by `Embeint/infuse-sdk` API | Preserved; guest agent unavailable and direct SSH inaccessible from this Mac |
| 2006 `github-actions-runner` | x86_64, 3 vCPU, 8000 MiB, 32 GiB disk; Ubuntu 24.04 guest | `Embeint/Embeint-HTF-V2`, `proxmox-htf-v2` | Preserved; only 8.7 GiB free at inspection |

Both HTF registrations reported `online`, `busy=false` before installation; no
`Runner.Worker` process was present on the selected VM. The selected guest had
38 GiB free disk and over 3 GiB available RAM. Existing Docker containers and
runner services were left running. The AgentFleet deployment host/runner and
Windows/WSL/Rancher were not used.

The KiCad repository originally exposed zero runners. Org-wide runner and group
APIs returned 403 for the current OAuth scope. Repository admin permission and
existing authorized SSH were sufficient to create a separate repository runner;
no OAuth scopes, org permissions or other repository registrations were changed.

## Isolation verification

The local runner archive was verified against the repository download API's
SHA-256 for version 2.338.2:
`baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40`.
The image preinstalls the workflows' pinned Rust 1.99.1 and MSRV 1.85.0 toolchains
and wasm target. Its Ubuntu package versions are ordinary product build inputs,
not a numerical identity qualification.

Executed container checks confirmed UID 10001, no Docker socket, successful
public GitHub HTTPS, and refused HTTP connections to a LAN address, the Tailscale
CGNAT DNS address, and the bridge/host gateway. The initial network test detected
Tailscale accepting forwarded traffic ahead of Docker's `DOCKER-USER` chain.
The final setup inserts its bridge-specific guard first in `FORWARD`; the repeated
three-destination test passed. No shared chain was flushed. The image/container
has no published ports or host bind mounts and the network has IPv6 disabled.

Three local admission-hook tests passed: trusted push/manual admission, refusal of
PR/PR-target/other events, and wrong-repository refusal. `actionlint` 1.7.7 passed
for all three edited workflows. Shell syntax and `git diff --check` passed.

## End-to-end measurements

Pending execution on the published infrastructure branch. No speed improvement
or numerical certification is claimed until the actual runs below are recorded.
