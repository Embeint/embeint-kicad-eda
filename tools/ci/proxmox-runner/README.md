<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Product and Rust developer CI on existing Proxmox build runners

The product MCP contracts and Rust layout workflows can opt into the repository-scoped
`proxmox-kicad-isolated-01` runner, with the single custom label
`kicad-ci-isolated-linux-x64`. It has no default labels, so unrelated
`self-hosted` jobs cannot accidentally select it. This is an additional isolated
container on an existing build VM, not a replacement registration or a new VM.

Set repository Actions variable `KICAD_CI_RUNNER_ENABLED=true` only after verifying
that the runner is online and its isolation tests pass. An absent/false variable
routes all jobs to GitHub-hosted Ubuntu. Pull requests (including same-repository
PRs) always select hosted runners. Push and manually dispatched product jobs can
select the isolated runner. Do not use `pull_request_target` to execute PR code.
The long-running Rust MCP protocol parity workflow stays hosted so that its
roughly 20 minutes of fixture waits cannot monopolize the single isolated worker.
Its tests/cache policy are unchanged. The existing `all_external_contributors`
approval policy must remain enabled.

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
start, including after reboot. The image enables the runner's built-in PID 1
signal forwarding so an idle stop releases its GitHub session promptly. If the service/rules fail, leave routing disabled.

The image's root-owned pre-job hook rejects PR events and wrong repositories
before workflow steps. This is defense in depth, not an authorization system for
hostile workflow authors. Repository writers and operators remain trusted; Docker
shares the build VM's kernel. Do not approve external changes that route arbitrary
PR code here. GitHub recommends hosted runners for public PRs; see
[secure use](https://docs.github.com/en/actions/reference/security/secure-use) and
[runner hooks](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/run-scripts).

## Cache and numerical policies

Only ordinary MCP contracts and Rust layout jobs use persistent Cargo registries,
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

## Rust-port developer feedback

`rust-port-preflight.yml` adds ordinary cached developer checks on pushes to
`rust` and repository writers' `t3/**` branches. Pushes and ordinary manual runs
use GitHub Ubuntu 24.04, so an offline isolated runner cannot strand automatic
feedback. A manual `use_isolated=true` run uses the cached isolated runner only
when `KICAD_CI_RUNNER_ENABLED=true`; verify the runner is online before opting in.
The existing manual `use_hosted=true` option forces GitHub even if both options
are selected. No repository settings need changing. The installed admission hook remains unchanged:
PR events and other repositories are refused. No Docker access, sudo, additional
memory, private-network access or runner registration is added.

The preflight checks the full workspace's formatting, clippy and tests, builds
every portable package selected by the existing policy tool for wasm32, and on
hosted runs validates unit/source accounting. `validate_units.py` loads the
2.4 GB decompressed call graph, more than the isolated runner's 2.5 GiB
swapless limit, and gains nothing from the Rust cache; on Proxmox the step is
recorded as deferred to the hosted `Rust port progress pages` workflow, which
runs it on every `rust` PR and push. Commands use the pinned Rust toolchain and at
most two build jobs. A separate ordinary target cache lives under
`$HOME/build-cache/rust-port-preflight-<toolchain>`; incremental compilation is
disabled to bound its size. Existing MCP/layout caches are separate. Every check
still executes; no native oracle, golden or clean image/rebuild qualification
uses these artifacts. This supplemental feedback does not satisfy or replace
the hosted `rust-port`, numerical or platform-package checks.

Pushes check their exact event SHA even if the branch subsequently moves. A
manual run may select another existing `rust`/`t3/` branch in this repository;
its SHA is resolved once before checkout and recorded separately from the
workflow revision. Arbitrary SHAs, tags, external repositories, PR refs and
wildcard branch names are refused. This allows checking an existing port topic
before it has merged the workflow itself. The resulting manual check belongs to
the workflow run, not to the selected topic's required PR checks.

The runner is persistent, and every job on it shares one user and `$HOME`. The
per-workflow cache directories are separate paths, not separate trust domains.
A `t3/**` push runs that branch's build scripts and proc-macros, including any
new dependency in its `Cargo.lock`, and that code can write the shared
workspace, `~/.cargo`, `~/.rustup` and the other workflows' caches. So a writer's
topic branch can change the results of later `rust` or `main` jobs on this
runner. This is within the trust model (only repository writers' pushed
branches run here), and hosted checks remain authoritative for every PR. The
source resolver itself runs outside the leftover workspace, with Python isolated
mode and no inherited Git configuration, so it cannot pick up files from an
earlier job. An ephemeral runner, or a separate user for topic branches, would
remove this cross-job influence; neither is installed.

Read-only preflight artifacts record source identity, runner, cgroup limits,
tools and actual check logs. They explicitly say numerical qualification is
false. Per-branch and selected-runner preflight concurrency cancels superseded
developer runs; ordinary pushes and forced-hosted manual runs share a group. The
hosted Rust-port, Rust MCP and package workflows also cancel superseded PR
heads; tag releases, push qualification and manual migration runs retain their
existing non-cancelling behavior.

Run the source-resolution/admission regressions with:

```sh
python3 -B -m unittest discover -s tools/ci/proxmox-runner -p 'test_*.py' -v
```

The tests execute the actual inline resolver and verify event-SHA pinning,
manual branch resolution, wrong repository/event refusals and malformed or
ambiguous remote references. Hosted/native numerical policies remain unchanged.
