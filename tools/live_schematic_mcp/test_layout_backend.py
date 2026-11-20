# SPDX-License-Identifier: GPL-3.0-or-later
"""The opt-in Rust symbol layout backend: selection, fallback, shadow checks and real parity."""

import json
import os
import subprocess
from pathlib import Path

import pytest

import layout_backend
from symbol_layout import layout_box

LDO = [{"number": "1", "name": "GND", "type": "power_in"}, {"number": "2", "name": "VOUT", "type": "power_out"},
       {"number": "3", "name": "VIN", "type": "power_in"}, {"number": "4", "name": "EN", "type": "input"}]
CASES = Path(__file__).resolve().parents[2] / "rust" / "parity" / "cases.json"


def as_response(value):
    return json.loads(json.dumps(value))


def outcome(call):
    """A layout result as JSON text, or the exception type and text it raised."""
    try:
        return json.dumps(call())
    except (ValueError, KeyError, TypeError) as exc:
        return f"{type(exc).__name__}: {exc}"


@pytest.fixture
def runner(monkeypatch, tmp_path):
    """A fake embeint-layout: answers each call with the next queued reply and records requests."""
    binary = tmp_path / layout_backend.RUNNER
    binary.write_bytes(b"")
    monkeypatch.setenv(layout_backend.ENV_BIN, str(binary))
    calls, replies = [], []

    def run(command, input, **kwargs):
        calls.append(json.loads(input.decode("utf-8")))
        reply = replies.pop(0)
        if isinstance(reply, BaseException):
            raise reply
        if isinstance(reply, subprocess.CompletedProcess):
            return reply
        text = reply if isinstance(reply, str) else json.dumps(reply)
        return subprocess.CompletedProcess(command, 0, (text + "\n").encode("utf-8"), b"")

    monkeypatch.setattr(layout_backend.subprocess, "run", run)
    return calls, replies


def use(monkeypatch, backend):
    monkeypatch.setenv(layout_backend.ENV_MODE, backend)


def test_python_is_the_default_and_never_starts_the_runner(monkeypatch, runner):
    monkeypatch.delenv(layout_backend.ENV_MODE, raising=False)
    calls, _ = runner
    result, info = layout_backend.layout(LDO)
    assert result == layout_box(LDO) and info is None and calls == []


def test_an_unknown_setting_warns_and_uses_python(monkeypatch, runner):
    use(monkeypatch, "fortran")
    result, info = layout_backend.layout(LDO)
    assert result == layout_box(LDO) and runner[0] == []
    assert info["backend"] == "python" and "fortran" in info["warning"]


def test_rust_backend_returns_the_runner_layout(monkeypatch, runner):
    use(monkeypatch, "Rust")
    calls, replies = runner
    replies.append(as_response(layout_box(LDO, group_gap=1)))
    result, info = layout_backend.layout(LDO, arrange="auto", group_gap=1)
    assert calls == [{"pins": LDO, "arrange": "auto", "group_gap": 1}]
    assert result == as_response(layout_box(LDO, group_gap=1))
    assert info == {"backend": "rust", "requested": "rust"}


def test_unicode_line_separators_inside_a_response_are_not_line_breaks(monkeypatch, runner):
    use(monkeypatch, "rust")
    pins = [{"number": "1", "name": "A B\x1cC\u0085D"}]
    runner[1].append(json.dumps(as_response(layout_box(pins)), ensure_ascii=False))
    result, info = layout_backend.layout(pins)
    assert info["backend"] == "rust" and result["pins"][0]["name"] == pins[0]["name"]


@pytest.mark.parametrize("pins", [[],[{"number": "1"}, {"number": "1"}], [{"number": "1", "side": "middle"}],
                                  [{"gap": True, "side": "middle"}], [{"gap": True, "side": ["x"]}]])
def test_rust_layout_errors_are_raised_as_python_raises_them(monkeypatch, runner, pins):
    use(monkeypatch, "rust")
    expected = outcome(lambda: layout_box(pins))
    kind, message = expected.split(": ", 1)
    runner[1].append({"error": {"type": kind, "message": message}})
    assert outcome(lambda: layout_backend.layout(pins)[0]) == expected


@pytest.mark.parametrize("reply, reason", [
    (FileNotFoundError("gone"), "did not start"),
    (subprocess.TimeoutExpired("embeint-layout", 10), "timed out"),
    (subprocess.CompletedProcess([], 101, b"", b"thread 'main' panicked"), "exited with 101"),
    ("not json", "no response"),
    ('{"body": [0, 0, 1, 1]}', "no body and pins"),
    ({"error": {"type": "RequestError", "message": "group_gap must be an integer"}}, "boundary"),
    ({"error": {"type": "OSError", "message": "?"}}, "boundary"),
])
def test_runner_failures_fall_back_to_python(monkeypatch, runner, reply, reason):
    use(monkeypatch, "rust")
    runner[1].append(reply)
    result, info = layout_backend.layout(LDO)
    assert result == layout_box(LDO)
    assert info["backend"] == "python" and info["requested"] == "rust" and reason in info["fallback"]


def test_a_missing_runner_falls_back_to_python(monkeypatch, tmp_path):
    use(monkeypatch, "rust")
    monkeypatch.setenv(layout_backend.ENV_BIN, str(tmp_path / "missing"))
    assert layout_backend.find_runner() is None
    result, info = layout_backend.layout(LDO)
    assert result == layout_box(LDO) and "EMBEINT_LAYOUT_BIN=" in info["fallback"]
    assert "is not a file" in info["fallback"]
    monkeypatch.delenv(layout_backend.ENV_BIN)
    monkeypatch.setenv("PATH", "")
    monkeypatch.setattr(layout_backend, "HERE", tmp_path / "a" / "b")
    assert "no embeint-layout" in layout_backend.layout(LDO)[1]["fallback"]


def test_a_quoted_runner_setting_is_found(monkeypatch, runner):
    path = os.environ[layout_backend.ENV_BIN]
    monkeypatch.setenv(layout_backend.ENV_BIN, f' "{path}" ')
    assert str(layout_backend.find_runner()) == path


def test_the_working_directory_is_not_searched(monkeypatch, tmp_path):
    monkeypatch.delenv(layout_backend.ENV_BIN, raising=False)
    monkeypatch.setenv("PATH", "")
    monkeypatch.setattr(layout_backend, "HERE", tmp_path / "a" / "b")
    (tmp_path / layout_backend.RUNNER).write_bytes(b"")
    monkeypatch.chdir(tmp_path)
    assert layout_backend.find_runner() is None


@pytest.mark.parametrize("backend", ["rust", "shadow"])
@pytest.mark.parametrize("pins", [
    [{"number": "1", "name": "A", "note": float("nan")}],  # NaN is not JSON
    [{"number": "1", "name": "A", "at": (1, 2)}],         # JSON would turn the tuple into a list
    [{"number": "1", 7: "seven"}],                        # and the key 7 into "7"
])
def test_a_request_json_cannot_carry_falls_back_without_starting_the_runner(monkeypatch, runner, pins, backend):
    use(monkeypatch, backend)
    result, info = layout_backend.layout(pins)
    assert runner[0] == [] and "not JSON" in info["fallback"]
    assert result == layout_box(pins)


def test_shadow_ignores_python_3_14_wording_of_unhashable_types(monkeypatch):
    def unhashable(*args, **kwargs):
        raise TypeError("cannot use 'list' as a dict key (unhashable type: 'list')")
    monkeypatch.setattr(layout_backend, "layout_box", unhashable)
    response = layout_backend.python_layout([], "auto", 2)[1]
    assert response == {"error": {"type": "TypeError", "message": "unhashable type: 'list'"}}


def test_shadow_uses_python_and_reports_a_match(monkeypatch, runner):
    use(monkeypatch, "shadow")
    runner[1].append(as_response(layout_box(LDO)))
    result, info = layout_backend.layout(LDO)
    assert result == layout_box(LDO) and info == {"backend": "python", "requested": "shadow", "shadow": "match"}


def test_shadow_reports_and_logs_a_difference_but_uses_python(monkeypatch, runner, caplog):
    use(monkeypatch, "shadow")
    wrong = as_response(layout_box(LDO))
    wrong["pins"][0]["length"] = 2  # an int where Python gives the float 2.54: a parity break
    runner[1].append(wrong)
    result, info = layout_backend.layout(LDO)
    assert result == layout_box(LDO) and info["shadow"] == "mismatch"
    assert "differs from Python" in caplog.text


def test_shadow_raises_python_errors_after_comparing(monkeypatch, runner):
    use(monkeypatch, "shadow")
    runner[1].append({"error": {"type": "ValueError", "message": "A symbol needs at least one pin"}})
    with pytest.raises(ValueError, match="at least one pin"):
        layout_backend.layout([])
    assert len(runner[0]) == 1


def test_status_reports_the_setting_and_runner(monkeypatch, runner):
    use(monkeypatch, "shadow")
    report = layout_backend.status()
    assert report["backend"] == "shadow" and report["runner"].endswith(layout_backend.RUNNER)


# ---- Against the real runner, when rust/target/release has one (cargo build --release) ----------

def parity_requests():
    for case in json.loads(CASES.read_text(encoding="utf-8")):
        request = case.get("request")
        if (isinstance(request, dict) and set(request) <= {"pins", "arrange", "group_gap"}
                and isinstance(request.get("pins"), list) and all(isinstance(p, dict) for p in request["pins"])
                and isinstance(request.get("arrange", "auto"), str)
                and type(request.get("group_gap", 2)) is int):
            yield pytest.param(request, id=case["name"])


@pytest.fixture
def real_runner(monkeypatch):
    monkeypatch.delenv(layout_backend.ENV_BIN, raising=False)
    found = layout_backend.find_runner()
    if found is None:
        message = "no embeint-layout runner built (cargo build --release in rust/)"
        if os.environ.get("EMBEINT_LAYOUT_REQUIRE_RUNNER"):
            pytest.fail(message)
        pytest.skip(message)
    return found


@pytest.mark.parametrize("backend", ["rust", "shadow"])
def test_a_lone_surrogate_falls_back_instead_of_failing_to_encode(monkeypatch, real_runner, backend):
    # Sent as an ASCII escape; the runner's JSON parser rejects it, so Python lays it out
    use(monkeypatch, backend)
    pins = [{"number": "1", "name": "A" + chr(0xD800)}]
    result, info = layout_backend.layout(pins)
    assert result == layout_box(pins) and "boundary" in info["fallback"]


@pytest.mark.parametrize("request_", list(parity_requests()))
def test_real_runner_matches_python(monkeypatch, real_runner, request_, caplog):
    pins, arrange, gap = request_["pins"], request_.get("arrange", "auto"), request_.get("group_gap", 2)
    caplog.set_level("WARNING", logger=layout_backend.log.name)
    # The runner itself answers (RunnerUnavailable fails the test), with Python's response
    response = layout_backend.run_rust(pins, arrange, gap, runner=real_runner)
    assert layout_backend.same(layout_backend.python_layout(pins, arrange, gap)[1], response)
    use(monkeypatch, "python")
    expected = outcome(lambda: as_response(layout_backend.layout(pins, arrange, gap)[0]))
    # Through layout(), layouts and errors alike, with no fallback and no shadow difference
    for backend in ("rust", "shadow"):
        use(monkeypatch, backend)
        assert outcome(lambda: as_response(layout_backend.layout(pins, arrange, gap)[0])) == expected
    assert caplog.records == []
