# Reviewed a7 extraction snapshot

These are historical bytes published at
`a7ec77beffd3b2e5d60409423fa28b6c116c6e00`, preserved before the review corrections.
The independent review manifests and clang probes describe that implementation;
their old results are not evidence that the corrections work.

Original raw evidence remains in `port/extractor/generated/raw/`. The original
reconciled AST graph is retained both here and at the root
`reconciled-extraction/`; the original canonical EOF-normalized graph is this
directory's `graph.json.gz`. Relative paths in historical reports describe their
original repository locations. The 1,570 original unit records remain available
in Git at the reviewed commit (unchanged by parent fix `77c144493f`).

Corrected extraction has a different parser/cache identity and additionally
inventories build-generated sources. Its coverage, classifications, ownership and
hashes must be read independently from the current root measurement. No old
result was relabelled as corrected.
