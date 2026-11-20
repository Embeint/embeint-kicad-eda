# SPDX-License-Identifier: GPL-3.0-or-later
"""Fake manufacturing outputs for tests: what KiCad would write for each export job.

test_fab_package.py and rust/parity/mcp_parity.py answer export requests with these files, so the
package checks run on realistic Gerber job, Excellon, placement and BOM files without a KiCad.
"""

import json
from pathlib import Path

from fab_package import name_stem

POSITION = ('Ref,Val,Package,PosX,PosY,Rot,Side\n'
            '"C1","100n","C_0402_1005Metric",101.5,52.25,90.0,top\n'
            '"FID1","Fiducial","Fiducial_1mm",95.0,45.0,0.0,top\n'
            '"R1","10k","R_0402_1005Metric",104.0,52.25,0.0,top\n'
            '"R2","10k","R_0402_1005Metric",104.0,58.0,180.0,bottom\n')
BOM = ('"Reference","Qty","Value","Footprint","Manufacturer","MPN","Alt Manufacturer","Alt MPN","Description",'
       '"Datasheet"\n'
       '"C1","1","100n","Capacitor_SMD:C_0402_1005Metric","Murata","GRM155R71C104KA88D","","","",""\n'
       '"J1","1","USB_C","Connector:USB_C","GCT","USB4105-GF-A","","","",""\n'
       '"R1,R2","2","10k","Resistor_SMD:R_0402_1005Metric","","","","","",""\n')
EXCELLON_PTH = ("M48\n; DRILL file {KiCad 10.99} date fixture\nFMAT,2\nMETRIC\nT1C0.300\nT2C0.800\n%\nG90\nG05\n"
                "T1\nX100.0Y50.0\nX102.0Y50.0\nX104.0Y50.0\nT2\nX110.0Y55.0\nM30\n")
EXCELLON_NPTH = "M48\n; DRILL file {KiCad 10.99} date fixture\nFMAT,2\nMETRIC\nT1C3.200\n%\nG90\nG05\nT1\nX92.0Y42.0\nM30\n"


def layer_file(name, layer):
    return f"{name}-{layer.removeprefix('BL_')}.gbr"


def gerber_function(layer):
    # As KiCad 10.99 writes them in the job file
    names = {"BL_F_Mask": "SolderMask,Top", "BL_B_Mask": "SolderMask,Bot", "BL_F_Paste": "SolderPaste,Top",
             "BL_B_Paste": "SolderPaste,Bot", "BL_F_SilkS": "Legend,Top", "BL_B_SilkS": "Legend,Bot",
             "BL_Edge_Cuts": "Profile"}
    return names.get(layer, "Other,User")


def write_outputs(request, layer_names, drop_profile=False):
    """Write what KiCad writes for this export request (an unpacked RunBoardJobExport* or
    RunSchematicJobExport* message).  layer_names maps BoardLayer values to enum names."""
    kind = type(request).__name__ if hasattr(request, "DESCRIPTOR") else request
    target = request.job_settings.output_path
    doc = request.job_settings.document
    name = name_stem(doc.board_filename) if doc.board_filename else doc.project.name
    if target.endswith("/"):
        folder = Path(target)
        folder.mkdir(parents=True, exist_ok=True)
    else:
        Path(target).parent.mkdir(parents=True, exist_ok=True)
    if kind == "RunBoardJobExportGerbers":
        layers = [layer_names[v] for v in request.plot_settings.layers]
        copper = [layer for layer in layers if layer.endswith("_Cu")]
        attributes = []
        for layer in layers:
            if drop_profile and layer == "BL_Edge_Cuts":
                continue
            (folder / layer_file(name, layer)).write_text("G04 fixture*\nM02*\n", newline="")
            if layer in copper:
                index = copper.index(layer) + 1
                side = "Top" if index == 1 else "Bot" if index == len(copper) else "Inr"
                function = f"Copper,L{index},{side}"
            else:
                function = gerber_function(layer)
            attributes.append({"Path": layer_file(name, layer), "FileFunction": function,
                               "FilePolarity": "Positive"})
        job = {"Header": {"GenerationSoftware": {"Vendor": "KiCad", "Application": "Pcbnew"}},
               "GeneralSpecs": {"ProjectId": {"Name": name}, "LayerNumber": len(copper),
                                "Size": {"X": 50.0, "Y": 30.0}},
               "FilesAttributes": attributes}
        (folder / f"{name}-job.gbrjob").write_text(json.dumps(job, indent=2), newline="")
    elif kind == "RunBoardJobExportDrill":
        (folder / f"{name}-PTH.drl").write_text(EXCELLON_PTH, newline="")
        (folder / f"{name}-NPTH.drl").write_text(EXCELLON_NPTH, newline="")
        (folder / f"{name}-PTH-drl_map.pdf").write_bytes(b"%PDF-1.5\nfixture\n")
        (folder / f"{name}-NPTH-drl_map.pdf").write_bytes(b"%PDF-1.5\nfixture\n")
        (folder / f"{name}-drl.rpt").write_text("Drill report fixture\n", newline="")
    elif kind == "RunBoardJobExportPosition":
        path = Path(target)
        if not request.naked_filename:
            # KiCad decorates the name unless told not to: demo-pos.csv -> demo-pos-all-pos.csv
            path = path.with_name(f"{path.stem}-all-pos{path.suffix}")
        path.write_text(POSITION, newline="")
    elif kind == "RunSchematicJobExportBOM":
        Path(target).write_text(BOM, newline="")
    elif kind in ("RunBoardJobExportPdf", "RunSchematicJobExportPdf"):
        Path(target).write_bytes(b"%PDF-1.5\nfixture\n")
    elif kind == "RunBoardJobExport3D":
        Path(target).write_bytes(b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;\n")
    else:
        Path(target).write_bytes(b"fixture output\n")
