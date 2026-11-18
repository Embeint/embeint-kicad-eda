# SPDX-License-Identifier: GPL-3.0-or-later
"""Entry point shared by the frozen stdio server and the Linux desktop launcher."""

import json
from contextlib import ExitStack
import os
from pathlib import Path
import sys


def run_stdio(serve):
    # The SDK rewraps binary stdio, whose wrapper finalizers can close the supplied
    # buffers. Give it duplicate handles so the frozen bootloader can still flush
    # the process's original stdout during shutdown.
    original_stdin, original_stdout = sys.stdin, sys.stdout
    with ExitStack() as streams:
        stdin = streams.enter_context(os.fdopen(os.dup(original_stdin.fileno()), "r", encoding="utf-8"))
        stdout = streams.enter_context(os.fdopen(os.dup(original_stdout.fileno()), "w", encoding="utf-8"))
        try:
            sys.stdin, sys.stdout = stdin, stdout
            serve()
        finally:
            sys.stdin, sys.stdout = original_stdin, original_stdout


def configure_desktop(config_home):
    """Initialize this distribution's isolated settings without replacing preferences."""
    config = Path(config_home) / "10.99"
    config.mkdir(parents=True, exist_ok=True)
    path = config / "kicad_common.json"
    data = json.loads(path.read_text()) if path.exists() else {}
    previous = json.dumps(data, sort_keys=True)
    data.setdefault("api", {}).setdefault("enable_server", True)
    prompts = data.setdefault("do_not_show_again", {})
    prompts.setdefault("data_collection_prompt", True)
    prompts.setdefault("update_check_prompt", True)
    if not path.exists() or json.dumps(data, sort_keys=True) != previous:
        # Exclusive creation also refuses an unexpected pre-existing temporary path.
        temporary = path.with_name(f"{path.name}.{os.getpid()}.tmp")
        created = False
        try:
            with temporary.open("x") as stream:
                created = True
                json.dump(data, stream, indent=2)
                stream.write("\n")
            temporary.replace(path)
        finally:
            if created:
                temporary.unlink(missing_ok=True)
    for name, kind, directory, pattern in (
        ("sym-lib-table", "sym", "symbols", "*.kicad_sym"),
        ("fp-lib-table", "fp", "footprints", "*.pretty"),
        ("design-block-lib-table", "design_block", "blocks", "*.kicad_blocks"),
    ):
        target = config / name
        if target.exists():
            continue
        libraries = sorted((Path("/usr/share/kicad") / directory).glob(pattern))
        entries = [f" (lib (name {json.dumps(lib.stem)}) (type \"KiCad\") "
                   f"(uri {json.dumps(str(lib))}) (options \"\") (descr \"\"))" for lib in libraries]
        try:
            with target.open("x") as stream:
                stream.write(f"({kind}_lib_table (version 7)\n" + "\n".join(entries) + ")\n")
        except FileExistsError:
            # Another launch initialized this table first.
            pass


def main():
    if sys.argv[1:2] == ["--diagnose"]:
        from diagnose import main as diagnose
        del sys.argv[1]
        diagnose()
        return
    if sys.argv[1:] == ["--configure-desktop"]:
        configure_desktop(os.environ["KICAD_CONFIG_HOME"])
        return
    if len(sys.argv) != 1:
        raise SystemExit("Usage: embeint-kicad-mcp [--configure-desktop | --diagnose [options]]")
    from server import mcp
    run_stdio(lambda: mcp.run(transport="stdio"))


if __name__ == "__main__":
    main()
