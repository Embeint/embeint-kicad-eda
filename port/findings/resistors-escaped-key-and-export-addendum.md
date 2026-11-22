# Resistor escaped-key coverage and export qualification

This corrects two evidence findings from independent review of PR #44 head
`19e8384772bba1d02fc07889335f6656a354892e`. The earlier proof, source manifest,
executables, 889 literal requests/replies and fixture-authoring script remain
unchanged. No portable algorithm or adapter runtime defect was reproduced.

The authoring script's `N_ESC`, `R_ESC` and `N_ALL_ESC` constants contain plain
keys. A raw-string prefix alone does not encode their characters, so the
claimed escaped-key and mixed-spelling duplicate cases repeated plain spellings.
The committed 889 requests contain no escaped marker object keys.

The new, separate 199-request fixture retains the independent review's literal
escaped-dollar, escaped-colon and fully Unicode-encoded keys, nonstring values,
numeric/JSON/overflow-looking strings, six mixed literal/escaped duplicate-key
refusals and a final successful recovery. Complete expected replies were captured
from the actual pinned C++ oracle and independently matched both Rust profiles.
The default CTest, Rust process replay and combined request script now include
this fixture. A literal generator and explicit raw-spelling assertions prevent
the same coverage error. The original 889 fixtures stay byte-identical, and the
original 960 combined requests remain an unchanged prefix of the 1,159 checks.

The earlier report's claim of an LF reference archive is also withdrawn. Its
recorded archive `c962971d0e4b42174ea5932756f8a358a403aa20e3e464ba5231b9fc48e9a06e`
contains 875 files matching CRLF conversions of pinned Git blobs, and 38 matching
raw Git bytes. All differences are line endings. The committed source manifest,
retained reference files and embedded executable provenance agree exactly; their
hash chain is intact. They are not relabelled as a raw-LF export. Setting
`core.autocrlf=false` alone did not override the repository's export attributes.

The separately recorded combined integration proof at `3731499b2b` uses
`core.autocrlf=false` and `core.eol=lf` for export and verifies all 915 reference
files against raw pinned Git blobs. Its resistor outputs equal the historical
golden. That is separate evidence, not a replacement manifest for the earlier
binary. GCC 13/15 Release x86-64 qualification remains; GCC O0, Clang 18 and other
architectures are not certified by these records.

No registered case, golden, baseline evidence, dependency pin, lockfile, portable
body or native adapter body changes. No reblessing or phase-zero acceptance is
implied; the resistor record remains claimed pending extractor acceptance.
