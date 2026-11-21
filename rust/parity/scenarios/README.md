# Parity scenarios per tool area

`mcp_parity.py` loads every `<area>.py` here (files starting with `_` are skipped). Each defines

```python
def scenarios(h):
    """h is mcp_parity itself: h.S (scenario), h.reply, h.parse, h.failure, h.documents,
    h.version, h.status, h.SHEET_DOC, h.BOARD_DOC, h.DROP, h.CLOSE, the protobuf modules
    (h.editor, h.envelope, h.types, h.base) and h.pyclient."""
    return [h.S("schematic_place_symbol", [("live_schematic_place_symbol", {...})],
                [("GetLibrarySymbol", lambda r: h.reply(...)), ...]),
            ...]
```

A scenario is `S(name, steps, script, **options)`: `steps` are `(tool, arguments)` calls made in
one MCP session, and `script` is the KiCad side, one `(request type, respond)` pair per request
in order. Both servers run every scenario; the requests KiCad receives, the tool results and any
files written under `<out>` must be identical. Options:

- `compare="error_prefix"`: argument validation, compared by failure and tool, not wording.
- `compare=callable(python_results, rust_results)`: a behavioural comparison, only where the
  parity plan allows one (routing and path checks).
- `prepare=callable(out_dir)`, `discovery=[names]`, `stop_before=index`: as in `mcp_parity.py`.

Scenario names must be unique across all areas; prefix them with the area. Every tool the Rust
server advertises must be called by at least one scenario (the schema check fails otherwise).
