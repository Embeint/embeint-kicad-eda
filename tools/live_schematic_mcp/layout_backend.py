# SPDX-License-Identifier: GPL-3.0-or-later
"""Choose the symbol pin layout implementation: Python (authoritative) or the Rust port.

``symbol_layout.layout_box`` stays the reference.  The Rust port (``rust/embeint-layout``) has
the same JSON boundary and is measured to match it exactly (``rust/parity``); this module lets
the MCP use it, opt-in, without making it a dependency:

  EMBEINT_LAYOUT_BACKEND=python   the default: Python only, and the tool result is unchanged
  EMBEINT_LAYOUT_BACKEND=rust     the Rust runner lays out the pins; Python is the fallback
  EMBEINT_LAYOUT_BACKEND=shadow   Python lays out the pins and the Rust runner is run as well,
                                  so every real request checks parity; a difference is logged
                                  and reported, never used

  EMBEINT_LAYOUT_BIN              path of the ``embeint-layout`` runner.  When it is not set:
                                  next to a bundled MCP executable, then rust/target/release in
                                  this checkout, then the PATH directories

The runner is started for each call (one JSON line in, one out), so no child process outlives a
tool call.  Anything that stops the Rust side answering (no runner, a crash, a timeout, output that
is not a response, or a request outside its JSON boundary) falls back to Python, and the result
says so.  A request that JSON cannot carry unchanged (a tuple, a non-string key, NaN) never
reaches the runner; it is sent as ASCII, so a lone surrogate gets there and is refused.  A layout error from Rust (a bad pin list) is raised as the
same exception with Python's text, within the limits rust/README.md lists (repr() of unassigned
code points, the Unicode version of str.upper(), Python 3.14's "unhashable type" wording).
"""

import ast
import json
import logging
import os
import re
import subprocess
import sys
from pathlib import Path

from symbol_layout import layout_box

log = logging.getLogger("embeint.layout")

MODES = ("python", "rust", "shadow")
ENV_MODE = "EMBEINT_LAYOUT_BACKEND"
ENV_BIN = "EMBEINT_LAYOUT_BIN"
TIMEOUT_S = 10
RUNNER = "embeint-layout" + (".exe" if os.name == "nt" else "")
HERE = Path(__file__).resolve().parent

# Layout errors the Rust runner reports by Python exception name.  Anything else (RequestError, an
# unknown name) means the request did not get through the boundary, so Python handles it.
LAYOUT_ERRORS = {"ValueError": ValueError, "TypeError": TypeError, "KeyError": KeyError}


class RunnerUnavailable(Exception):
    """The Rust side gave no layout answer; the message is the reason for falling back."""


def mode():
    """The configured backend, and a warning when the setting is not one of MODES."""
    value = os.environ.get(ENV_MODE, "").strip().lower() or "python"
    if value in MODES:
        return value, None
    return "python", f"{ENV_MODE}={value!r} is not one of {', '.join(MODES)}; using python"


def configured_runner():
    """EMBEINT_LAYOUT_BIN without surrounding quotes, or "" when it is not set."""
    return os.environ.get(ENV_BIN, "").strip().strip("\"'").strip()


def find_runner():
    """Path of the embeint-layout runner, or None.  A set EMBEINT_LAYOUT_BIN is the only place."""
    configured = configured_runner()
    if configured:
        return Path(configured) if Path(configured).is_file() else None
    candidates = []
    if getattr(sys, "frozen", False):
        candidates.append(Path(sys.executable).resolve().parent / RUNNER)
    candidates.append(HERE.parent.parent / "rust" / "target" / "release" / RUNNER)
    # PATH directories only: shutil.which() on Windows also searches the working directory first
    candidates.extend(Path(directory) / RUNNER for directory in os.environ.get("PATH", "").split(os.pathsep)
                      if directory.strip())
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return None


def status():
    """What the layout backend would do now, for diagnostics."""
    selected, warning = mode()
    runner = find_runner()
    return {"backend": selected, "runner": str(runner) if runner else None, "warning": warning}


def run_rust(pins, arrange, group_gap, runner=None):
    """The runner's response object for one request.  Raises RunnerUnavailable."""
    arguments = {"pins": pins, "arrange": arrange, "group_gap": group_gap}
    try:
        # ASCII, with everything else escaped: a lone surrogate cannot be encoded as UTF-8
        request = json.dumps(arguments, allow_nan=False)
        # Tuples, non-string keys and other values JSON would quietly change stay with Python
        if json.loads(request) != arguments:
            raise ValueError("JSON would change it")
    except (TypeError, ValueError, RecursionError) as exc:
        raise RunnerUnavailable(f"request is not JSON: {exc}") from None
    runner = runner or find_runner()
    if runner is None:
        configured = configured_runner()
        raise RunnerUnavailable(f"{ENV_BIN}={configured!r} is not a file" if configured else
                                f"no {RUNNER} runner (set {ENV_BIN})")
    try:
        done = subprocess.run([str(runner)], input=(request + "\n").encode("ascii"), capture_output=True,
                              timeout=TIMEOUT_S, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    except subprocess.TimeoutExpired:
        raise RunnerUnavailable(f"runner timed out after {TIMEOUT_S} s") from None
    except OSError as exc:
        raise RunnerUnavailable(f"runner did not start: {exc}") from None
    if done.returncode != 0:
        detail = done.stderr.decode("utf-8", "replace").strip()[:200]
        raise RunnerUnavailable(f"runner exited with {done.returncode}: {detail}")
    # One response line.  Split on LF only: splitlines() would also split at U+2028 and other
    # separators that the runner writes unescaped inside JSON strings.
    lines = done.stdout.decode("utf-8", "replace").removesuffix("\n").split("\n")
    try:
        if len(lines) != 1:
            raise ValueError(f"{len(lines)} lines")
        response = json.loads(lines[0])
        if not isinstance(response, dict):
            raise ValueError("not an object")
    except ValueError as exc:
        raise RunnerUnavailable(f"runner gave no response ({exc})") from None
    if "error" in response:
        error = response["error"]
        if not isinstance(error, dict) or error.get("type") not in LAYOUT_ERRORS:
            raise RunnerUnavailable(f"request outside the runner's boundary: {error}")
    elif not ("body" in response and "pins" in response):
        raise RunnerUnavailable("runner response has no body and pins")
    return response


def raise_layout_error(error):
    """Raise a runner layout error as the Python exception layout_box raises."""
    kind, message = LAYOUT_ERRORS[error["type"]], str(error.get("message", ""))
    if kind is KeyError:
        # str(KeyError(key)) is repr(key); raise the key itself so str() gives the same text
        try:
            raise KeyError(ast.literal_eval(message))
        except (ValueError, SyntaxError):
            raise KeyError(message) from None
    raise kind(message)


def python_layout(pins, arrange, group_gap):
    """(result, response, error): layout_box's result, the same as a runner response, and its error."""
    try:
        result = layout_box(pins, arrange=arrange, group_gap=group_gap)
    except (ValueError, KeyError, TypeError) as exc:
        message = str(exc)
        if isinstance(exc, TypeError):
            # As parity.py does: Python 3.14 words this "cannot use 'list' as a dict key (...)"
            found = re.search(r"unhashable type: '\w+'", message)
            message = found.group(0) if found else message
        return None, {"error": {"type": type(exc).__name__, "message": message}}, exc
    return result, json.loads(json.dumps(result, ensure_ascii=False)), None


def same(a, b):
    """Equal as the parity harness compares: key order, int against float and exact values count."""
    return json.dumps(a, ensure_ascii=False) == json.dumps(b, ensure_ascii=False)


def layout(pins, arrange="auto", group_gap=2):
    """(layout, info): layout_box's result from the configured backend, and how it was produced.

    info is None for the default Python backend (so the tool result does not change); otherwise
    {"backend": what produced the layout, "requested": the setting, and "fallback" (the reason
    Python was used), "warning" or "shadow" ("match" or "mismatch") where they apply}.
    """
    selected, warning = mode()
    if selected == "python":
        result = layout_box(pins, arrange=arrange, group_gap=group_gap)
        if warning:
            log.warning(warning)
            return result, {"backend": "python", "requested": "python", "warning": warning}
        return result, None

    info = {"backend": "python", "requested": selected}
    try:
        response = run_rust(pins, arrange, group_gap)
    except RunnerUnavailable as exc:
        log.warning("Rust layout unavailable, using Python: %s", exc)
        info["fallback"] = str(exc)
        return layout_box(pins, arrange=arrange, group_gap=group_gap), info

    if selected == "rust":
        info["backend"] = "rust"
        if "error" in response:
            raise_layout_error(response["error"])
        return response, info

    result, expected, error = python_layout(pins, arrange, group_gap)
    if same(expected, response):
        info["shadow"] = "match"
    else:
        info["shadow"] = "mismatch"
        log.warning("Rust layout differs from Python for %s: python %s, rust %s",
                    json.dumps({"pins": pins, "arrange": arrange, "group_gap": group_gap}, default=repr),
                    json.dumps(expected), json.dumps(response))
    if error is not None:
        raise error
    return result, info
