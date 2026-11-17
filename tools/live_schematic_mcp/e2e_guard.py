# SPDX-License-Identifier: GPL-3.0-or-later
"""Pick the schematic and board the end-to-end tests may edit, and refuse anything else.

The tests make real (unsaved) edits and change page settings.  They run only against the test
project that run_kicad_wsl.sh creates (live-demo) or the one named in KICAD_LIVE_TEST_PROJECT, so a
design someone has open in the development KiCad is never touched.
"""

import os

from client import board_document, document


def allowed_project():
    return os.environ.get("KICAD_LIVE_TEST_PROJECT", "live-demo")


def refuse(kind, documents):
    open_projects = sorted({spec.get("project", {}).get("name", "?") for spec in documents}) or ["none"]
    raise SystemExit(f"REFUSED: the open {kind} is not the test project '{allowed_project()}' "
                     f"(open: {', '.join(open_projects)}).  Start run_kicad_wsl.sh without arguments, "
                     "or set KICAD_LIVE_TEST_PROJECT to a project the tests may edit.")


def test_document(client):
    documents = client.documents()["documents"]
    if not documents:
        # KiCad running as the project manager: open the project's schematic first
        try:
            documents = client.open_schematic()["documents"]
        except RuntimeError:
            pass
    for spec in documents:
        if spec.get("project", {}).get("name") == allowed_project():
            return document(spec)
    refuse("schematic", documents)


def test_board(client):
    """The test project's board, opened (and created if new) in the PCB editor."""
    boards = client.boards()
    if not boards:
        test_document(client)  # never open a board in a project the tests may not edit
        boards = [client.open_board()]
    for spec in boards:
        if spec.get("project", {}).get("name") == allowed_project():
            return board_document(spec)
    refuse("board", boards)
