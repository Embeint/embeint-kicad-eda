# SPDX-License-Identifier: GPL-3.0-or-later
"""One readiness probe for scripts/wait_ready.sh and scripts/wait_ready.ps1.

Asks the running KiCad for its version and open schematics once.  Exit status:
  0   ready: prints the documents, then "endpoint: <KICAD_API_SOCKET value>"
  75  not yet (EX_TEMPFAIL): no editor endpoint yet, still loading, busy, refused or timed out
  1   anything else: a real error the caller should report instead of waiting

The decision uses error codes (DiscoveryError.code, the API status name, pynng exception types),
not message text, so rewording a message cannot turn "not started yet" into "ready".
"""

import argparse
import sys

import pynng

from instance_discovery import DiscoveryError

READY, WAIT, ERROR = 0, 75, 1

# DiscoveryError codes meaning no editor answers yet: no endpoint at all, only endpoints that are
# not listening, or the pinned instance stopped listening.  "ambiguous" and the rest need a person.
WAIT_DISCOVERY_CODES = frozenset({"no_editor", "unavailable", "instance_unavailable"})

# API status codes KiCad answers with while it starts (AS_NOT_READY) or is otherwise occupied
WAIT_API_STATUSES = ("AS_NOT_READY", "AS_BUSY")

# Transport failures while the endpoint appears or the editor is still starting its server
WAIT_TRANSPORT_ERRORS = (pynng.Timeout, pynng.ConnectionRefused, pynng.ConnectionReset,
                         pynng.ConnectionAborted, pynng.Closed, pynng.TryAgain,
                         pynng.DestinationUnreachable, ConnectionError, FileNotFoundError)


def classify(error):
    """WAIT when error only means KiCad does not answer yet, otherwise ERROR."""
    if isinstance(error, DiscoveryError):
        return WAIT if error.code in WAIT_DISCOVERY_CODES else ERROR
    if isinstance(error, WAIT_TRANSPORT_ERRORS):
        return WAIT
    # LiveSchematic.send reports a non-OK reply as "<status name>: <message>"
    if isinstance(error, RuntimeError) and str(error).split(":", 1)[0] in WAIT_API_STATUSES:
        return WAIT
    return ERROR


def probe(timeout_ms=5000, client_factory=None):
    """(status, lines) for one attempt; never raises for editor or transport errors."""
    if client_factory is None:
        from client import LiveSchematic
        client_factory = LiveSchematic
    try:
        client = client_factory(timeout_ms=timeout_ms)
        documents = client.documents()
    except Exception as error:  # noqa: BLE001 - every failure is classified and reported
        code = getattr(error, "code", None)
        prefix = f"{type(error).__name__}({code})" if code else type(error).__name__
        return classify(error), [f"{prefix}: {error}"]
    return READY, [str(documents), f"endpoint: {client.socket_url}"]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--timeout-ms", type=int, default=5000, help="per-request timeout")
    args = parser.parse_args(argv)
    status, lines = probe(args.timeout_ms)
    print("\n".join(lines), file=sys.stdout if status == READY else sys.stderr)
    return status


if __name__ == "__main__":
    sys.exit(main())
