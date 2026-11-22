# SPDX-License-Identifier: GPL-3.0-or-later
"""Single source pin and the complete source closure for the minimal kimath build."""

KICAD_PIN = "4cf939778df04d2b747bb2c4714fc5014a7565c7"
SOURCE_PATHS = (
    "libs/core", "libs/kimath", "include", "thirdparty/clipper2",
    "thirdparty/rtree", "thirdparty/nlohmann_json",
    "cmake/config.h.cmake", "cmake/PerformFeatureChecks.cmake",
    "pcb_calculator/calculator_panels/iec60664.cpp",
    "pcb_calculator/calculator_panels/iec60664.h",
)
