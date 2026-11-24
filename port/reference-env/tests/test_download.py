# SPDX-License-Identifier: GPL-3.0-or-later
"""Locked downloads retain their trust boundary through transient service failures."""

import hashlib
import io
from pathlib import Path, PureWindowsPath
import sys
import tempfile
import unittest
from unittest import mock
from urllib.error import HTTPError

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import refenv  # noqa: E402
import fetch_inputs  # noqa: E402


class DownloadTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.target = Path(self.directory.name) / "locked-input"
        self.data = b"the locked input"
        self.digest = hashlib.sha256(self.data).hexdigest()
        self.sleep = mock.patch.object(refenv.time, "sleep").start()
        self.addCleanup(mock.patch.stopall)

    def test_verified_cache_needs_no_network(self):
        self.target.write_bytes(self.data)
        with mock.patch.object(refenv.urllib.request, "urlopen") as fetch:
            self.assertFalse(refenv.download("https://primary.invalid/input", self.target, self.digest))
        fetch.assert_not_called()

    def test_index_fallback_urls_remain_posix_on_windows(self):
        class ReachedIndex(Exception):
            pass

        sha = "0" * 64
        lock = {"snapshot": {"url": "https://snapshot.ubuntu.com/ubuntu",
                             "id": "20261120T000000Z", "suites": {
                                 "noble": {"inrelease_sha256": sha, "indices": {
                                     "main": {"path": "main/binary-amd64/Packages.gz", "sha256": sha}
                                 }}
                             }}}
        with mock.patch.object(fetch_inputs, "Path", PureWindowsPath), \
                mock.patch.object(fetch_inputs, "stage", side_effect=[None, ReachedIndex]) as stage:
            with self.assertRaises(ReachedIndex):
                fetch_inputs.fetch_inputs(lock, Path(self.directory.name) / "cache",
                                          Path(self.directory.name) / "stage")
        alternate = stage.call_args.args[5][0]
        self.assertEqual(alternate, "https://snapshot.ubuntu.com/ubuntu/20261120T000000Z/"
                         "dists/noble/main/binary-amd64/by-hash/SHA256/" + sha)
        self.assertNotIn("\\", alternate)

    def test_transient_retry_then_atomic_verified_replacement(self):
        self.target.write_bytes(b"previous bytes")
        with mock.patch.object(refenv.urllib.request, "urlopen", side_effect=[
            HTTPError("https://primary.invalid/input", 503, "unavailable", {}, None), io.BytesIO(self.data)
        ]) as fetch:
            self.assertTrue(refenv.download("https://primary.invalid/input", self.target, self.digest))
        self.assertEqual(fetch.call_count, 2)
        self.sleep.assert_called_once_with(2)
        self.assertEqual(self.target.read_bytes(), self.data)
        self.assertFalse(self.target.with_suffix(".partial").exists())

    def test_interrupted_stream_is_restarted_without_retaining_partial_bytes(self):
        broken = mock.MagicMock()
        broken.__enter__.return_value = broken
        broken.read.side_effect = [b"partial", refenv.http.client.IncompleteRead(b"truncated")]
        with mock.patch.object(refenv.urllib.request, "urlopen", side_effect=[
            broken, io.BytesIO(self.data)
        ]) as fetch:
            self.assertTrue(refenv.download("https://primary.invalid/input", self.target, self.digest))
        self.assertEqual(fetch.call_count, 2)
        self.assertEqual(self.target.read_bytes(), self.data)
        self.assertFalse(self.target.with_suffix(".partial").exists())

    def test_fallback_must_produce_the_same_locked_hash(self):
        for payload in (self.data, b"different bytes"):
            with self.subTest(payload=payload):
                self.target.unlink(missing_ok=True)
                responses = [HTTPError("https://primary.invalid/input", 502, "bad gateway", {}, None)
                             for _ in range(3)] + [io.BytesIO(payload)]
                with mock.patch.object(refenv.urllib.request, "urlopen", side_effect=responses) as fetch:
                    if payload == self.data:
                        self.assertTrue(refenv.download("https://primary.invalid/input", self.target, self.digest, ("https://alternate.invalid/input",)))
                        self.assertEqual(self.target.read_bytes(), self.data)
                    else:
                        with self.assertRaises(ValueError):
                            refenv.download("https://primary.invalid/input", self.target, self.digest, ("https://alternate.invalid/input",))
                        self.assertFalse(self.target.exists())
                self.assertEqual([c.args[0].full_url for c in fetch.call_args_list],
                                 ["https://primary.invalid/input"] * 3 + ["https://alternate.invalid/input"])
                self.assertFalse(self.target.with_suffix(".partial").exists())

    def test_hash_mismatch_and_permanent_error_never_use_alternate(self):
        self.target.write_bytes(b"original destination")
        for failure in (io.BytesIO(b"wrong bytes"), HTTPError("https://primary.invalid/input", 404, "missing", {}, None)):
            with mock.patch.object(refenv.urllib.request, "urlopen", side_effect=[failure]) as fetch:
                with self.assertRaises((ValueError, HTTPError)):
                    refenv.download("https://primary.invalid/input", self.target, self.digest, ("https://alternate.invalid/input",))
                self.assertEqual(fetch.call_count, 1)
            self.assertEqual(self.target.read_bytes(), b"original destination")
            self.assertFalse(self.target.with_suffix(".partial").exists())

    def test_exhausted_retries_leave_no_output_or_partial(self):
        with mock.patch.object(refenv.urllib.request, "urlopen", side_effect=
                               HTTPError("https://primary.invalid/input", 502, "bad gateway", {}, None)) as fetch:
            with self.assertRaises(HTTPError):
                refenv.download("https://primary.invalid/input", self.target, self.digest)
            self.assertEqual(fetch.call_count, 3)
        self.assertFalse(self.target.exists())
        self.assertFalse(self.target.with_suffix(".partial").exists())


if __name__ == "__main__":
    unittest.main()
