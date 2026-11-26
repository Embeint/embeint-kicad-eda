// SPDX-License-Identifier: GPL-3.0-or-later
// Plain-data interface to the two extra compilations of the pinned, unmodified
// thirdparty/clipper2/Clipper2Lib/src/clipper.rectclip.cpp used by the rectangle clipping
// oracle adapter (shims/clipper2_rectclip.cpp):
//
// - rectclip_guard: compiled with UBSan in trap mode (signed overflow, float-cast-overflow,
//   bounds, null, shift, ...) and _GLIBCXX_ASSERTIONS (std::vector subscripts). The adapter
//   runs a request on it in a forked child first; a trap or assertion there means the same
//   source executes undefined behaviour on that input, and the request is refused.
// - rectclip_copy: an uninstrumented copy, so the adapter can call the file's inline helpers
//   (GetLocation, GetIntersection, ...), which the pinned archive need not emit.
//
// Each compilation renames namespace Clipper2Lib (by macro, in its own translation unit) so
// its symbols never collide with the genuine clipper2 archive, which the adapter calls for
// RectClip64/RectClipLines64 and the exported GetSegmentIntersection/StartLocsAreClockwise.
#pragma once
#include <cstdint>
#include <vector>

namespace rectclip_replay
{
struct PT
{
    int64_t x = 0;
    int64_t y = 0;
    int64_t z = 0;
};

struct RECT
{
    int64_t left = 0;
    int64_t top = 0;
    int64_t right = 0;
    int64_t bottom = 0;
};

using PATH = std::vector<PT>;
using PATHS = std::vector<PATH>;
} // namespace rectclip_replay

#define RECTCLIP_REPLAY_API( NS )                                                                  \
    namespace NS                                                                                   \
    {                                                                                              \
    using namespace rectclip_replay;                                                               \
    std::vector<PATHS> Execute( const RECT& aRect, const std::vector<PATHS>& aCalls, bool aLines ); \
    bool               Path1ContainsPath2( const PATH& aPath1, const PATH& aPath2 );               \
    bool               GetLocation( const RECT& aRect, const PT& aPt, int& aLoc );                 \
    bool               IsHorizontal( const PT& aPt1, const PT& aPt2 );                             \
    bool GetSegmentIntersection( const PT& aP1, const PT& aP2, const PT& aP3, const PT& aP4,      \
                                 PT& aIp );                                                        \
    bool GetIntersection( const PATH& aRectPath, const PT& aP, const PT& aP2, int& aLoc, PT& aIp ); \
    int  GetAdjacentLocation( int aLoc, bool aIsClockwise );                                       \
    bool HeadingClockwise( int aPrev, int aCurr );                                                 \
    bool AreOpposites( int aPrev, int aCurr );                                                     \
    bool IsClockwise( int aPrev, int aCurr, const PT& aPrevPt, const PT& aCurrPt,                  \
                      const PT& aRectMp );                                                         \
    uint32_t GetEdgesForPt( const PT& aPt, const RECT& aRect );                                    \
    bool     IsHeadingClockwise( const PT& aPt1, const PT& aPt2, int aEdgeIdx );                   \
    bool     HasHorzOverlap( const PT& aL1, const PT& aR1, const PT& aL2, const PT& aR2 );         \
    bool     HasVertOverlap( const PT& aT1, const PT& aB1, const PT& aT2, const PT& aB2 );         \
    bool     StartLocsAreClockwise( const std::vector<int>& aLocs );                               \
    }

RECTCLIP_REPLAY_API( rectclip_guard )
RECTCLIP_REPLAY_API( rectclip_copy )
