// SPDX-License-Identifier: GPL-3.0-or-later
// Undefined-behaviour replay of the pinned clipper.rectclip.cpp: compiled by CMakeLists.txt
// with UBSan in trap mode and _GLIBCXX_ASSERTIONS. Only ever run in a forked child.
#define RECTCLIP_REPLAY_NS rectclip_guard
#define Clipper2Lib        rectclip_guard_clipper2
#include "clipper2_rectclip_replay.inc"
