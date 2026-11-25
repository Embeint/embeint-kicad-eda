# SPDX-License-Identifier: GPL-3.0-or-later
"""Verify the host-installed admission hook before any workflow steps run."""
import os
from pathlib import Path
import subprocess
import unittest


class Admission(unittest.TestCase):
    def check_event(self, event, repository, expected):
        env = dict(os.environ, GITHUB_EVENT_NAME=event, GITHUB_REPOSITORY=repository)
        result = subprocess.run(["bash", str(Path(__file__).with_name("job-started.sh"))],
                                env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)

    def test_trusted_push_and_manual(self):
        for event in ("push", "workflow_dispatch"):
            self.check_event(event, "Embeint/embeint-kicad-eda", 0)

    def test_pull_requests_and_other_events_refused(self):
        for event in ("pull_request", "pull_request_target", "workflow_run", "", "schedule"):
            self.check_event(event, "Embeint/embeint-kicad-eda", 1)

    def test_wrong_repository_refused(self):
        for event in ("push", "workflow_dispatch"):
            self.check_event(event, "outsider/embeint-kicad-eda", 1)


if __name__ == "__main__":
    unittest.main()
