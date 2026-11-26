// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p003
// (thirdparty/clipper2/Clipper2Lib/src/clipper.rectclip.cpp).
//
// Results come from the pinned code itself:
// - rect_clip / rect_clip_lines construct the genuine Clipper2Lib::RectClip64 /
//   RectClipLines64 from the pinned clipper2 archive and call Execute once per entry of
//   "calls" on the same object;
// - get_segment_intersection and start_locs_are_clockwise call the archive's exported
//   (non-inline) Clipper2Lib functions;
// - the file's inline helpers, which the archive need not emit, are called in
//   reference/clipper2_rectclip_copy.cpp, an uninstrumented second compilation of the same
//   unmodified source with the same flags and definitions (renamed namespace).
//
// Undefined behaviour is decided by the same source, not by a hand-written model: every
// request first runs in a forked child on reference/clipper2_rectclip_guard.cpp, the pinned
// source compiled with UBSan in trap mode and _GLIBCXX_ASSERTIONS. A trap, failed assertion or
// fault there refuses the request as "undefined behaviour: <op>". A child that exceeds the
// replay time or memory limit refuses it as "harness limit: <op>" (a harness bound, not a
// claim about the reference). Only a cleanly finished replay lets the genuine call run.
#include "registry.h"
#include "clipper2_rectclip_replay.h"
#include <clipper2/clipper.core.h>
#include <clipper2/clipper.rectclip.h>
#include <cerrno>
#include <csignal>
#include <cstdint>
#include <cstdio>
#include <fcntl.h>
#include <initializer_list>
#include <iostream>
#include <new>
#include <string>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>
#include <vector>

#ifndef USINGZ
#error "KiCad compiles Clipper2 with the public USINGZ definition"
#endif

// The archive's exported definitions in clipper.rectclip.cpp; the file declares no header.
namespace Clipper2Lib
{
bool GetSegmentIntersection( const Point64& p1, const Point64& p2, const Point64& p3,
                             const Point64& p4, Point64& ip );
bool StartLocsAreClockwise( const std::vector<Location>& startlocs );
} // namespace Clipper2Lib

namespace
{
using oracle::JSON;
namespace c2 = Clipper2Lib;
using rectclip_replay::PATH;
using rectclip_replay::PATHS;
using rectclip_replay::PT;
using rectclip_replay::RECT;

constexpr const char* RECTCLIP_UNIT = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p003";
// Request size limits, identical in the Rust shim.
constexpr size_t MAX_POINTS = 1 << 16;
constexpr size_t MAX_CALLS = 64;
// Replay limits for the forked UB guard.
constexpr unsigned REPLAY_SECONDS = 20;
constexpr rlim_t   REPLAY_BYTES = rlim_t( 4 ) << 30;

[[noreturn]] void Invalid( const std::string& aWhat )
{
    throw std::invalid_argument( "invalid input: " + aWhat );
}


void Keys( const JSON& aValue, std::initializer_list<const char*> aFields )
{
    if( !aValue.is_object() || aValue.size() != aFields.size() )
        Invalid( "fields" );

    for( const char* field : aFields )
    {
        if( !aValue.contains( field ) )
            Invalid( "fields" );
    }
}


int64_t I64( const JSON& aValue )
{
    if( !aValue.is_number_integer()
        || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > static_cast<uint64_t>( INT64_MAX ) ) )
    {
        Invalid( "int64" );
    }

    return aValue.get<int64_t>();
}


int I32( const JSON& aValue )
{
    const int64_t value = I64( aValue );

    if( value < INT32_MIN || value > INT32_MAX )
        Invalid( "int" );

    return static_cast<int>( value );
}


bool Bool( const JSON& aValue )
{
    if( !aValue.is_boolean() )
        Invalid( "boolean" );

    return aValue.get<bool>();
}


// A Location value: the enumerators 0..4 only.
int Loc( const JSON& aValue )
{
    const int64_t value = I64( aValue );

    if( value < 0 || value > 4 )
        Invalid( "location" );

    return static_cast<int>( value );
}


struct COUNTER
{
    size_t points = 0;

    void Add( size_t aCount )
    {
        points += aCount;

        if( points > MAX_POINTS )
            Invalid( "size" );
    }
};


PT Point( const JSON& aValue, COUNTER& aCount )
{
    if( !aValue.is_array() || aValue.size() != 3 )
        Invalid( "point" );

    aCount.Add( 1 );
    return PT{ I64( aValue[0] ), I64( aValue[1] ), I64( aValue[2] ) };
}


PATH Path( const JSON& aValue, COUNTER& aCount )
{
    if( !aValue.is_array() )
        Invalid( "path" );

    PATH path;

    for( const JSON& point : aValue )
        path.push_back( Point( point, aCount ) );

    return path;
}


PATHS Paths( const JSON& aValue, COUNTER& aCount )
{
    if( !aValue.is_array() )
        Invalid( "paths" );

    PATHS paths;

    for( const JSON& path : aValue )
        paths.push_back( Path( path, aCount ) );

    return paths;
}


RECT Rect( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 4 )
        Invalid( "rect" );

    return RECT{ I64( aValue[0] ), I64( aValue[1] ), I64( aValue[2] ), I64( aValue[3] ) };
}


std::vector<PT> Points( const JSON& aValue, size_t aCount, COUNTER& aCounter )
{
    if( !aValue.is_array() || aValue.size() != aCount )
        Invalid( "points" );

    return Path( aValue, aCounter );
}


JSON Write( const PT& aPt )
{
    return JSON::array( { aPt.x, aPt.y, aPt.z } );
}


JSON Write( const PATHS& aPaths )
{
    JSON out = JSON::array();

    for( const PATH& path : aPaths )
    {
        JSON p = JSON::array();

        for( const PT& pt : path )
            p.push_back( Write( pt ) );

        out.push_back( std::move( p ) );
    }

    return out;
}


c2::Point64 C( const PT& aPt )
{
    c2::Point64 pt;
    pt.x = aPt.x;
    pt.y = aPt.y;
    pt.z = aPt.z;
    return pt;
}


PT Back( const c2::Point64& aPt )
{
    return PT{ aPt.x, aPt.y, aPt.z };
}


// The genuine archive's RectClip64 / RectClipLines64, one Execute per call on one object.
std::vector<PATHS> GenuineExecute( const RECT& aRect, const std::vector<PATHS>& aCalls, bool aLines )
{
    std::vector<PATHS> out;
    auto               run = [&]( auto& aClipper )
    {
        for( const PATHS& call : aCalls )
        {
            c2::Paths64 paths;

            for( const PATH& path : call )
            {
                c2::Path64 p;

                for( const PT& pt : path )
                    p.push_back( C( pt ) );

                paths.push_back( std::move( p ) );
            }

            PATHS result;

            for( const c2::Path64& path : aClipper.Execute( paths ) )
            {
                PATH converted;

                for( const c2::Point64& pt : path )
                    converted.push_back( Back( pt ) );

                result.push_back( std::move( converted ) );
            }

            out.push_back( std::move( result ) );
        }
    };
    const c2::Rect64 rect( aRect.left, aRect.top, aRect.right, aRect.bottom );

    if( aLines )
    {
        c2::RectClipLines64 clipper( rect );
        run( clipper );
    }
    else
    {
        c2::RectClip64 clipper( rect );
        run( clipper );
    }

    return out;
}


// Evaluate one parsed request. GUARD selects the instrumented replay for every call; the real
// evaluation uses the genuine archive where it exports the function, else the plain copy.
template <bool GUARD>
JSON Evaluate( const std::string& aOp, const JSON& aIn )
{
    namespace copy = rectclip_copy;
    namespace guard = rectclip_guard;
    COUNTER count;

    if( aOp == "rect_clip" || aOp == "rect_clip_lines" )
    {
        Keys( aIn, { "op", "rect", "calls" } );
        const RECT  rect = Rect( aIn.at( "rect" ) );
        const JSON& calls = aIn.at( "calls" );

        if( !calls.is_array() || calls.size() > MAX_CALLS )
            Invalid( "calls" );

        std::vector<PATHS> input;

        for( const JSON& call : calls )
            input.push_back( Paths( call, count ) );

        const bool         lines = aOp == "rect_clip_lines";
        std::vector<PATHS> out = GUARD ? guard::Execute( rect, input, lines )
                                       : GenuineExecute( rect, input, lines );
        JSON results = JSON::array();

        for( const PATHS& paths : out )
            results.push_back( Write( paths ) );

        return JSON{ { "results", results } };
    }

    if( aOp == "path1_contains_path2" )
    {
        Keys( aIn, { "op", "path1", "path2" } );
        const PATH a = Path( aIn.at( "path1" ), count );
        const PATH b = Path( aIn.at( "path2" ), count );
        return GUARD ? guard::Path1ContainsPath2( a, b ) : copy::Path1ContainsPath2( a, b );
    }

    if( aOp == "get_location" )
    {
        Keys( aIn, { "op", "rect", "pt", "loc" } );
        const RECT rect = Rect( aIn.at( "rect" ) );
        const PT   pt = Point( aIn.at( "pt" ), count );
        int        loc = Loc( aIn.at( "loc" ) );
        const bool result = GUARD ? guard::GetLocation( rect, pt, loc ) : copy::GetLocation( rect, pt, loc );
        return JSON{ { "result", result }, { "loc", loc } };
    }

    if( aOp == "is_horizontal" )
    {
        Keys( aIn, { "op", "points" } );
        const std::vector<PT> p = Points( aIn.at( "points" ), 2, count );
        return GUARD ? guard::IsHorizontal( p[0], p[1] ) : copy::IsHorizontal( p[0], p[1] );
    }

    if( aOp == "get_segment_intersection" )
    {
        Keys( aIn, { "op", "points", "ip" } );
        const std::vector<PT> p = Points( aIn.at( "points" ), 4, count );
        PT                    ip = Point( aIn.at( "ip" ), count );
        bool                  result;

        if( GUARD )
        {
            result = guard::GetSegmentIntersection( p[0], p[1], p[2], p[3], ip );
        }
        else
        {
            c2::Point64 cip = C( ip );
            result = c2::GetSegmentIntersection( C( p[0] ), C( p[1] ), C( p[2] ), C( p[3] ), cip );
            ip = Back( cip );
        }

        return JSON{ { "result", result }, { "ip", Write( ip ) } };
    }

    if( aOp == "get_intersection" )
    {
        Keys( aIn, { "op", "rect_path", "p", "p2", "loc", "ip" } );
        const PATH rectPath = Path( aIn.at( "rect_path" ), count );
        const PT   p = Point( aIn.at( "p" ), count );
        const PT   p2 = Point( aIn.at( "p2" ), count );
        int        loc = Loc( aIn.at( "loc" ) );
        PT         ip = Point( aIn.at( "ip" ), count );
        const bool result = GUARD ? guard::GetIntersection( rectPath, p, p2, loc, ip )
                                  : copy::GetIntersection( rectPath, p, p2, loc, ip );
        return JSON{ { "result", result }, { "loc", loc }, { "ip", Write( ip ) } };
    }

    if( aOp == "get_adjacent_location" )
    {
        Keys( aIn, { "op", "loc", "clockwise" } );
        const int  loc = Loc( aIn.at( "loc" ) );
        const bool cw = Bool( aIn.at( "clockwise" ) );
        return GUARD ? guard::GetAdjacentLocation( loc, cw ) : copy::GetAdjacentLocation( loc, cw );
    }

    if( aOp == "heading_clockwise" || aOp == "are_opposites" )
    {
        Keys( aIn, { "op", "prev", "curr" } );
        const int prev = Loc( aIn.at( "prev" ) );
        const int curr = Loc( aIn.at( "curr" ) );

        if( aOp == "heading_clockwise" )
            return GUARD ? guard::HeadingClockwise( prev, curr ) : copy::HeadingClockwise( prev, curr );

        return GUARD ? guard::AreOpposites( prev, curr ) : copy::AreOpposites( prev, curr );
    }

    if( aOp == "is_clockwise" )
    {
        Keys( aIn, { "op", "prev", "curr", "prev_pt", "curr_pt", "rect_mp" } );
        const int prev = Loc( aIn.at( "prev" ) );
        const int curr = Loc( aIn.at( "curr" ) );
        const PT  a = Point( aIn.at( "prev_pt" ), count );
        const PT  b = Point( aIn.at( "curr_pt" ), count );
        const PT  m = Point( aIn.at( "rect_mp" ), count );
        return GUARD ? guard::IsClockwise( prev, curr, a, b, m ) : copy::IsClockwise( prev, curr, a, b, m );
    }

    if( aOp == "get_edges_for_pt" )
    {
        Keys( aIn, { "op", "pt", "rect" } );
        const PT   pt = Point( aIn.at( "pt" ), count );
        const RECT rect = Rect( aIn.at( "rect" ) );
        return GUARD ? guard::GetEdgesForPt( pt, rect ) : copy::GetEdgesForPt( pt, rect );
    }

    if( aOp == "is_heading_clockwise" )
    {
        Keys( aIn, { "op", "points", "edge" } );
        const std::vector<PT> p = Points( aIn.at( "points" ), 2, count );
        const int             edge = I32( aIn.at( "edge" ) );
        return GUARD ? guard::IsHeadingClockwise( p[0], p[1], edge ) : copy::IsHeadingClockwise( p[0], p[1], edge );
    }

    if( aOp == "has_horz_overlap" || aOp == "has_vert_overlap" )
    {
        Keys( aIn, { "op", "points" } );
        const std::vector<PT> p = Points( aIn.at( "points" ), 4, count );

        if( aOp == "has_horz_overlap" )
        {
            return GUARD ? guard::HasHorzOverlap( p[0], p[1], p[2], p[3] )
                         : copy::HasHorzOverlap( p[0], p[1], p[2], p[3] );
        }

        return GUARD ? guard::HasVertOverlap( p[0], p[1], p[2], p[3] )
                     : copy::HasVertOverlap( p[0], p[1], p[2], p[3] );
    }

    if( aOp == "start_locs_are_clockwise" )
    {
        Keys( aIn, { "op", "locs" } );
        const JSON& locs = aIn.at( "locs" );

        if( !locs.is_array() || locs.size() > MAX_POINTS )
            Invalid( "locs" );

        std::vector<int>          ints;
        std::vector<c2::Location> values;

        for( const JSON& loc : locs )
        {
            ints.push_back( Loc( loc ) );
            values.push_back( static_cast<c2::Location>( ints.back() ) );
        }

        return GUARD ? guard::StartLocsAreClockwise( ints ) : c2::StartLocsAreClockwise( values );
    }

    Invalid( "op" );
}


// Run the instrumented replay in a child process: 0 clean (or a malformed request, which the
// parent's own evaluation rejects before any reference call), 1 undefined behaviour, 2 harness
// limit.
int Replay( const std::string& aOp, const JSON& aIn )
{
    std::cout.flush();
    std::cerr.flush();
    std::fflush( nullptr );
    const pid_t pid = fork();

    if( pid < 0 )
        throw std::runtime_error( "replay fork failed" );

    if( pid == 0 )
    {
        const int null = open( "/dev/null", O_WRONLY );

        if( null >= 0 )
        {
            dup2( null, STDOUT_FILENO );
            dup2( null, STDERR_FILENO );
        }

        const rlimit limit{ REPLAY_BYTES, REPLAY_BYTES };
        setrlimit( RLIMIT_AS, &limit );
        alarm( REPLAY_SECONDS );

        try
        {
            Evaluate<true>( aOp, aIn );
        }
        catch( const std::bad_alloc& )
        {
            _exit( 3 );
        }
        catch( const std::invalid_argument& )
        {
            // Malformed request: the parent reports it; every Evaluate branch validates its
            // whole input before calling any reference code.
            _exit( 5 );
        }
        catch( ... )
        {
            _exit( 4 );
        }

        _exit( 0 );
    }

    int status = 0;

    while( waitpid( pid, &status, 0 ) < 0 )
    {
        if( errno != EINTR )
            throw std::runtime_error( "replay wait failed" );
    }

    if( WIFEXITED( status ) )
    {
        switch( WEXITSTATUS( status ) )
        {
        case 0:
        case 5: return 0;
        case 3: return 2;
        default: throw std::runtime_error( "replay failed" );
        }
    }

    if( WIFSIGNALED( status ) )
    {
        switch( WTERMSIG( status ) )
        {
        case SIGILL:
        case SIGTRAP:
        case SIGABRT:
        case SIGSEGV:
        case SIGBUS:
        case SIGFPE: return 1;
        case SIGALRM:
        case SIGKILL: return 2;
        default: break;
        }
    }

    throw std::runtime_error( "replay ended abnormally" );
}


JSON Run( const JSON& aIn )
{
    if( !aIn.is_object() || !aIn.contains( "op" ) || !aIn.at( "op" ).is_string() )
        Invalid( "op" );

    const std::string op = aIn.at( "op" ).get<std::string>();

    switch( Replay( op, aIn ) )
    {
    case 0: return Evaluate<false>( op, aIn );
    case 1: throw std::invalid_argument( "undefined behaviour: " + op );
    default: throw std::invalid_argument( "harness limit: " + op );
    }
}
} // namespace


namespace oracle
{
void RegisterClipper2RectClip( REGISTRY& aRegistry )
{
    aRegistry.Add( RECTCLIP_UNIT, "thirdparty/clipper2/Clipper2Lib/src/clipper.rectclip.cpp", Run );
}
} // namespace oracle
