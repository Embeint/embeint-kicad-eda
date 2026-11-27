<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Preflight routing correction

The sole `kicad-ci-isolated-linux-x64` runner was offline while the repository
flag remained enabled, so automatic preflights on four current port PR heads
queued indefinitely. [PR #97](https://github.com/Embeint/embeint-kicad-eda/pull/97)
makes pushes and ordinary manual runs use hosted Ubuntu 24.04. The isolated
cache runner requires explicit manual opt-in and the existing enable flag;
`use_hosted` still overrides that choice. Concurrency groups follow the selected
runner. Repository settings, admission/source pinning and check steps are intact.

Fresh independent Sol 6.1 review found no routing/admission objection. Its one
P3 stale trust-model description was corrected in a docs-only follow-up. All
eight Linux admission/resolver tests, checksum-verified actionlint 1.7.12 and
whitespace checks pass. Automatic push run 45913906805 actually started on a
GitHub-hosted Ubuntu runner at reviewed code head `054d5b258e`; final published
head `eea5e2c142105a5c0b8cff300becbdba5ef12bea` changes only that documentation.

The four stuck runs on #92/#93/#95/#96 were cancelled and replaced with hosted
dispatches at each current head. All four were observed running on GitHub Ubuntu,
not awaiting the offline runner. [summary.json](summary.json) retains run IDs,
heads, cancellation IDs and the observed routing/status snapshot. Their results
and PR #97's final-head checks remain pending at this recording. Only the CI
routing fix can become ready; port source findings still block those draft PRs.
