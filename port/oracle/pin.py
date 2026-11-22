# SPDX-License-Identifier: GPL-3.0-or-later
"""Single source pin and the complete source closure for the minimal kimath build."""

KICAD_PIN = "4cf939778df04d2b747bb2c4714fc5014a7565c7"
SOURCE_PATHS = (
    "libs/core", "libs/kimath", "include", "thirdparty/clipper2",
    "thirdparty/rtree", "thirdparty/nlohmann_json",
    "cmake/config.h.cmake", "cmake/PerformFeatureChecks.cmake",
    "pcb_calculator/eseries.cpp", "pcb_calculator/eseries.h",
    "pcb_calculator/resistor_substitution_utils.cpp",
    "pcb_calculator/resistor_substitution_utils.h", "thirdparty/fmt",
    "pcb_calculator/datafile_read_write.h", "pcb_calculator/datafile_read_write.cpp",
    "pcb_calculator/class_regulator_data.h", "pcb_calculator/pcb_calculator_frame.h",
    "pcb_calculator/calculator_panels/calculator_panel.h",
    "pcb_calculator/calculator_panels/panel_regulator.h",
    "pcb_calculator/calculator_panels/panel_regulator_base.h",
    "pcb_calculator/pcb_calculator_datafile.keywords",
    "common/dsnlexer.cpp", "common/richio.cpp", "common/exceptions.cpp",
    "libs/kiplatform/include", "thirdparty/fast_float",
    "cmake/BuildSteps/TokenList2DsnLexer.cmake",
    "pcb_calculator/units_scales.h",
)
