// SPDX-License-Identifier: GPL-3.0-or-later
// Shared body of the rectclip replay compilations. The including file defines
// RECTCLIP_REPLAY_NS and renames Clipper2Lib before including this file; the pinned source is
// included verbatim, then thin wrappers convert plain data and call its functions unchanged.
#include "clipper2_rectclip_replay.h"
#include "clipper.rectclip.cpp"

namespace RECTCLIP_REPLAY_NS
{
namespace
{
Clipper2Lib::Point64 P( const PT& aPt )
{
    Clipper2Lib::Point64 pt;
    pt.x = aPt.x;
    pt.y = aPt.y;
    pt.z = aPt.z;
    return pt;
}

PT Back( const Clipper2Lib::Point64& aPt )
{
    return PT{ aPt.x, aPt.y, aPt.z };
}

Clipper2Lib::Path64 P( const PATH& aPath )
{
    Clipper2Lib::Path64 path;

    for( const PT& pt : aPath )
        path.push_back( P( pt ) );

    return path;
}

Clipper2Lib::Rect64 R( const RECT& aRect )
{
    return Clipper2Lib::Rect64( aRect.left, aRect.top, aRect.right, aRect.bottom );
}

Clipper2Lib::Location L( int aLoc )
{
    return static_cast<Clipper2Lib::Location>( aLoc );
}
} // namespace


std::vector<PATHS> Execute( const RECT& aRect, const std::vector<PATHS>& aCalls, bool aLines )
{
    std::vector<PATHS> out;
    auto               run = [&]( auto& aClipper )
    {
        for( const PATHS& call : aCalls )
        {
            Clipper2Lib::Paths64 paths;

            for( const PATH& path : call )
                paths.push_back( P( path ) );

            PATHS result;

            for( const Clipper2Lib::Path64& path : aClipper.Execute( paths ) )
            {
                PATH converted;

                for( const Clipper2Lib::Point64& pt : path )
                    converted.push_back( Back( pt ) );

                result.push_back( std::move( converted ) );
            }

            out.push_back( std::move( result ) );
        }
    };

    if( aLines )
    {
        Clipper2Lib::RectClipLines64 clipper( R( aRect ) );
        run( clipper );
    }
    else
    {
        Clipper2Lib::RectClip64 clipper( R( aRect ) );
        run( clipper );
    }

    return out;
}


bool Path1ContainsPath2( const PATH& aPath1, const PATH& aPath2 )
{
    return Clipper2Lib::Path1ContainsPath2( P( aPath1 ), P( aPath2 ) );
}


bool GetLocation( const RECT& aRect, const PT& aPt, int& aLoc )
{
    Clipper2Lib::Location loc = L( aLoc );
    const bool            result = Clipper2Lib::GetLocation( R( aRect ), P( aPt ), loc );
    aLoc = static_cast<int>( loc );
    return result;
}


bool IsHorizontal( const PT& aPt1, const PT& aPt2 )
{
    return Clipper2Lib::IsHorizontal( P( aPt1 ), P( aPt2 ) );
}


bool GetSegmentIntersection( const PT& aP1, const PT& aP2, const PT& aP3, const PT& aP4, PT& aIp )
{
    Clipper2Lib::Point64 ip = P( aIp );
    const bool result = Clipper2Lib::GetSegmentIntersection( P( aP1 ), P( aP2 ), P( aP3 ), P( aP4 ), ip );
    aIp = Back( ip );
    return result;
}


bool GetIntersection( const PATH& aRectPath, const PT& aP, const PT& aP2, int& aLoc, PT& aIp )
{
    Clipper2Lib::Location loc = L( aLoc );
    Clipper2Lib::Point64  ip = P( aIp );
    const bool result = Clipper2Lib::GetIntersection( P( aRectPath ), P( aP ), P( aP2 ), loc, ip );
    aLoc = static_cast<int>( loc );
    aIp = Back( ip );
    return result;
}


int GetAdjacentLocation( int aLoc, bool aIsClockwise )
{
    return static_cast<int>( Clipper2Lib::GetAdjacentLocation( L( aLoc ), aIsClockwise ) );
}


bool HeadingClockwise( int aPrev, int aCurr )
{
    return Clipper2Lib::HeadingClockwise( L( aPrev ), L( aCurr ) );
}


bool AreOpposites( int aPrev, int aCurr )
{
    return Clipper2Lib::AreOpposites( L( aPrev ), L( aCurr ) );
}


bool IsClockwise( int aPrev, int aCurr, const PT& aPrevPt, const PT& aCurrPt, const PT& aRectMp )
{
    return Clipper2Lib::IsClockwise( L( aPrev ), L( aCurr ), P( aPrevPt ), P( aCurrPt ), P( aRectMp ) );
}


uint32_t GetEdgesForPt( const PT& aPt, const RECT& aRect )
{
    return Clipper2Lib::GetEdgesForPt( P( aPt ), R( aRect ) );
}


bool IsHeadingClockwise( const PT& aPt1, const PT& aPt2, int aEdgeIdx )
{
    return Clipper2Lib::IsHeadingClockwise( P( aPt1 ), P( aPt2 ), aEdgeIdx );
}


bool HasHorzOverlap( const PT& aL1, const PT& aR1, const PT& aL2, const PT& aR2 )
{
    return Clipper2Lib::HasHorzOverlap( P( aL1 ), P( aR1 ), P( aL2 ), P( aR2 ) );
}


bool HasVertOverlap( const PT& aT1, const PT& aB1, const PT& aT2, const PT& aB2 )
{
    return Clipper2Lib::HasVertOverlap( P( aT1 ), P( aB1 ), P( aT2 ), P( aB2 ) );
}


bool StartLocsAreClockwise( const std::vector<int>& aLocs )
{
    std::vector<Clipper2Lib::Location> locs;

    for( int loc : aLocs )
        locs.push_back( L( loc ) );

    return Clipper2Lib::StartLocsAreClockwise( locs );
}
} // namespace RECTCLIP_REPLAY_NS
