# SPDX-License-Identifier: GPL-3.0-or-later
"""Read-only P1.2 qualification, manually run against an already-open isolated test PCB.

Requires KICAD_P12_ISOLATED=1 and an explicit KICAD_API_SOCKET. The operator must
start a separate app/config with a disposable project; this script never launches,
opens, replaces or closes an editor. KICAD_LIVE_TEST_PROJECT defaults to live-demo.
Set KICAD_P12_EXPECT_FILE to a fixture's embedded filename to require populated coverage.
Run with the same interpreter used for generate_proto.py. No native parsing is tested.
"""

from hashlib import sha256
import os
from pathlib import Path

from google.protobuf import json_format

from client import LiveSchematic, as_dict, base, board_document
from common.types import embedded_files_pb2 as embedded
from e2e_guard import allowed_project, refuse


def snapshot(client, target):
    payload = {"document": as_dict(target)}
    return (client.execute("SaveDocumentToString", payload)["result"],
            client.execute("GetDocumentModifiedState", payload)["result"])


def main():
    if os.environ.get("KICAD_P12_ISOLATED") != "1" or not os.environ.get("KICAD_API_SOCKET"):
        raise SystemExit("REFUSED: explicitly select an isolated test app with "
                         "KICAD_P12_ISOLATED=1 and KICAD_API_SOCKET")
    client = LiveSchematic()
    boards = client.boards()
    candidates = [spec for spec in boards if spec.get("project", {}).get("name") == allowed_project()]
    if len(candidates) != 1:
        refuse("unique PCB", boards)
    target = board_document(candidates[0])
    board_file = Path(target.project.path) / target.board_filename
    # No disk writes: require an existing disposable fixture for a meaningful hash check.
    on_disk = sha256(board_file.read_bytes()).hexdigest()
    before = snapshot(client, target)
    payload = {"board": as_dict(target)}
    result = client.execute("GetEmbeddedFiles", payload)["result"]
    files = json_format.ParseDict(result, embedded.EmbeddedFiles())
    expected = os.environ.get("KICAD_P12_EXPECT_FILE")
    if expected:
        assert expected in {file.name for file in files.files}, "expected embedded fixture not returned"
    assert client.execute("GetEmbeddedFiles", payload)["result"] == result
    batch = client.batch(target, [{"command": "GetEmbeddedFiles"}])
    assert batch["steps"][0]["result"] == result

    for bad in ({}, {"type": "DOCTYPE_SCHEMATIC"},
                {**as_dict(target), "board_filename": "p12-not-open.kicad_pcb"}):
        try:
            client.execute("GetEmbeddedFiles", {"board": bad})
        except RuntimeError as error:
            expected_status = "AS_BAD_REQUEST" if bad.get("board_filename") else "AS_UNHANDLED"
            assert expected_status in str(error), str(error)
        else:
            raise AssertionError("native handler accepted a missing/wrong document target")

    assert snapshot(client, target) == before, "read changed live contents or modified state"
    assert sha256(board_file.read_bytes()).hexdigest() == on_disk, "read changed the board on disk"
    version = as_dict(client.send(base.GetVersion(), base.GetVersionResponse))
    print(f"ok: embedded read, batch, targeting, live-state and disk invariance; version={version}")
    print(f"embedded files returned: {len(files.files)}")
    if not files.files:
        print("pending: populated embedded-file fixture coverage (empty board only)")


if __name__ == "__main__":
    main()
