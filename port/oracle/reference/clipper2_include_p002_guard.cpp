// SPDX-License-Identifier: GPL-3.0-or-later
// Undefined-behaviour replay for the clipper.h/clipper.minkowski.h oracle adapter
// (shims/clipper2_include_p002.cpp): the pinned, unmodified clipper.engine.cpp,
// clipper.offset.cpp and clipper.rectclip.cpp (which include clipper.h and its headers),
// compiled by CMakeLists.txt with UBSan in trap mode and _GLIBCXX_ASSERTIONS, in a renamed
// namespace, together with the adapter's own request evaluation. Only ever run in a forked
// child: a trap, failed assertion or fault means the reference evaluation of that request is
// undefined.
#include "registry.h"
#include <clipper2/clipper.version.h>

#define Clipper2Lib include_p002_guard_clipper2
#include "clipper.engine.cpp"
#include "clipper.offset.cpp"
#include "clipper.rectclip.cpp"

#define INCLUDE_P002_NS include_p002_guard
// Enables the replay-only flag bounds trace (GCC 13.3's vector<bool> has no subscript assertion).
#define INCLUDE_P002_REPLAY 1
#include "clipper2_include_p002_ops.inc"
#undef Clipper2Lib
