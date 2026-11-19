# SPDX-License-Identifier: GPL-3.0-or-later
"""Hand a footprint's 3D model to SolidCraft and bring the STEP back.

The plan is a SolidCraft MCP `batch` that builds the package from datasheet numbers, in KiCad's 3D
model conventions, plus the `export` call that writes the STEP.  The agent runs both on the
SolidCraft MCP, then hands the file to live_footprint_attach_step, which embeds it.

KiCad 3D model conventions (what the plan follows):
- millimetres; the model origin sits on the footprint origin, on the board's top surface
- +Z up out of the board; +X matches the footprint's +X; +Y is the footprint's -Y (KiCad's
  footprint Y axis points down the screen, the 3D Y axis points up), so pin 1 at footprint
  (-2.475, -1.905) is at model (-2.475, +1.905)
"""

import re
import sys

BODY_COLOR, LEAD_COLOR = "#1f1f1f", "#c9ccd1"


def to_wsl(path):
    """C:\\Users\\x\\m.step -> /mnt/c/Users/x/m.step (other paths unchanged)."""
    match = re.match(r"^([A-Za-z]):[\\/](.*)$", str(path))
    if not match:
        return str(path)
    return f"/mnt/{match.group(1).lower()}/" + match.group(2).replace("\\", "/")


def to_windows(path):
    """/mnt/c/Users/x/m.step -> C:\\Users\\x\\m.step (other paths unchanged)."""
    match = re.match(r"^/mnt/([a-z])/(.*)$", str(path))
    if not match:
        return str(path)
    return f"{match.group(1).upper()}:\\" + match.group(2).replace("/", "\\")


def kicad_path(path):
    """The path as the KiCad this MCP drives reads it.

    The MCP runs next to KiCad: natively on Windows, where a WSL /mnt/x path is mapped back
    to X:\\, and inside WSL, where SolidCraft's Windows paths are mapped to /mnt/x.
    """
    return to_windows(path) if sys.platform == "win32" else to_wsl(path)


def _r(value):
    return round(value, 4)


# ---- Checking a footprint's 3D model in KiCad -----------------------------------------------

def _balanced(text, start):
    """The parenthesised expression starting at text[start], honouring quoted strings and |...| data."""
    depth, quoted, barred, escaped = 0, False, False, False
    for index in range(start, len(text)):
        char = text[index]
        if barred:
            barred = char != "|"
        elif quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            else:
                quoted = char != '"'
        elif char == '"':
            quoted = True
        elif char == "|":
            barred = True
        elif char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return text[start:index + 1]
    raise ValueError("unbalanced expression")


def footprint_board(footprint_text, lib_id, margin=2.0):
    """A minimal board holding one library footprint at the origin, with a board outline around it.

    The footprint keeps everything (pads, graphics, embedded files, 3D models); only the library
    header tokens a board does not take are dropped.
    """
    text = footprint_text.strip()
    version = re.search(r"\(version\s+(\d+)\)", text)
    board_version = version.group(1) if version else "20250114"
    if text.startswith("(kicad_footprint_lib"):
        # KiCad 11 library files wrap the footprint: keep just the (footprint ...) expression
        text = _balanced(text, text.index("(footprint", 1))
    text = re.sub(r'^\(footprint\s+("[^"]*"|\S+)', f'(footprint "{lib_id}"', text, count=1)
    for token in ("version", "generator_version", "generator"):
        text = re.sub(rf'\n?\s*\({token}\s+("[^"]*"|[^\s()]+)\)', "", text, count=1)
    text = re.sub(r'\(layer\s+"F\.Cu"\)', '(layer "F.Cu")\n  (at 0 0)', text, count=1)
    xs, ys = [], []
    for x, y in re.findall(r'\((?:start|end|at|xy)\s+(-?[\d.]+)\s+(-?[\d.]+)', text):
        xs.append(float(x))
        ys.append(float(y))
    x1, x2 = min(xs, default=-5) - margin, max(xs, default=5) + margin
    y1, y2 = min(ys, default=-5) - margin, max(ys, default=5) + margin
    return (f'(kicad_pcb\n  (version {board_version})\n  (generator "kicad-live-mcp")\n  (generator_version "10.99")\n'
            f'  (general (thickness 1.6) (legacy_teardrops no))\n  (paper "A4")\n'
            f'{text}\n'
            f'  (gr_rect (start {x1:.3f} {y1:.3f}) (end {x2:.3f} {y2:.3f})'
            f' (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))\n)\n')


def kicad_cli_env(cli_path, kicad_variables=None):
    """Environment for kicad-cli: what the running KiCad reported (kicad_variables), else rebuilt from
    the install layout (see kicad_cli.cli_environment)."""
    from kicad_cli import cli_environment
    return cli_environment(cli_path, kicad_variables)[0]


def render_footprint_3d(cli_path, footprint_text, lib_id, rotate="-45,0,45", size=(900, 700), zoom=1.0,
                        env=None):
    """Render one footprint's 3D view through kicad-cli; returns PNG bytes."""
    import subprocess
    import tempfile
    from pathlib import Path
    with tempfile.TemporaryDirectory(prefix="kicad-3d-check-") as scratch:
        board = Path(scratch) / "check.kicad_pcb"
        image = Path(scratch) / "check.png"
        board.write_text(footprint_board(footprint_text, lib_id))
        command = [cli_path, "pcb", "render", "-w", str(size[0]), "-h", str(size[1]), "--quality", "high",
                   "--perspective", "--zoom", str(zoom), "--background", "opaque", "-o", str(image), str(board)]
        if rotate:
            command[3:3] = ["--rotate", rotate]
        run = subprocess.run(command, env=env or kicad_cli_env(cli_path), capture_output=True, text=True,
                             timeout=300)
        if run.returncode or not image.exists():
            raise RuntimeError(f"kicad-cli render failed ({run.returncode}): {run.stderr[-800:] or run.stdout[-800:]}")
        return image.read_bytes()


def export_board_step(cli_path, footprint_text, lib_id, env=None):
    """Export the footprint's board to STEP through kicad-cli (a separate KiCad process), so the
    output proves KiCad itself resolves the footprint's 3D model.  Returns the STEP text."""
    import subprocess
    import tempfile
    from pathlib import Path
    with tempfile.TemporaryDirectory(prefix="kicad-3d-check-") as scratch:
        board = Path(scratch) / "check.kicad_pcb"
        output = Path(scratch) / "check.step"
        board.write_text(footprint_board(footprint_text, lib_id))
        run = subprocess.run([cli_path, "pcb", "export", "step", "--force", "-o", str(output), str(board)],
                             env=env or kicad_cli_env(cli_path), capture_output=True, text=True, timeout=300)
        if run.returncode or not output.exists():
            raise RuntimeError(f"kicad-cli STEP export failed ({run.returncode}): {(run.stderr or run.stdout)[-800:]}")
        return output.read_text(errors="replace")


def default_model_dir():
    """A folder that both SolidCraft (Windows) and KiCad can reach, as KiCad sees it."""
    import os
    import subprocess
    if os.environ.get("KICAD_LIVE_MODEL_DIR"):
        return os.environ["KICAD_LIVE_MODEL_DIR"]
    if sys.platform == "win32":
        profile = os.environ.get("USERPROFILE", "")
        if re.match(r"^[A-Za-z]:\\", profile):
            return profile.rstrip("\\") + "\\kicad-models"
        raise RuntimeError("Set KICAD_LIVE_MODEL_DIR to a folder for SolidCraft's STEP exports")
    try:
        profile = subprocess.run(["cmd.exe", "/c", "echo %USERPROFILE%"], capture_output=True, text=True,
                                 timeout=20, cwd="/mnt/c").stdout.strip()
    except (OSError, subprocess.SubprocessError):
        profile = ""
    if re.match(r"^[A-Za-z]:\\", profile):
        return to_wsl(profile) + "/kicad-models"
    raise RuntimeError("Set KICAD_LIVE_MODEL_DIR to a folder on a Windows drive (e.g. /mnt/c/Users/me/kicad-models)")


def embedded_file_bytes(footprint_text, name):
    """Decode one embedded file from a saved .kicad_mod: base64 of a zstd stream."""
    import base64
    match = re.search(r'\(file\s+\(name\s+"' + re.escape(name) + r'"\).*?\(data\s+((?:\|[^|]*\|\s*)+)\)',
                      footprint_text, re.S)
    if not match:
        raise ValueError(f"no embedded file named {name}")
    encoded = "".join(part.strip() for part in re.findall(r"\|([^|]*)\|", match.group(1)))
    compressed = base64.b64decode(re.sub(r"\s+", "", encoded))
    try:
        from compression import zstd  # Python 3.14+
        return zstd.decompress(compressed)
    except ImportError:
        import zstandard
        return zstandard.ZstdDecompressor().decompress(compressed, max_output_size=1 << 30)


def _box_steps(name, x1, y1, x2, y2, z0, height):
    return [
        {"command": "sketch.create", "params": {"plane": "XY", "offset": _r(z0)}},
        {"command": "sketch.rectangle.two_point", "params": {"p0": [_r(x1), _r(y1)], "p1": [_r(x2), _r(y2)]}},
        {"command": "sketch.finish", "params": {}},
        {"command": "solid.extrude", "params": {"distance": _r(height), "operation": "new", "body_name": name}},
    ]


def _gullwing_lead(name, side, y, body_edge, tip, foot, thickness, width, exit_z):
    """A gull-wing lead: profile in the XZ plane at model y, extruded symmetrically by its width."""
    s = 1 if side > 0 else -1
    xa, xb = body_edge - 0.1, body_edge + 0.25
    xc, xd = tip - foot, tip
    t = thickness
    # Centre-line A (inside the body) -> B (shoulder) -> C (knee on the board) -> D (toe), offset by t/2
    profile = [(xa, exit_z + t / 2), (xb + t / 2, exit_z + t / 2), (xc + t / 2, t), (xd, t),
               (xd, 0.0), (xc - t / 2, 0.0), (xb - t / 2, exit_z - t / 2), (xa, exit_z - t / 2)]
    points = [[_r(s * x), _r(z)] for x, z in profile]
    return [
        {"command": "sketch.create",
         "params": {"plane": {"origin": [0, _r(y), 0], "x_dir": [1, 0, 0], "y_dir": [0, 0, 1]}}},
        {"command": "sketch.line", "params": {"points": points, "closed": True}},
        {"command": "sketch.finish", "params": {}},
        {"command": "solid.extrude", "params": {"distance": _r(width), "direction": "symmetric",
                                                 "operation": "new", "body_name": name}},
    ]


def model_plan(package, params, body_height, standoff=0.1, lead_thickness=0.2, lead_width=None,
               lead_span=None, foot_length=None, export_path=None):
    """SolidCraft batch steps for the package footprint_gen builds from the same params.

    body_height is seated height (board to top of body, datasheet A); standoff is the gap under
    the body (A1).  Gull-wing packages need lead_span (tip to tip, datasheet E) unless the
    footprint params carry it; foot_length is datasheet L.
    """
    from footprint_gen import generate
    shape = generate(package, **params)
    bx1, by1, bx2, by2 = shape["body"]
    # Footprint body is in footprint coordinates; flip Y into model coordinates
    steps = _box_steps("Body", bx1, -by2, bx2, -by1, standoff, body_height - standoff)
    lead_names = []
    key = package.lower()
    pads = [p for p in shape["pads"] if p["type"] == "smd"]

    if key in ("soic", "tssop", "msop", "ssop", "gullwing", "sot"):
        span = lead_span or params.get("lead_span")
        if span is None:
            # The land pattern's outer pad edges sit TOE beyond the lead tips
            outer = max(abs(p["position"][0]) + p["size"][0] / 2 for p in pads)
            span = 2 * (outer - 0.35)
        foot = foot_length or params.get("lead_length", 0.6)
        exit_z = standoff + 0.45 * (body_height - standoff)
        for pad in pads:
            x, y = pad["position"]
            if abs(x) < 1e-6:
                continue  # centred exposed pad: modelled below
            name = f"Lead{pad['number']}"
            pad_w = pad["size"][1]
            width = lead_width or params.get("lead_width") or pad_w * 0.7
            if pad_w > 2.5 * width:  # a tab (SOT-223 pin 4) is as wide as most of its pad
                width = pad_w * 0.85
            edge = bx2 if x > 0 else -bx1
            steps += _gullwing_lead(name, 1 if x > 0 else -1, -y, abs(edge), span / 2, foot, lead_thickness,
                                    width, exit_z)
            lead_names.append(name)
    elif key in ("qfn", "dfn"):
        # No standoff: the body sits on the board with its terminals flush underneath
        steps = _box_steps("Body", bx1, -by2, bx2, -by1, 0.0, body_height)
        for pad in pads:
            (x, y), (w, h) = pad["position"], pad["size"]
            tx1, tx2 = max(x - w / 2, bx1), min(x + w / 2, bx2)
            ty1, ty2 = max(y - h / 2, by1), min(y + h / 2, by2)
            name = f"Pad{pad['number']}"
            steps += _box_steps(name, tx1, -ty2, tx2, -ty1, 0.0, 0.05)
            lead_names.append(name)
    elif key == "chip":
        length = bx2 - bx1
        cap = min(0.5, 0.25 * length)
        steps = _box_steps("Body", bx1 + cap, -by2, bx2 - cap, -by1, 0.0, body_height)
        for name, x1, x2 in (("Cap1", bx1, bx1 + cap), ("Cap2", bx2 - cap, bx2)):
            steps += _box_steps(name, x1, -by2, x2, -by1, 0.0, body_height)
            lead_names.append(name)

    if shape.get("pin1_marker") and key not in ("chip",):
        # Pin-1 dimple near the pin-1 corner of the top face (model +Y is footprint -Y)
        cx, cy = bx1 + 0.6, -(by1 + 0.6)
        steps += [
            {"command": "sketch.create", "params": {"plane": "XY", "offset": _r(body_height)}},
            {"command": "sketch.circle.center", "params": {"center": [_r(cx), _r(cy)], "radius": 0.25}},
            {"command": "sketch.finish", "params": {}},
            {"command": "solid.extrude", "params": {"distance": 0.05, "direction": "negative",
                                                     "operation": "cut", "targets": ["Body"]}},
        ]

    steps.append({"command": "appearance.assign", "params": {"bodies": ["Body"], "color": BODY_COLOR}})
    if lead_names:
        steps.append({"command": "appearance.assign", "params": {"bodies": lead_names, "color": LEAD_COLOR}})

    plan = {
        "footprint": shape["name"],
        "conventions": "mm; origin = footprint origin on the board top surface; +Z up; model +Y = "
                       "footprint -Y (pin 1 top-left in KiCad is at model x<0, y>0)",
        "solidcraft": {"tool": "batch", "arguments": {"commands": steps}},
        "bodies": ["Body"] + lead_names,
    }
    if export_path:
        plan["export"] = {"tool": "export", "arguments": {"path": to_windows(export_path), "format": "step"}}
        plan["then"] = {"tool": "live_footprint_attach_step", "step_path": kicad_path(export_path)}
    return plan
