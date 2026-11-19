# SPDX-License-Identifier: GPL-3.0-or-later
"""Pick the schematic and board the end-to-end tests may edit, and refuse anything else.

The tests make real (unsaved) edits and change page settings.  They run only against the test
project that run_kicad_wsl.sh and run_kicad_windows.ps1 create (live-demo) or the one named in KICAD_LIVE_TEST_PROJECT, so a
design someone has open in the development KiCad is never touched.
"""

import os

from client import board_document, document


def allowed_project():
    return os.environ.get("KICAD_LIVE_TEST_PROJECT", "live-demo")


def refuse(kind, documents):
    open_projects = sorted({spec.get("project", {}).get("name", "?") for spec in documents}) or ["none"]
    raise SystemExit(f"REFUSED: the open {kind} is not the test project '{allowed_project()}' "
                     f"(open: {', '.join(open_projects)}).  Start run_kicad_wsl.sh or run_kicad_windows.ps1 "
                     "without arguments, "
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


STOCK_FOOTPRINT = "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm"


def park_footprint_editor(client):
    """Open a stock footprint before a test deletes its scratch footprint library folder.

    The Footprint Editor watches the open footprint's library folder.  On Windows, deleting a
    watched folder makes the watcher report a modal error, which blocks the API until someone
    dismisses it.  Opening another (saved, unmodified) footprint moves the watch away.
    """
    try:
        client.edit_footprint(STOCK_FOOTPRINT)
    except Exception as error:  # noqa: BLE001 - cleanup continues regardless
        print("cleanup:", error)
