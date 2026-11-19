# SPDX-License-Identifier: GPL-3.0-or-later
"""Read-only startup diagnostics. The default path needs only Python's stdlib."""

import argparse
import importlib
import json
import os
from pathlib import Path
import sys

from instance_discovery import (NO_EDITOR_ACTION, DiscoveryError, discover_instances, is_endpoint,
                                socket_entry)


SCOPES = {
    "linux": "Linux/WSL local filesystem IPC (Unix sockets) only",
    "win32": "Windows local IPC (named pipes on this machine) only",
    "other": "Default local IPC endpoint only; automatic discovery covers Linux/WSL and Windows",
}
PROBE_PLATFORMS = ("linux", "win32")


def runtime_status():
    """Check this interpreter and this checkout, including absent generated code."""
    bundled = bool(getattr(sys, "frozen", False))
    here = Path(__file__).resolve().parent
    expected = []
    if not bundled:
        source = here.parents[1] / "api" / "proto"
        expected = [here / "_generated" / path.relative_to(source).with_name(path.stem + "_pb2.py")
                    for path in sorted(source.rglob("*.proto"))]
    missing = [path.relative_to(here).as_posix() for path in expected if not path.is_file()]
    dependencies = {}
    dependency_actions = {}
    for name in ("google.protobuf", "pynng", "mcp.server.fastmcp", "shapely", "numpy"):
        try:
            importlib.import_module(name)
            dependencies[name] = "available"
        except Exception as error:
            dependencies[name] = "unavailable"
            dependency_actions[name] = ("Reinstall the Embeint KiCad MCP package and check its native "
                                        "shared-library dependencies." if bundled else
                                        "Install requirements.txt with the MCP interpreter and "
                                        "check the package's native shared-library dependencies.")
            if name == "pynng" and sys.platform == "linux" and "libatomic.so.1" in str(error):
                dependency_actions[name] = ("Install libatomic1 on Debian/Ubuntu to provide "
                                            "libatomic.so.1, then rerun with the MCP interpreter.")
    if not bundled and (missing or not expected):
        protocols = {"state": "missing", "missing_modules": missing,
                     "action": "Run this interpreter with generate_proto.py from this checkout."}
    elif "unavailable" in dependencies.values():
        protocols = {"state": "unchecked", "action": "Resolve dependency_actions with this interpreter."}
    else:
        try:
            importlib.import_module("client")
            protocols = {"state": "ready"}
            if bundled:
                protocols["source"] = "bundled modules"
        except Exception:
            # Import exceptions can contain paths or arbitrary dependency text. Do not echo them.
            protocols = {"state": "incompatible", "action":
                         "Reinstall the Embeint KiCad MCP package." if bundled else
                         "Regenerate protocols with generate_proto.py and install requirements.txt "
                         "using this interpreter."}
    return {"dependencies": dependencies, "dependency_actions": dependency_actions, "protocols": protocols}


def _redact(value, secrets):
    if isinstance(value, str):
        for secret in sorted(secrets, key=len, reverse=True):
            if secret:
                value = value.replace(secret, "[redacted]")
        return value
    if isinstance(value, list):
        return [_redact(item, secrets) for item in value]
    if isinstance(value, dict):
        return {key: _redact(item, secrets) for key, item in value.items()}
    return value


def _probe_instance(url, timeout_ms, token, secrets):
    """Observe only these fixed reads; never take a command name from the caller.

    Each request has its own bounded connection. There are no application retries
    or fallback endpoints. Tokens stay in memory and are excluded from the report.
    """
    import pynng
    from google.protobuf import json_format
    from google.protobuf.message import DecodeError
    from client import base, editor, envelope, types

    if not is_endpoint(socket_entry(url)):
        return {"state": "not_probed", "action": "The endpoint must be an existing local socket "
                "file (Linux/WSL) or named pipe (Windows)."}
    requests = [(base.GetVersion(), base.GetVersionResponse)]
    requests.extend((editor.GetOpenDocuments(type=kind), editor.GetOpenDocumentsResponse)
                    for kind in (types.DOCTYPE_SCHEMATIC, types.DOCTYPE_PCB,
                                 types.DOCTYPE_SYMBOL, types.DOCTYPE_FOOTPRINT))
    result = {"state": "responding", "commands": []}
    pinned_token = token
    for command, response_type in requests:
        entry = {"command": command.DESCRIPTOR.full_name}
        if isinstance(command, editor.GetOpenDocuments):
            entry["document_type"] = types.DocumentType.Name(command.type)
        result["commands"].append(entry)
        request = envelope.ApiRequest()
        request.header.client_name = "embeint.mcp-read-only-diagnostic"
        request.header.kicad_token = pinned_token
        request.message.Pack(command)
        try:
            # NNG_DURATION_INFINITE disables resends, including after a peer disconnects.
            with pynng.Req0(send_timeout=timeout_ms, recv_timeout=timeout_ms,
                            resend_time=-1) as conn:
                conn.dial(url, block=False)
                conn.send(request.SerializeToString())
                raw = conn.recv()
            reply = envelope.ApiResponse.FromString(raw)
        except pynng.Timeout:
            entry["status"] = "timeout"
            result.update(state="unresponsive", action="Socket may be stale, busy or inaccessible. "
                          "Check the intended KiCad instance and IPC preferences; no socket was removed.")
            break
        except pynng.NNGException:
            entry["status"] = "transport_error"
            result.update(state="unreachable", action="Check socket permissions and the intended "
                          "KiCad instance. No socket was removed.")
            break
        except (DecodeError, RuntimeError):
            # pynng can raise RuntimeError on an empty received frame.
            entry["status"] = "invalid_envelope"
            result["state"] = "incompatible_response"
            break
        observed_token = reply.header.kicad_token
        secrets.add(observed_token)
        if pinned_token and observed_token and pinned_token != observed_token:
            entry["status"] = "instance_changed_or_token_mismatch"
            result.update(state="identity_mismatch", action="Check KICAD_API_SOCKET and "
                          "KICAD_API_TOKEN for the intended instance. No retry was attempted.")
            break
        pinned_token = pinned_token or observed_token
        status = reply.status.status
        if status not in envelope.ApiStatusCode.values() or status == envelope.AS_UNKNOWN:
            entry["status"] = "invalid_status"
            result["state"] = "incompatible_response"
            break
        entry["status"] = envelope.ApiStatusCode.Name(status)
        if status != envelope.AS_OK:
            entry["support"] = ("editor_not_open_or_unsupported" if status == envelope.AS_UNHANDLED
                                else "unsupported" if status == envelope.AS_UNIMPLEMENTED else "unverified")
            entry["action"] = "Check the Embeint build and open editor; version alone cannot establish support."
            if status not in (envelope.AS_UNHANDLED, envelope.AS_UNIMPLEMENTED):
                break
            continue
        response = response_type()
        try:
            unpacked = reply.message.Unpack(response)
        except DecodeError:
            unpacked = False
        if not unpacked:
            entry["status"] = "unexpected_reply_type"
            result["state"] = "incompatible_response"
            break
        entry["support"] = "observed_for_this_read_only_request"
        entry["response"] = json_format.MessageToDict(response, preserving_proto_field_name=True)
    return result


def diagnose(*, probe=False, socket_url=None, timeout_ms=500):
    if not 1 <= timeout_ms <= 5000:
        raise ValueError("timeout_ms must be between 1 and 5000")
    secrets = {os.environ.get("KICAD_API_TOKEN", "")}
    report = {"read_only": True, "connection_attempted": False,
              "live_support_verified": False, "platform": sys.platform,
              "scope": SCOPES.get(sys.platform, SCOPES["other"]),
              "capability_limit": "Only listed read-only requests are probed. No mutation support "
              "or negotiated capability set is inferred from an application version.",
              "runtime": runtime_status()}
    if report["runtime"]["protocols"]["state"] == "ready":
        # Server and packaged application versions (support imports the protocol modules)
        from support import version_identity
        report["mcp"] = version_identity()
    try:
        inventory = discover_instances(socket_url)
        report["discovery"] = inventory
    except DiscoveryError as error:
        report["discovery"] = {"selection": error.code, "action": str(error)}
        return _redact(report, secrets)
    report["discovery"]["action"] = (
        "Set KICAD_API_SOCKET to the intended candidate before editor operations. "
        "Use --probe to identify responding candidates; stale sockets are never removed."
        if inventory["selection"] in ("ambiguous", "unavailable") else
        NO_EDITOR_ACTION + " Use --probe for bounded read-only checks."
        if inventory["selection"] == "default_missing" else
        "Use --probe for bounded read-only checks.")
    if probe:
        if sys.platform not in PROBE_PLATFORMS:
            report["probe"] = {"state": "unsupported_platform",
                               "action": "Run opt-in probes on Windows or inside Linux/WSL."}
        elif report["runtime"]["protocols"]["state"] != "ready":
            report["probe"] = {"state": "runtime_unavailable", "action": "Fix runtime diagnostics first."}
        else:
            for candidate in inventory["candidates"]:
                if is_endpoint(candidate):
                    report["connection_attempted"] = True
                    token = os.environ.get("KICAD_API_TOKEN", "") if inventory["selection"] == "explicit" else ""
                    candidate["probe"] = _probe_instance(candidate["socket_url"], timeout_ms, token, secrets)
            if not report["connection_attempted"]:
                report["probe"] = {"state": "no_socket", "action": NO_EDITOR_ACTION}
    return _redact(report, secrets)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", action="store_true", help="Opt in to version/open-document reads only")
    parser.add_argument("--socket", help="Explicit local IPC URL, ipc:///path (Linux/WSL) or "
                        r"ipc://C:\...\api.sock (Windows); overrides the environment")
    parser.add_argument("--timeout-ms", type=int, default=500, help="Per send/receive timeout, 1..5000")
    args = parser.parse_args()
    if not 1 <= args.timeout_ms <= 5000:
        parser.error("--timeout-ms must be between 1 and 5000")
    print(json.dumps(diagnose(probe=args.probe, socket_url=args.socket, timeout_ms=args.timeout_ms),
                     indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
