<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Product CI on existing Proxmox build runners

The product MCP/layout workflows can opt into the repository-scoped
`proxmox-kicad-isolated-01` runner, with the single custom label
`kicad-ci-isolated-linux-x64`. It has no default labels, so unrelated
`self-hosted` jobs cannot accidentally select it. This is an additional isolated
container on an existing build VM, not a replacement registration or a new VM.

Set repository Actions variable `KICAD_CI_RUNNER_ENABLED=true` only after verifying
that the runner is online and its isolation tests pass. An absent/false variable
routes all jobs to GitHub-hosted Ubuntu. Pull requests (including same-repository
PRs) always select hosted runners. Push and manually dispatched product jobs can
select the isolated runner. Do not use `pull_request_target` to execute PR code.
The existing `all_external_contributors` approval policy must remain enabled.

## Boundaries

The container runs as UID 10001 with all Linux capabilities dropped,
`no-new-privileges`, 2 CPU equivalents, 2560 MiB RAM with no additional swap, and
512 PIDs. It publishes no ports, has no Docker socket, sudo, host paths, SSH keys,
Tailscale identity, registry credentials or deployment access. Two dedicated Docker
volumes hold runner registration/workspaces and its home/tool/build caches.
The existing host runner keeps its registration, account, configuration and jobs.

A dedicated IPv4 bridge blocks RFC1918, CGNAT/Tailscale, link-local, loopback,
reserved and multicast destinations and all connections to the host itself.
IPv6 is disabled on that network. Public DNS/HTTPS remain available for GitHub and
verified tool/package downloads. Rules affect only this bridge. The runner has no
Docker restart policy; its systemd service reapplies the firewall before every
start, including after reboot. If the service/rules fail, leave routing disabled.

The image's root-owned pre-job hook rejects PR events and wrong repositories
before workflow steps. This is defense in depth, not an authorization system for
hostile workflow authors. Repository writers and operators remain trusted; Docker
shares the build VM's kernel. Do not approve external changes that route arbitrary
PR code here. GitHub recommends hosted runners for public PRs; see
[secure use](https://docs.github.com/en/actions/reference/security/secure-use) and
[runner hooks](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/run-scripts).

## Cache and numerical policies

Only the ordinary product MCP/layout jobs use persistent Cargo registries,
toolchains, Python downloads and per-job/toolchain Cargo target directories. The
checkout still cleans source files and does not persist its GitHub credentials.
Cargo rebuilds changed inputs, and every original test, lint, minimum Rust version,
WASM build, fixture and report step still runs. Hosted jobs retain their existing
Actions caches. MCP/layout fixtures and floating-point comparisons are unchanged.

This runner is **not** the certified numerical reference environment. Do not route
`rust-port`, reference-image candidates, migrations or clean/rebuild qualification
here through this label or these caches. Their exact cases, required profiles,
source binding, compiler/runtime/SSE2 audits, no compiled-cache policy and golden
approval rules remain with their owning workflows. The Ubuntu 24.04 container
alone does not establish a reference identity. The open reference-environment and
numerical-unit PRs are independent and are not incorporated by this change.

## Install or recover

Use an authorized root operator on an **existing build VM**, never the AgentFleet
deployment runner. Inventory active jobs/resources first. Build this directory as
a local image (no registry push needed), install the scripts root-owned at
`/opt/kicad-ci`, and pipe the short-lived repository registration token into
`sudo bash /opt/kicad-ci/install.sh <local-image>`. Obtain the token with
`POST repos/Embeint/embeint-kicad-eda/actions/runners/registration-token` on the
authenticated workstation; forward stdout directly through SSH. Never print or
store the token. An existing container is refused rather than replaced.

Install `kicad-ci.service` in `/etc/systemd/system`, reload systemd, then enable and
start **that service only**. Check the actual repository runner API, container user,
mounts/resource limits, private-network refusal and public HTTPS before opting in.
Run `python3 -m unittest discover -s tools/ci/proxmox-runner -v` and shell syntax
checks before updating installed scripts. Runner 2.338.2 is checksum-pinned from
the repository runner-download API. Automatic runner updates are disabled: update
the pin/checksum and recreate the idle container before GitHub's runner version
expiry. Preserve the named volumes; do not run Docker-wide prune commands.

For rollback, disable `KICAD_CI_RUNNER_ENABLED` first. Let active KiCad work finish,
then stop/disable `kicad-ci.service`. Disabling the variable affects newly
scheduled jobs; cancel/re-dispatch only this task's queued manual runs if needed.
Retain volumes for inspection. No other runner needs restarting. Remove the
repository registration only when retiring this runner. Numerical routing remains
unchanged throughout.

Measured installation details and run evidence are in [validation.md](validation.md).
