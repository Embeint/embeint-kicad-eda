// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002: lines 2272-3337
// of the pinned, unmodified clipper.engine.cpp and the whole of clipper.offset.cpp. Several of
// these bodies are file-local (inline/static) functions that no other translation unit can
// call, so this adapter compiles both pinned source files into this translation unit, in its
// own namespace (the identifier Clipper2Lib is renamed, nothing else changes), with the
// oracle's flags and KiCad's public USINGZ definition. The engine bodies run through the
// public Clipper64/ClipperD/ClipperOffset operations, which reach every one of them; the
// offset's file-local helpers are also called directly. Every request mirrors
// rust/kicad-clipper2-p002-shim request for request, including its input domain and resource
// guards, which refuse before any reference code runs. Doubles travel as their 16 hex digit
// bit patterns.
#include "registry.h"
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <functional>
#include <limits>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#define Clipper2Lib Clipper2EngineP002
#include "clipper.engine.cpp"
#include "clipper.offset.cpp"
#undef Clipper2Lib

#ifndef USINGZ
#error "KiCad compiles Clipper2 with the public USINGZ definition"
#endif

namespace
{
using oracle::JSON;
namespace c2 = Clipper2EngineP002;

constexpr const char* ENGINE_P002_UNIT = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002";
constexpr size_t      MAX_POINTS = size_t( 1 ) << 16;
constexpr size_t      MAX_GROUPS = 64;
constexpr size_t      MAX_EXECUTES = 4;
constexpr size_t      MAX_RUNS = 2;
constexpr size_t      MAX_TABLE = 64;
// Clipper2's declared coordinate range (INT64_MAX >> 2), the engine domain.
constexpr int64_t     ENGINE_COORD = std::numeric_limits<int64_t>::max() >> 2;
// The offset domain: coordinates and deltas whose offsets stay far inside the engine domain.
constexpr double      OFFSET_LIMIT = 1099511627776.0; // 2^40
constexpr int64_t     OFFSET_COORD = int64_t( 1 ) << 40;
constexpr double      MAX_ARC_STEPS = 65536.0;
constexpr double      MAX_OFFSET_POINTS = 4194304.0; // 2^22
constexpr int64_t     Z_BASE = 1000000;

// --- access to private members (explicit instantiation is exempt from access checks) -----

template <typename TAG, auto MEMBER>
struct ROB
{
    friend auto Get( TAG ) { return MEMBER; }
};

#define ROB_MEMBER( TAG, MEMBER )                                                               \
    struct TAG                                                                                  \
    {                                                                                           \
        friend auto Get( TAG );                                                                 \
    };                                                                                          \
    template struct ROB<TAG, MEMBER>

ROB_MEMBER( OFFSET_GROUPS, &c2::ClipperOffset::groups_ );
ROB_MEMBER( OFFSET_NORMS, &c2::ClipperOffset::norms );

// --- input ---------------------------------------------------------------------------

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


const JSON& At( const JSON& aValue, const char* aName )
{
    if( !aValue.is_object() || !aValue.contains( aName ) )
        Invalid( aName );

    return aValue.at( aName );
}


const std::string& Text( const JSON& aValue )
{
    if( !aValue.is_string() )
        Invalid( "string" );

    return aValue.get_ref<const std::string&>();
}


bool Bool( const JSON& aValue )
{
    if( !aValue.is_boolean() )
        Invalid( "boolean" );

    return aValue.get<bool>();
}


int64_t I64( const JSON& aValue )
{
    if( aValue.is_number_unsigned() )
    {
        if( aValue.get<uint64_t>() > uint64_t( std::numeric_limits<int64_t>::max() ) )
            Invalid( "int64" );

        return int64_t( aValue.get<uint64_t>() );
    }

    if( !aValue.is_number_integer() )
        Invalid( "int64" );

    return aValue.get<int64_t>();
}


int Enum( const JSON& aValue, int aCount )
{
    const int64_t v = I64( aValue );

    if( v < 0 || v >= aCount )
        Invalid( "enum" );

    return int( v );
}


double F64( const JSON& aValue )
{
    if( !aValue.is_string() )
        Invalid( "double" );

    const std::string& s = aValue.get_ref<const std::string&>();

    if( s.size() != 16 )
        Invalid( "double" );

    uint64_t bits = 0;

    for( char c : s )
    {
        uint64_t digit;

        if( c >= '0' && c <= '9' )
            digit = uint64_t( c - '0' );
        else if( c >= 'a' && c <= 'f' )
            digit = uint64_t( c - 'a' + 10 );
        else
            Invalid( "double" );

        bits = ( bits << 4 ) | digit;
    }

    double value;
    std::memcpy( &value, &bits, sizeof value );
    return value;
}


JSON Bits( double aValue )
{
    uint64_t bits;
    std::memcpy( &bits, &aValue, sizeof bits );
    char text[17];
    std::snprintf( text, sizeof text, "%016llx", static_cast<unsigned long long>( bits ) );
    return std::string( text );
}


/// Points read so far in one request.
struct COUNTER
{
    size_t n = 0;

    void Add( size_t aCount )
    {
        n += aCount;

        if( n > MAX_POINTS )
            Invalid( "size" );
    }
};


const JSON& Array( const JSON& aValue, const char* aWhat )
{
    if( !aValue.is_array() )
        Invalid( aWhat );

    return aValue;
}


c2::Point64 ReadPoint( const JSON& aValue, COUNTER& aCount )
{
    if( !aValue.is_array() || aValue.size() != 3 )
        Invalid( "point" );

    aCount.Add( 1 );
    c2::Point64 p;
    p.x = I64( aValue[0] );
    p.y = I64( aValue[1] );
    p.z = I64( aValue[2] );
    return p;
}


c2::Paths64 ReadPaths( const JSON& aValue, COUNTER& aCount )
{
    c2::Paths64 paths;

    for( const JSON& path : Array( aValue, "paths" ) )
    {
        c2::Path64 out;

        for( const JSON& p : Array( path, "path" ) )
            out.push_back( ReadPoint( p, aCount ) );

        paths.push_back( std::move( out ) );
    }

    return paths;
}


c2::PointD ReadPointD( const JSON& aValue, COUNTER& aCount )
{
    if( !aValue.is_array() || aValue.size() != 3 )
        Invalid( "point" );

    aCount.Add( 1 );
    c2::PointD p;
    p.x = F64( aValue[0] );
    p.y = F64( aValue[1] );
    p.z = I64( aValue[2] );
    return p;
}


c2::PathsD ReadPathsD( const JSON& aValue, COUNTER& aCount )
{
    c2::PathsD paths;

    for( const JSON& path : Array( aValue, "paths" ) )
    {
        c2::PathD out;

        for( const JSON& p : Array( path, "path" ) )
            out.push_back( ReadPointD( p, aCount ) );

        paths.push_back( std::move( out ) );
    }

    return paths;
}


JSON WritePoint( const c2::Point64& aPoint )
{
    return JSON::array( { aPoint.x, aPoint.y, aPoint.z } );
}


JSON WritePointD( const c2::PointD& aPoint )
{
    return JSON::array( { Bits( aPoint.x ), Bits( aPoint.y ), aPoint.z } );
}


JSON WritePath( const c2::Path64& aPath )
{
    JSON p = JSON::array();

    for( const c2::Point64& pt : aPath )
        p.push_back( WritePoint( pt ) );

    return p;
}


JSON WritePaths( const c2::Paths64& aPaths )
{
    JSON out = JSON::array();

    for( const c2::Path64& path : aPaths )
        out.push_back( WritePath( path ) );

    return out;
}


JSON WritePathD( const c2::PathD& aPath )
{
    JSON p = JSON::array();

    for( const c2::PointD& pt : aPath )
        p.push_back( WritePointD( pt ) );

    return p;
}


JSON WritePathsD( const c2::PathsD& aPaths )
{
    JSON out = JSON::array();

    for( const c2::PathD& path : aPaths )
        out.push_back( WritePathD( path ) );

    return out;
}


/// A tree in preorder: each node's parent index (-1 for the root's children), hole flag and
/// polygon.
template <typename NODE, typename WRITE>
void WriteTree( const NODE& aNode, int64_t aParent, JSON& aOut, WRITE aWrite )
{
    for( size_t i = 0; i < aNode.Count(); ++i )
    {
        const auto* child = aNode.Child( i );
        const int64_t index = int64_t( aOut.size() );
        aOut.push_back( JSON{ { "parent", aParent },
                              { "hole", child->IsHole() },
                              { "polygon", aWrite( child->Polygon() ) } } );
        WriteTree( *child, index, aOut, aWrite );
    }
}


void EngineDomain( const c2::Paths64& aPaths )
{
    for( const c2::Path64& path : aPaths )
    {
        for( const c2::Point64& p : path )
        {
            if( p.x < -ENGINE_COORD || p.x > ENGINE_COORD || p.y < -ENGINE_COORD
                || p.y > ENGINE_COORD )
                Invalid( "domain" );
        }
    }
}


void NotNan( const c2::PathsD& aPaths )
{
    for( const c2::PathD& path : aPaths )
    {
        for( const c2::PointD& p : path )
        {
            if( std::isnan( p.x ) || std::isnan( p.y ) )
                Invalid( "domain" );
        }
    }
}


c2::ClipType ReadClipType( const JSON& aValue )
{
    return static_cast<c2::ClipType>( Enum( aValue, 5 ) );
}


c2::FillRule ReadFillRule( const JSON& aValue )
{
    return static_cast<c2::FillRule>( Enum( aValue, 4 ) );
}


size_t Runs( const JSON& aValue )
{
    const int64_t runs = I64( aValue );

    if( runs < 1 || runs > int64_t( MAX_RUNS ) )
        Invalid( "runs" );

    return size_t( runs );
}


bool Recorder( const JSON& aValue )
{
    const std::string& z = Text( aValue );

    if( z == "none" )
        return false;

    if( z == "recorder" )
        return true;

    Invalid( "z" );
}


/// The output form: closed paths only, closed and open paths, a tree, or a tree and open
/// paths.
int Output( const JSON& aValue )
{
    const std::string& o = Text( aValue );

    if( o == "paths" )
        return 0;

    if( o == "paths_open" )
        return 1;

    if( o == "tree" )
        return 2;

    if( o == "tree_open" )
        return 3;

    Invalid( "output" );
}

// --- operations ----------------------------------------------------------------------

JSON RunClip64( const JSON& aIn )
{
    Keys( aIn, { "op", "subjects", "open", "clips", "clip_type", "fill_rule",
                 "preserve_collinear", "reverse_solution", "pip_threshold", "z", "output",
                 "runs" } );
    COUNTER           count;
    const c2::Paths64 subjects = ReadPaths( At( aIn, "subjects" ), count );
    const c2::Paths64 open = ReadPaths( At( aIn, "open" ), count );
    const c2::Paths64 clips = ReadPaths( At( aIn, "clips" ), count );
    const c2::ClipType ct = ReadClipType( At( aIn, "clip_type" ) );
    const c2::FillRule fr = ReadFillRule( At( aIn, "fill_rule" ) );
    const bool         preserve = Bool( At( aIn, "preserve_collinear" ) );
    const bool         reverse = Bool( At( aIn, "reverse_solution" ) );
    const int64_t      pip = I64( At( aIn, "pip_threshold" ) );
    const bool         recorder = Recorder( At( aIn, "z" ) );
    const int          output = Output( At( aIn, "output" ) );
    const size_t       runs = Runs( At( aIn, "runs" ) );

    if( pip < 0 )
        Invalid( "int64" );

    EngineDomain( subjects );
    EngineDomain( open );
    EngineDomain( clips );

    auto calls = std::make_shared<JSON>( JSON::array() );
    JSON results = JSON::array();

    try
    {
        c2::Clipper64 c;
        c.PreserveCollinear( preserve );
        c.ReverseSolution( reverse );
        c.PipIndexThreshold( size_t( pip ) );

        if( recorder )
        {
            c.SetZCallback(
                    [calls]( const c2::Point64& e1b, const c2::Point64& e1t, const c2::Point64& e2b,
                             const c2::Point64& e2t, c2::Point64& pt )
                    {
                        calls->push_back( JSON::array( { WritePoint( e1b ), WritePoint( e1t ),
                                                         WritePoint( e2b ), WritePoint( e2t ),
                                                         WritePoint( pt ) } ) );
                        pt.z = Z_BASE + int64_t( calls->size() );
                    } );
        }

        c.AddSubject( subjects );
        c.AddOpenSubject( open );
        c.AddClip( clips );

        for( size_t run = 0; run < runs; ++run )
        {
            c2::Paths64 closed, openOut;
            JSON        tree = nullptr;
            bool        ok;

            if( output == 0 )
                ok = c.Execute( ct, fr, closed );
            else if( output == 1 )
                ok = c.Execute( ct, fr, closed, openOut );
            else
            {
                c2::PolyTree64 polytree;

                if( output == 2 )
                    ok = c.Execute( ct, fr, polytree );
                else
                    ok = c.Execute( ct, fr, polytree, openOut );

                tree = JSON::array();
                WriteTree( polytree, -1, tree, WritePath );
            }

            results.push_back( JSON{ { "succeeded", ok },
                                     { "closed", WritePaths( closed ) },
                                     { "open", WritePaths( openOut ) },
                                     { "tree", tree },
                                     { "error_code", c.ErrorCode() } } );
        }
    }
    catch( const c2::Clipper2Exception& e )
    {
        results.push_back( JSON{ { "exception", e.what() } } );
    }

    return JSON{ { "runs", results }, { "z_calls", *calls } };
}


JSON RunClipD( const JSON& aIn )
{
    Keys( aIn, { "op", "precision", "subjects", "open", "clips", "clip_type", "fill_rule",
                 "preserve_collinear", "reverse_solution", "z", "output", "runs" } );
    const int64_t     precision = I64( At( aIn, "precision" ) );
    COUNTER           count;
    const c2::PathsD  subjects = ReadPathsD( At( aIn, "subjects" ), count );
    const c2::PathsD  open = ReadPathsD( At( aIn, "open" ), count );
    const c2::PathsD  clips = ReadPathsD( At( aIn, "clips" ), count );
    const c2::ClipType ct = ReadClipType( At( aIn, "clip_type" ) );
    const c2::FillRule fr = ReadFillRule( At( aIn, "fill_rule" ) );
    const bool         preserve = Bool( At( aIn, "preserve_collinear" ) );
    const bool         reverse = Bool( At( aIn, "reverse_solution" ) );
    const bool         recorder = Recorder( At( aIn, "z" ) );
    const int          output = Output( At( aIn, "output" ) );
    const size_t       runs = Runs( At( aIn, "runs" ) );

    if( precision < -64 || precision > 64 )
        Invalid( "int" );

    NotNan( subjects );
    NotNan( open );
    NotNan( clips );

    auto calls = std::make_shared<JSON>( JSON::array() );
    JSON results = JSON::array();

    try
    {
        c2::ClipperD c{ static_cast<int>( precision ) };
        c.PreserveCollinear( preserve );
        c.ReverseSolution( reverse );

        if( recorder )
        {
            c.SetZCallback(
                    [calls]( const c2::PointD& e1b, const c2::PointD& e1t, const c2::PointD& e2b,
                             const c2::PointD& e2t, c2::PointD& pt )
                    {
                        calls->push_back( JSON::array( { WritePointD( e1b ), WritePointD( e1t ),
                                                         WritePointD( e2b ), WritePointD( e2t ),
                                                         WritePointD( pt ) } ) );
                        pt.z = Z_BASE + int64_t( calls->size() );
                    } );
        }

        c.AddSubject( subjects );
        c.AddOpenSubject( open );
        c.AddClip( clips );

        for( size_t run = 0; run < runs; ++run )
        {
            c2::PathsD closed, openOut;
            JSON       tree = nullptr;
            bool       ok;

            if( output == 0 )
                ok = c.Execute( ct, fr, closed );
            else if( output == 1 )
                ok = c.Execute( ct, fr, closed, openOut );
            else
            {
                c2::PolyTreeD polytree;

                if( output == 2 )
                    ok = c.Execute( ct, fr, polytree );
                else
                    ok = c.Execute( ct, fr, polytree, openOut );

                tree = JSON::array();
                WriteTree( polytree, -1, tree, WritePathD );
            }

            results.push_back( JSON{ { "succeeded", ok },
                                     { "closed", WritePathsD( closed ) },
                                     { "open", WritePathsD( openOut ) },
                                     { "tree", tree },
                                     { "error_code", c.ErrorCode() } } );
        }
    }
    catch( const c2::Clipper2Exception& e )
    {
        results.push_back( JSON{ { "exception", e.what() } } );
    }

    return JSON{ { "runs", results }, { "z_calls", *calls } };
}


/// `std::min` as `ClipperOffset` uses it.
double StdMin( double a, double b )
{
    return ( b < a ) ? b : a;
}


/// The arc steps per full circle `DoGroupOffset`/`DoRound` compute for `abs_delta`.
double StepsPer360( double aAbsDelta, double aArcTolerance )
{
    const double arcTol = ( aArcTolerance > c2::floating_point_tolerance )
                                  ? StdMin( aAbsDelta, aArcTolerance )
                                  : aAbsDelta * c2::arc_const;
    return StdMin( c2::PI / std::acos( 1 - arcTol / aAbsDelta ), aAbsDelta * c2::PI );
}


double OffsetValue( const JSON& aValue )
{
    const double v = F64( aValue );

    if( !std::isfinite( v ) || std::fabs( v ) > OFFSET_LIMIT )
        Invalid( "delta" );

    return v;
}


JSON RunOffset( const JSON& aIn )
{
    Keys( aIn, { "op", "groups", "miter_limit", "arc_tolerance", "preserve_collinear",
                 "reverse_solution", "z", "delta_table", "set_callback", "executes" } );
    COUNTER count;

    struct GROUP_IN
    {
        c2::Paths64   paths;
        c2::JoinType  join;
        c2::EndType   end;
        bool          single;
    };

    std::vector<GROUP_IN> groups;
    const JSON&           groupsIn = Array( At( aIn, "groups" ), "groups" );

    if( groupsIn.size() > MAX_GROUPS )
        Invalid( "size" );

    for( const JSON& g : groupsIn )
    {
        Keys( g, { "paths", "join", "end", "single" } );
        GROUP_IN group;
        group.paths = ReadPaths( At( g, "paths" ), count );
        group.join = static_cast<c2::JoinType>( Enum( At( g, "join" ), 4 ) );
        group.end = static_cast<c2::EndType>( Enum( At( g, "end" ), 5 ) );
        group.single = Bool( At( g, "single" ) );

        if( group.single && group.paths.size() != 1 )
            Invalid( "single" );

        for( const c2::Path64& path : group.paths )
        {
            for( const c2::Point64& p : path )
            {
                if( p.x < -OFFSET_COORD || p.x > OFFSET_COORD || p.y < -OFFSET_COORD
                    || p.y > OFFSET_COORD )
                    Invalid( "domain" );
            }
        }

        groups.push_back( std::move( group ) );
    }

    const double miterLimit = OffsetValue( At( aIn, "miter_limit" ) );
    const double arcTolerance = OffsetValue( At( aIn, "arc_tolerance" ) );
    const bool   preserve = Bool( At( aIn, "preserve_collinear" ) );
    const bool   reverse = Bool( At( aIn, "reverse_solution" ) );
    const bool   recorder = Recorder( At( aIn, "z" ) );

    std::vector<double> table;
    const JSON&         tableIn = At( aIn, "delta_table" );

    if( !tableIn.is_null() )
    {
        if( Array( tableIn, "delta_table" ).empty() || tableIn.size() > MAX_TABLE )
            Invalid( "delta_table" );

        for( const JSON& v : tableIn )
            table.push_back( OffsetValue( v ) );
    }

    const bool setCallback = Bool( At( aIn, "set_callback" ) );

    if( setCallback && table.empty() )
        Invalid( "callback" );

    struct EXECUTE
    {
        int    kind; // 0 paths, 1 tree, 2 Execute(DeltaCallback64, paths)
        double delta;
    };

    std::vector<EXECUTE> executes;
    const JSON&          executesIn = Array( At( aIn, "executes" ), "executes" );

    if( executesIn.empty() || executesIn.size() > MAX_EXECUTES )
        Invalid( "executes" );

    for( const JSON& e : executesIn )
    {
        Keys( e, { "kind", "delta" } );
        const std::string& kind = Text( At( e, "kind" ) );
        EXECUTE            ex;

        if( kind == "paths" )
            ex.kind = 0;
        else if( kind == "tree" )
            ex.kind = 1;
        else if( kind == "callback" )
            ex.kind = 2;
        else
            Invalid( "kind" );

        ex.delta = OffsetValue( At( e, "delta" ) );

        if( ex.kind == 2 && table.empty() )
            Invalid( "callback" );

        executes.push_back( ex );
    }

    // Resource guard: round joins and ends emit at most (steps per circle + 16) points per
    // input vertex at any delta the request can produce.
    bool round = false;

    for( const GROUP_IN& g : groups )
        round = round || g.join == c2::JoinType::Round || g.end == c2::EndType::Round;

    if( round )
    {
        std::vector<double> deltas = table;

        for( const EXECUTE& e : executes )
            deltas.push_back( e.kind == 2 ? 1.0 : e.delta );

        for( double d : deltas )
        {
            const double absDelta = std::fabs( d );

            if( absDelta <= c2::floating_point_tolerance )
                continue;

            const double steps = StepsPer360( absDelta, arcTolerance );

            if( !( steps <= MAX_ARC_STEPS )
                || double( count.n ) * ( steps + 16.0 ) > MAX_OFFSET_POINTS )
                Invalid( "arc steps" );
        }
    }

    auto zCalls = std::make_shared<JSON>( JSON::array() );
    auto deltaCalls = std::make_shared<JSON>( JSON::array() );
    JSON results = JSON::array();
    JSON groupsOut = JSON::array();
    JSON norms = JSON::array();

    c2::ClipperOffset co( miterLimit, arcTolerance, preserve, reverse );

    auto makeCallback = [&]() -> c2::DeltaCallback64
    {
        return [table, deltaCalls]( const c2::Path64& path, const c2::PathD& normals, size_t curr,
                                    size_t prev ) -> double
        {
            const double value = table[deltaCalls->size() % table.size()];
            deltaCalls->push_back( JSON::array(
                    { curr, prev, path.size(), normals.size() } ) );
            return value;
        };
    };

    try
    {
        if( recorder )
        {
            co.SetZCallback(
                    [zCalls]( const c2::Point64& e1b, const c2::Point64& e1t, const c2::Point64& e2b,
                              const c2::Point64& e2t, c2::Point64& pt )
                    {
                        zCalls->push_back( JSON::array( { WritePoint( e1b ), WritePoint( e1t ),
                                                          WritePoint( e2b ), WritePoint( e2t ),
                                                          WritePoint( pt ) } ) );
                        pt.z = Z_BASE + int64_t( zCalls->size() );
                    } );
        }

        if( setCallback )
            co.SetDeltaCallback( makeCallback() );

        for( const GROUP_IN& g : groups )
        {
            if( g.single )
                co.AddPath( g.paths[0], g.join, g.end );
            else
                co.AddPaths( g.paths, g.join, g.end );
        }

        for( const auto& group : co.*Get( OFFSET_GROUPS{} ) )
        {
            groupsOut.push_back( JSON{ { "paths_in", WritePaths( group.paths_in ) },
                                       { "lowest_path_idx",
                                         group.lowest_path_idx.has_value()
                                                 ? JSON( *group.lowest_path_idx )
                                                 : JSON( nullptr ) },
                                       { "is_reversed", group.is_reversed } } );
        }

        for( const EXECUTE& e : executes )
        {
            c2::Paths64 paths;
            JSON        tree = nullptr;

            if( e.kind == 0 )
            {
                co.Execute( e.delta, paths );
            }
            else if( e.kind == 1 )
            {
                c2::PolyTree64 polytree;
                co.Execute( e.delta, polytree );
                tree = JSON::array();
                WriteTree( polytree, -1, tree, WritePath );
            }
            else
            {
                co.Execute( makeCallback(), paths );
            }

            results.push_back( JSON{ { "paths", WritePaths( paths ) },
                                     { "tree", tree },
                                     { "error_code", co.ErrorCode() } } );
        }

        norms = WritePathD( co.*Get( OFFSET_NORMS{} ) );
    }
    catch( const c2::Clipper2Exception& e )
    {
        results.push_back( JSON{ { "exception", e.what() } } );
    }

    return JSON{ { "groups", groupsOut },
                 { "executes", results },
                 { "norms", norms },
                 { "z_calls", *zCalls },
                 { "delta_calls", *deltaCalls } };
}


JSON RunHelper( const JSON& aIn )
{
    const std::string& fn = Text( At( aIn, "fn" ) );
    COUNTER            count;

    if( fn == "hypot" )
    {
        Keys( aIn, { "op", "fn", "x", "y" } );
        return Bits( c2::Hypot( F64( At( aIn, "x" ) ), F64( At( aIn, "y" ) ) ) );
    }

    if( fn == "unit_normal" )
    {
        Keys( aIn, { "op", "fn", "a", "b" } );
        const c2::Point64 a = ReadPoint( At( aIn, "a" ), count );
        const c2::Point64 b = ReadPoint( At( aIn, "b" ), count );

        if( a.x < -ENGINE_COORD || a.x > ENGINE_COORD || a.y < -ENGINE_COORD
            || a.y > ENGINE_COORD || b.x < -ENGINE_COORD || b.x > ENGINE_COORD
            || b.y < -ENGINE_COORD || b.y > ENGINE_COORD )
            Invalid( "domain" );

        return WritePointD( c2::GetUnitNormal( a, b ) );
    }

    if( fn == "normalize" )
    {
        Keys( aIn, { "op", "fn", "v" } );
        return WritePointD( c2::NormalizeVector( ReadPointD( At( aIn, "v" ), count ) ) );
    }

    if( fn == "avg_unit" )
    {
        Keys( aIn, { "op", "fn", "a", "b" } );
        const c2::PointD a = ReadPointD( At( aIn, "a" ), count );
        const c2::PointD b = ReadPointD( At( aIn, "b" ), count );
        return WritePointD( c2::GetAvgUnitVector( a, b ) );
    }

    if( fn == "almost_zero" )
    {
        Keys( aIn, { "op", "fn", "v", "epsilon" } );
        const double v = F64( At( aIn, "v" ) );
        const JSON&  eps = At( aIn, "epsilon" );
        return eps.is_null() ? c2::AlmostZero( v ) : c2::AlmostZero( v, F64( eps ) );
    }

    if( fn == "perpendic" || fn == "perpendic_d" )
    {
        Keys( aIn, { "op", "fn", "pt", "norm", "delta" } );
        const c2::Point64 pt = ReadPoint( At( aIn, "pt" ), count );
        const c2::PointD  norm = ReadPointD( At( aIn, "norm" ), count );
        const double      delta = F64( At( aIn, "delta" ) );

        if( pt.x < -OFFSET_COORD || pt.x > OFFSET_COORD || pt.y < -OFFSET_COORD
            || pt.y > OFFSET_COORD || !std::isfinite( norm.x ) || !std::isfinite( norm.y )
            || std::fabs( norm.x ) > 2.0 || std::fabs( norm.y ) > 2.0 || !std::isfinite( delta )
            || std::fabs( delta ) > OFFSET_LIMIT )
            Invalid( "domain" );

        if( fn == "perpendic" )
            return WritePoint( c2::GetPerpendic( pt, norm, delta ) );

        return WritePointD( c2::GetPerpendicD( pt, norm, delta ) );
    }

    if( fn == "negate_path" )
    {
        Keys( aIn, { "op", "fn", "path" } );
        c2::PathD path;

        for( const JSON& p : Array( At( aIn, "path" ), "path" ) )
            path.push_back( ReadPointD( p, count ) );

        c2::NegatePath( path );
        return WritePathD( path );
    }

    if( fn == "is_closed_path" )
    {
        Keys( aIn, { "op", "fn", "end" } );
        return c2::IsClosedPath( static_cast<c2::EndType>( Enum( At( aIn, "end" ), 5 ) ) );
    }

    if( fn == "lowest_closed_path_info" )
    {
        Keys( aIn, { "op", "fn", "paths", "is_neg_area" } );
        const c2::Paths64 paths = ReadPaths( At( aIn, "paths" ), count );
        EngineDomain( paths );
        std::optional<size_t> idx = 7; // reset by the call
        bool                  isNegArea = Bool( At( aIn, "is_neg_area" ) );
        c2::GetLowestClosedPathInfo( paths, idx, isNegArea );
        return JSON{ { "idx", idx.has_value() ? JSON( *idx ) : JSON( nullptr ) },
                     { "is_neg_area", isNegArea } };
    }

    Invalid( "fn" );
}


JSON Run( const JSON& aIn )
{
    const std::string& op = Text( At( aIn, "op" ) );

    if( op == "clip64" )
        return RunClip64( aIn );

    if( op == "clipd" )
        return RunClipD( aIn );

    if( op == "offset" )
        return RunOffset( aIn );

    if( op == "helper" )
        return RunHelper( aIn );

    Invalid( "op" );
}
} // namespace


namespace oracle
{
void RegisterClipper2EngineP002( REGISTRY& aRegistry )
{
    aRegistry.Add( ENGINE_P002_UNIT, "thirdparty/clipper2/Clipper2Lib/src/clipper.offset.cpp",
                   Run );
}
} // namespace oracle
