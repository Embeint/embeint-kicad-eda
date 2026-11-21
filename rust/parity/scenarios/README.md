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
- `compare=callable(python, rust)`: a behavioural comparison, only where the parity plan allows
  one (routing and path checks). Each argument is that server's outcome, a dict with `requests`
  (per peer, as KiCad received them), `results` (the tool results), `written` (files under
  `<out>` by hash) and `errors`; return a list of differences, empty when they agree. The
  requests, results and written files are then not compared exactly.
- `normalize=callable(requests, results) -> (requests, results)`: replaces values a tool generates
  itself (new item UUIDs, a temporary path, today's date) the same way for both servers.
- `prepare=callable(out_dir)`, `discovery=[names]`, `stop_before=index`: as in `mcp_parity.py`.
- `random_ids=True`: requests carry new random UUIDs (items created with `uuid.uuid4()`); version 4
  UUIDs in the requests are compared by order of appearance. Every fixed id in such a scenario
  (items in the scripted replies, ids in the arguments) must therefore not be version 4 (use e.g.
  `aaaaaaaa-aaaa-1aaa-8aaa-aaaaaaaaaaaa`): renumbered fixed ids would let a request that targets
  the wrong item, or the right items in another order, compare equal.

Scenario names must be unique across all areas; prefix them with the area. Every tool the Rust
server advertises must be called by at least one scenario that is not only argument validation
(the schema check fails otherwise); `live_mcp_capabilities` is compared by the manifest check.
