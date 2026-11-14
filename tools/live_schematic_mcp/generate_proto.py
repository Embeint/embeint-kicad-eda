# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the client protocol from this exact KiCad checkout."""

from pathlib import Path

import grpc_tools
from grpc_tools import protoc


def main():
    here = Path(__file__).resolve().parent
    source = here.parents[1] / "api" / "proto"
    output = here / "_generated"
    output.mkdir(exist_ok=True)
    google = Path(grpc_tools.__file__).parent / "_proto"
    args = ["protoc", f"-I{source}", f"-I{google}", f"--python_out={output}"]
    args.extend(str(p) for p in sorted(source.rglob("*.proto")))
    raise SystemExit(protoc.main(args))


if __name__ == "__main__":
    main()
