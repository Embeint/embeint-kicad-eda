# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise the source resolver actually shipped inside the trusted workflow."""
from pathlib import Path
import subprocess
import textwrap
import unittest
from unittest.mock import Mock, patch


WORKFLOW = Path(__file__).resolve().parents[3] / ".github/workflows/rust-port-preflight.yml"
SHA = "1ac339f229d8ccb8ccb297f489493622903cc468"


class TrustedSource(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        text = WORKFLOW.read_text()
        source = text.split("# BEGIN SOURCE RESOLVER\n", 1)[1].split(
            "# END SOURCE RESOLVER", 1)[0]
        cls.namespace = {}
        exec(compile(textwrap.dedent(source), str(WORKFLOW), "exec"), cls.namespace)

    def resolve(self, **overrides):
        arguments = dict(event="push", repository="Embeint/embeint-kicad-eda",
                         ref="refs/heads/t3/unit", sha=SHA)
        arguments.update(overrides)
        return self.namespace["trusted_source"](**arguments)

    def test_push_is_pinned_to_its_event_even_when_branch_advances(self):
        lookup = Mock(side_effect=AssertionError("push must not resolve current branch head"))
        self.assertEqual(self.resolve(lookup=lookup), ("t3/unit", SHA))
        lookup.assert_not_called()
        self.assertEqual(self.resolve(ref="refs/heads/rust"), ("rust", SHA))

    def test_manual_resolves_only_an_existing_trusted_repository_branch(self):
        lookup = Mock(return_value=SHA)
        self.assertEqual(self.resolve(event="workflow_dispatch", manual_branch="t3/util",
                                      lookup=lookup), ("t3/util", SHA))
        lookup.assert_called_once_with("t3/util")
        self.assertEqual(self.resolve(event="workflow_dispatch", lookup=lambda _: SHA),
                         ("t3/unit", SHA))

    def test_untrusted_events_repositories_refs_and_push_overrides_refuse(self):
        for overrides in (
            {"event": "pull_request"}, {"event": "pull_request_target"},
            {"event": "workflow_run"}, {"event": "schedule"},
            {"repository": "outsider/embeint-kicad-eda"},
            {"ref": "refs/tags/rust"}, {"ref": "refs/heads/main"},
            {"deleted": "true"}, {"manual_branch": "t3/override"},
            {"sha": "HEAD"}, {"sha": "f" * 39}, {"sha": SHA + "\n"},
        ):
            with self.subTest(overrides=overrides), self.assertRaises(ValueError):
                self.resolve(**overrides)

    def test_manual_branch_names_cannot_be_tags_shas_patterns_or_shell_text(self):
        for branch in ("main", SHA, "refs/heads/t3/a", "t3/*", "t3/a..b", "t3/a.lock",
                       "t3/a\nb", "t3/$(id)", "t3/../rust", "t3/a/"):
            with self.subTest(branch=branch), self.assertRaises((ValueError, subprocess.CalledProcessError)):
                self.resolve(event="workflow_dispatch", manual_branch=branch, lookup=lambda _: SHA)

    def test_remote_resolution_requires_one_exact_matching_ref_and_sha(self):
        remote = self.namespace["remote_sha"]
        with patch.object(self.namespace["subprocess"], "run") as run:
            run.return_value.stdout = SHA + "\trefs/heads/t3/unit\n"
            self.assertEqual(remote("t3/unit"), SHA)
            self.assertEqual(run.call_args.args[0][-1], "refs/heads/t3/unit")
            for output in ("", SHA + "\trefs/heads/t3/other\n", "HEAD\trefs/heads/t3/unit\n",
                           (SHA + "\trefs/heads/t3/unit\n") * 2):
                run.return_value.stdout = output
                with self.subTest(output=output), self.assertRaises(ValueError):
                    remote("t3/unit")


if __name__ == "__main__":
    unittest.main()
