# Initial live vector comparison (historical head)

[Hosted run 45387261468](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45387261468)
passed the complete Rust port job, including audited native build/CTest, unit validation,
existing live units and this unit's `compare --partial`. This is evidence for PR58 head
`f98485d7c1b28896e59a1bc8ed494e801b29bb36`, **not for later working-tree changes**.
The Actions PR checkout and embedded overlay revision was merge commit
`50dfd1ee6eec8e94fb819976cd63aba81a9db290`; the initial accepted rust base was
`eb4cd4724ff6f5fa2119c86248c1a4beb4847451`.

Ubuntu 24.04 x86_64, GCC 13.3.0-6ubuntu2~24.04.1, glibc 2.39, wxWidgets 3.2.4,
`-fno-fast-math -ffp-contract=off`, Rust 1.99.1. Native compiles used bounded `-j2`.
The real 915-file pinned reference export and every overlay compilation were audited.
No editor/WSL VM was started. Full provenance and the untouched unit report are copied from
`rust-port-difftest-evidence`; SHA256SUMS binds those files.

| Exact live result | Count |
| --- | ---: |
| Valid cases | 4,103 |
| Compared exactly | 3,173 |
| Deferred | 930 |
| Failed / unprocessed / process faults / extra replies | 0 / 0 / 0 / 0 |

The native oracle returned real results for every case, including all Rust deferrals.
The separate native refusal CTest passed 29 undefined-input controls with 29 following
successful calls. Rust unit tests and all portable wasm builds passed. This is partial
progress; no golden was checked or blessed and it is not vector wasm runtime evidence.

| Deferral at this head | Count | Actual owner |
| --- | ---: | --- |
| NaN result arithmetic sites | 687 | vector unit ui.p002 |
| General EuclideanNorm std::hypot | 112 | vector unit ui.p002 |
| KiROUND<double,int> | 88 | math header ui.p001 |
| scalar equals<double> | 21 | math header ui.p001 |
| RoundedHypot | 10 | math header ui.p001 |
| rescale<int64_t> | 6 | math source util ui.p001 |
| double stream formatting | 3 | vector unit ui.p002 |
| rescale<double> | 3 | math header ui.p001 |

Later changes add strict malformed-input controls, explicit sign ownership deferral,
precise NaN upstream call attribution, Rust ordering traits and 351 unary/SetAll raw-bit
cases. They require their own live evidence; the initial count is not reused to certify them.
Reference NaN results in this report are observations, not a runtime answer table.
