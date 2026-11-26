<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Map helpers hosted reference evidence

Addendum to `core-map-helpers-review-and-reference-addendum.md`, unit
`libs.core.include.core.application.thin.replacement.p001`, PR #78.

All 19 checks completed on corrected source head
`1cadc88eb8f37b884b6db5eb4d09da18c4523379`: 16 success and three
inapplicable skips. The dedicated map-helpers-live run 45732438430,
job 139617483587, artifact 15078975842 records:

- Ubuntu GCC 13.3.0, glibc 2.39, wxWidgets 3.2.4 in oracle provenance.
- 3,084 compared cases, zero deferrals, failures or unprocessed inputs.
- 336 identical refusals in each native process, each followed by a successful
  lookup.
- Import-free wasm replay of all fresh C++ replies passes under Node 22.23.5.

Oracle binary SHA-256:
`ceeeb1b1d29a401ecd838fda9758b90ef4900c51d8e37bc0b791670d6e850b3e`.
Both reply hashes are
`f7103e18b3969d9821cad7e364c16fbb085ced4cd05d065935d0d8185a7c4691`,
matching the earlier local run and unchanged baseline. Provenance JSON SHA-256:
`2f3365197eff6846eb23bfedbba3a00c6cd76cdb0e7f6c98d32ca11d1f3c45e9`.

This establishes the recorded corpus/control agreement on the target wx
version. It does not change the historical baseline evidence, whose local
binary was wx 3.2.9, or claim all-version source equivalence. Generic coverage,
the port-imposed string capacity fence and unsupported Windows wchar behavior
remain as documented.

The parent subsequently imported accepted extractor `4738a9c23f`. The
imported diff equals exactly the accepted extractor delta and its tree matches
the precomputed merge. Runtime code is unchanged; final integrated-head CI
must pass before acceptance. The unit remains partial.
