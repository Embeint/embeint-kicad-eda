# Port foundation

The Rust port develops on `rust`; port PRs target that branch. The implementation
plan is [rust/PORT_PLAN.md](../rust/PORT_PLAN.md) and contributor rules are in
[AGENTS.md](AGENTS.md). This directory's foundation pins references and prepares
dependencies. It does not certify any ported unit or complete the phase 0 gate.

## Pins

[pins.toml](pins.toml) is the canonical machine-readable record, schema version 1:

- `kicad.url` and `kicad.revision`: upstream reading/oracle baseline, full commit
  `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
- `fork.url` and `fork.initial_rust_revision`: provenance of the initial `rust`
  branch, `3cd2c9afc6d2749b7d3a79fd1deaa86ac51cb3a7`. This records its starting
  point; it does not restrict later port commits to that HEAD.
- `solidcore.url`, `revision` and `path`: west project source, full commit and
  repository-relative checkout location. `solidcraft_revision` records the exact
  `solidcraft:` commit trailer, `93d5492ef295a1124664d36e967f58a81a76d7d2`.
- `tools.west_version`: exact west development-tool version. Its dependency
  closure is pinned in [requirements-west.txt](tools/requirements-west.txt).

[west.yml](../west.yml) projects the solidcore pin into west's manifest. Bootstrap
rejects disagreement before `west update`, validates full SHA syntax, and verifies
the external HEAD and SolidCraft trailer afterward. A pin upgrade changes both
files in one reviewed PR, with renewed dependency and licence checks.
KiCad and the fork are provenance records, not additional west clones.

## Bootstrap and build

Prerequisites: Python 3.11 or newer with `venv`/pip, Git, and the Rust toolchain
(workspace minimum 1.85). Ubuntu may need its distribution's `python3-venv` package.
Run from the repository root on Windows, WSL or a hosted Linux runner:

```text
python port/tools/bootstrap_west.py
python port/tools/bootstrap_west.py --check
cargo build --locked --manifest-path rust/Cargo.toml
```

Use `python3` where that names Python 3. Bootstrap installs west and its exact
dependencies into an isolated venv, configures this repository as west's topdir,
runs `west update`, then checks the external origin, cleanliness, commit and
trailer. The offline `--check` mode performs validation only and fails if setup is
missing or differs. Neither mode changes this repository's branch or root Git
configuration. An existing incompatible `.west/config` or dirty external checkout
is refused so local work is preserved.

Bootstrap's child processes use Git's `core.autocrlf=input` and
`core.filemode=false` overrides: existing CRLF checkouts remain clean on WSL, new
checkouts use LF, and host permission differences do not become source edits.
These overrides are process-local; they do not write Git configuration. Offline
checks also disable Git's optional index refresh writes. Use bootstrap when
switching between Windows and WSL on one checkout; a manual west invocation uses
that host's own Git settings.

For subsequent manual updates, use the installed tool from the repository root:

```powershell
.port-venv/windows/Scripts/west.exe update
python port/tools/bootstrap_west.py --check
```

```sh
.port-venv/linux/bin/west update
python3 port/tools/bootstrap_west.py --check
```

macOS uses `.port-venv/darwin/bin/west`. Windows and WSL use separate venvs even
when they share a checkout. Activation is optional; west is never installed
globally by bootstrap. Keep west commands in this repository; plain `west init -l
.` would choose its parent as topdir and mix dependencies between worktrees.

External sources live at `.port-deps/solidcore`, ignored with `.west/` and
`.port-venv/`. The existing Cargo workspace stays at `rust/`; solidcore retains its
own workspace and inherited package/lint settings. Its kernel is registered in
`rust/Cargo.toml` as a workspace path dependency with default features disabled.
It is not a member of our workspace. No pilot maths is routed through solidcore;
future STEP/3D consumers can opt into `solidcore.workspace = true`.

To verify the external portable library itself, independently of consumers:

```text
cargo check --locked --manifest-path .port-deps/solidcore/Cargo.toml -p solidcore -j8
rustup target add wasm32-unknown-unknown
cargo check --locked --manifest-path .port-deps/solidcore/Cargo.toml -p solidcore --target wasm32-unknown-unknown -j8
```

On the shared WSL host, use the build lock and job cap in [AGENTS.md](AGENTS.md).
Keep build output on the Linux filesystem. Read [DEPENDENCIES.md](DEPENDENCIES.md)
for the licence and native-library review. No dependency sources or build output
are committed.
