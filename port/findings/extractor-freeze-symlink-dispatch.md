# Preserve discovered tool symlink names in the freezer

Independent review of PR #39 at `6f02c95d98` found that the freezing CLI's
`Path(binary).resolve()` dereferenced a discovered `ninja` symlink to a
tool-manager dispatcher. A dispatcher using its invocation name then rejected
the calls. The reviewer reproduced successful freezing with the earlier CLI and
failure at the reviewed head using an actual Ninja graph, without fabricated
query output.

Code `b9f5a76b58` uses an absolute path that preserves the discovered symlink
spelling. The freezer records and invokes that path; hashing it still reads the
target bytes. The same recorded absolute path remains bound to every recipe
operation. The prior guarded-capture PATH-substitution fix remains intact.

The end-to-end regression creates name-dispatching symlinks for all four recorded
tools. It executes the actual freezing CLI with absolute and relative PATH entries
and a shim directory containing spaces, checks their preserved absolute paths,
versions and target hashes, and verifies the actual Markdown dependency. The old
freezer fails the control; all 37 tests pass in WSL with the correction.

The independently reproduced `c1e8a467da` capture uses `/usr/bin/ninja` directly
and is unaffected. Its graph, manifest, inventory, dependency maps, all 1,801
unit records and historical evidence retain their existing bytes and recorded
runtime identities. No recapture or AST/pilot qualification is claimed here.
The bounded correction and preservation evidence is under
`port/extractor/generated/evidence/freeze-symlink/`; independent review and
graph/parent/phase-zero acceptance remain outstanding.
