// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001: lines 1-2271 of
// the pinned, unmodified clipper.engine.cpp. Most of these bodies are private ClipperBase
// members or file-local (inline/static) functions that no other translation unit can call, so
// this adapter compiles the pinned source file itself into this translation unit, in its own
// namespace (the identifier Clipper2Lib is renamed, nothing else changes), with the oracle's
// flags and KiCad's public USINGZ definition. Private members are reached through explicit
// template instantiation, which C++ exempts from access checking. Every step mirrors
// rust/kicad-clipper2-engine-shim request for request; refusals repeat the reference's
// pointer and integer steps without producing a reply. Doubles travel as their 16 hex digit
// bit patterns; pointers never reach the output (edges are handles, records list positions).
#include "registry.h"
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <functional>
#include <limits>
#include <map>
#include <memory>
#include <numeric>
#include <optional>
#include <queue>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>

#define Clipper2Lib Clipper2EngineP001
#include "clipper.engine.cpp"
#undef Clipper2Lib

#ifndef USINGZ
#error "KiCad compiles Clipper2 with the public USINGZ definition"
#endif

namespace
{
using oracle::JSON;
namespace c2 = Clipper2EngineP001;

constexpr const char* ENGINE_P001_UNIT = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001";
constexpr size_t      MAX_STEPS = 512;
constexpr size_t      WALK_LIMIT = size_t( 1 ) << 20;
constexpr int64_t     WIND_LIMIT = int64_t( 1 ) << 20;
constexpr int64_t     Z_LIMIT = int64_t( 1 ) << 40;

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

ROB_MEMBER( CLIPTYPE, &c2::ClipperBase::cliptype_ );
ROB_MEMBER( FILLRULE, &c2::ClipperBase::fillrule_ );
ROB_MEMBER( USING_POLYTREE, &c2::ClipperBase::using_polytree_ );
ROB_MEMBER( SORTED, &c2::ClipperBase::minima_list_sorted_ );
ROB_MEMBER( ACTIVES, &c2::ClipperBase::actives_ );
ROB_MEMBER( SEL, &c2::ClipperBase::sel_ );
ROB_MEMBER( MINIMA, &c2::ClipperBase::minima_list_ );
ROB_MEMBER( LOCMIN_ITER, &c2::ClipperBase::current_locmin_iter_ );
ROB_MEMBER( VERTEX_LISTS, &c2::ClipperBase::vertex_lists_ );
ROB_MEMBER( SCANLINES, &c2::ClipperBase::scanline_list_ );
ROB_MEMBER( RESET, &c2::ClipperBase::Reset );
ROB_MEMBER( INSERT_SCANLINE, &c2::ClipperBase::InsertScanline );
ROB_MEMBER( POP_SCANLINE, &c2::ClipperBase::PopScanline );
ROB_MEMBER( POP_LOCAL_MINIMA, &c2::ClipperBase::PopLocalMinima );
ROB_MEMBER( INSERT_LOCAL_MINIMA, &c2::ClipperBase::InsertLocalMinimaIntoAEL );
ROB_MEMBER( INSERT_LEFT_EDGE, &c2::ClipperBase::InsertLeftEdge );
ROB_MEMBER( WIND_CLOSED, &c2::ClipperBase::SetWindCountForClosedPathEdge );
ROB_MEMBER( WIND_OPEN, &c2::ClipperBase::SetWindCountForOpenPathEdge );
ROB_MEMBER( CONTRIBUTING_CLOSED, &c2::ClipperBase::IsContributingClosed );
ROB_MEMBER( CONTRIBUTING_OPEN, &c2::ClipperBase::IsContributingOpen );
ROB_MEMBER( LOCAL_MIN_POLY, &c2::ClipperBase::AddLocalMinPoly );
ROB_MEMBER( LOCAL_MAX_POLY, &c2::ClipperBase::AddLocalMaxPoly );
ROB_MEMBER( JOIN_OUTREC_PATHS, &c2::ClipperBase::JoinOutrecPaths );
ROB_MEMBER( NEW_OUTREC, &c2::ClipperBase::NewOutRec );
ROB_MEMBER( ADD_OUT_PT, &c2::ClipperBase::AddOutPt );
ROB_MEMBER( START_OPEN_PATH, &c2::ClipperBase::StartOpenPath );
ROB_MEMBER( INTERSECT_EDGES, &c2::ClipperBase::IntersectEdges );
ROB_MEMBER( PUSH_HORZ, &c2::ClipperBase::PushHorz );
ROB_MEMBER( POP_HORZ, &c2::ClipperBase::PopHorz );
ROB_MEMBER( UPDATE_EDGE, &c2::ClipperBase::UpdateEdgeIntoAEL );
ROB_MEMBER( DO_SPLIT_OP, &c2::ClipperBase::DoSplitOp );
ROB_MEMBER( FIX_SELF_INTERSECTS, &c2::ClipperBase::FixSelfIntersects );
ROB_MEMBER( REUSE_MINIMA, &c2::ReuseableDataContainer64::minima_list_ );
ROB_MEMBER( REUSE_VERTEX_LISTS, &c2::ReuseableDataContainer64::vertex_lists_ );
ROB_MEMBER( STRIPE_EDGES, &c2::StripeIndex::edges_ );
ROB_MEMBER( STRIPE_STRIPES, &c2::StripeIndex::stripes_ );
ROB_MEMBER( STRIPE_Y_MIN, &c2::StripeIndex::y_min_ );
ROB_MEMBER( STRIPE_HEIGHT, &c2::StripeIndex::stripe_height_ );
ROB_MEMBER( STRIPE_COUNT, &c2::StripeIndex::stripe_count_ );
ROB_MEMBER( Y_TO_STRIPE, &c2::StripeIndex::YToStripe );

// The protected members, through a derived class.
class PROBE : public c2::ClipperBase
{
public:
    using c2::ClipperBase::AddPath;
    using c2::ClipperBase::AddPaths;
    using c2::ClipperBase::CleanCollinear;
    using c2::ClipperBase::CleanUp;
    using c2::ClipperBase::has_open_paths_;
    using c2::ClipperBase::outrec_list_;
    using c2::ClipperBase::preserve_collinear_;
    using c2::ClipperBase::SetZ;
    using c2::ClipperBase::succeeded_;
    using c2::ClipperBase::zCallback_;

    template <typename TAG, typename... ARGS>
    decltype( auto ) Call( ARGS&&... aArgs )
    {
        c2::ClipperBase& base = *this;
        return ( base.*Get( TAG{} ) )( std::forward<ARGS>( aArgs )... );
    }

    template <typename TAG>
    auto& Field()
    {
        c2::ClipperBase& base = *this;
        return base.*Get( TAG{} );
    }
};

// --- input ---------------------------------------------------------------------------

[[noreturn]] void Invalid( const std::string& aWhat )
{
    throw std::invalid_argument( "invalid input: " + aWhat );
}

[[noreturn]] void Undefined( const std::string& aOp )
{
    throw std::invalid_argument( "undefined behaviour: " + aOp );
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

const std::string& Text( const JSON& aValue, const char* aName )
{
    if( !aValue.is_object() || !aValue.contains( aName ) )
        Invalid( aName );

    const JSON& v = aValue.at( aName );

    if( !v.is_string() )
        Invalid( aName );

    return v.get_ref<const std::string&>();
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

uint64_t U64( const JSON& aValue )
{
    if( aValue.is_number_unsigned() )
        return aValue.get<uint64_t>();

    if( !aValue.is_number_integer() || aValue.get<int64_t>() < 0 )
        Invalid( "uint64" );

    return uint64_t( aValue.get<int64_t>() );
}

int I32( const JSON& aValue )
{
    const int64_t v = I64( aValue );

    if( v < std::numeric_limits<int>::min() || v > std::numeric_limits<int>::max() )
        Invalid( "int" );

    return int( v );
}

size_t Index( const JSON& aValue )
{
    return size_t( U64( aValue ) );
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

c2::Point64 ReadPoint( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 3 )
        Invalid( "point" );

    c2::Point64 p;
    p.x = I64( aValue[0] );
    p.y = I64( aValue[1] );
    p.z = I64( aValue[2] );
    return p;
}

JSON WritePoint( const c2::Point64& aPoint )
{
    return JSON::array( { aPoint.x, aPoint.y, aPoint.z } );
}

c2::Path64 ReadPath( const JSON& aValue )
{
    if( !aValue.is_array() )
        Invalid( "path" );

    c2::Path64 path;

    for( const JSON& p : aValue )
        path.push_back( ReadPoint( p ) );

    return path;
}

JSON WritePath( const c2::Path64& aPath )
{
    JSON out = JSON::array();

    for( const c2::Point64& p : aPath )
        out.push_back( WritePoint( p ) );

    return out;
}

c2::Paths64 ReadPaths( const JSON& aValue )
{
    if( !aValue.is_array() )
        Invalid( "paths" );

    c2::Paths64 paths;

    for( const JSON& p : aValue )
        paths.push_back( ReadPath( p ) );

    return paths;
}

const char* PipName( c2::PointInPolygonResult aResult )
{
    switch( aResult )
    {
    case c2::PointInPolygonResult::IsOn: return "IsOn";
    case c2::PointInPolygonResult::IsInside: return "IsInside";
    default: return "IsOutside";
    }
}

c2::PathType ReadPathType( const JSON& aValue )
{
    if( aValue == "subject" )
        return c2::PathType::Subject;

    if( aValue == "clip" )
        return c2::PathType::Clip;

    Invalid( "path_type" );
}

const char* PathTypeName( c2::PathType aType )
{
    return aType == c2::PathType::Subject ? "subject" : "clip";
}

c2::ClipType ReadClipType( const JSON& aValue )
{
    if( aValue == "NoClip" )
        return c2::ClipType::NoClip;
    if( aValue == "Intersection" )
        return c2::ClipType::Intersection;
    if( aValue == "Union" )
        return c2::ClipType::Union;
    if( aValue == "Difference" )
        return c2::ClipType::Difference;
    if( aValue == "Xor" )
        return c2::ClipType::Xor;

    Invalid( "clip_type" );
}

c2::FillRule ReadFillRule( const JSON& aValue )
{
    if( aValue == "EvenOdd" )
        return c2::FillRule::EvenOdd;
    if( aValue == "NonZero" )
        return c2::FillRule::NonZero;
    if( aValue == "Positive" )
        return c2::FillRule::Positive;
    if( aValue == "Negative" )
        return c2::FillRule::Negative;

    Invalid( "fill_rule" );
}

const char* JoinName( c2::JoinWith aJoin )
{
    switch( aJoin )
    {
    case c2::JoinWith::Left: return "Left";
    case c2::JoinWith::Right: return "Right";
    default: return "NoJoin";
    }
}

bool InDomain( const c2::Point64& aPoint )
{
    return aPoint.x >= c2::MIN_COORD && aPoint.x <= c2::MAX_COORD && aPoint.y >= c2::MIN_COORD
           && aPoint.y <= c2::MAX_COORD;
}

// --- precise guards on the reference's integer and conversion steps -------------------

bool SubOverflows( int64_t a, int64_t b )
{
    int64_t r;
    return __builtin_sub_overflow( a, b, &r );
}

bool AddOverflows( int64_t a, int64_t b )
{
    int64_t r;
    return __builtin_add_overflow( a, b, &r );
}

// GetDx: double(pt2.y - pt1.y), then double(pt2.x - pt1.x) when dy != 0.
void GuardGetDx( const c2::Point64& aPt1, const c2::Point64& aPt2, const std::string& aOp )
{
    if( SubOverflows( aPt2.y, aPt1.y ) )
        Undefined( aOp );

    if( aPt2.y - aPt1.y != 0 && SubOverflows( aPt2.x, aPt1.x ) )
        Undefined( aOp );
}

// TopX: currentY - ae.bot.y, static_cast<int64_t>(nearbyint(dx * that)), ae.bot.x + it.
void GuardTopX( const c2::Active& aAe, int64_t aY, const std::string& aOp )
{
    if( aY == aAe.top.y || aAe.top.x == aAe.bot.x || aY == aAe.bot.y )
        return;

    if( SubOverflows( aY, aAe.bot.y ) )
        Undefined( aOp );

    const double r = std::nearbyint( aAe.dx * double( aY - aAe.bot.y ) );

    if( !( r >= -9223372036854775808.0 && r < 9223372036854775808.0 ) )
        Undefined( aOp );

    if( AddOverflows( aAe.bot.x, int64_t( r ) ) )
        Undefined( aOp );
}

void GuardAreaTriangle( const c2::Point64& a, const c2::Point64& b, const c2::Point64& c,
                        const std::string& aOp )
{
    if( AddOverflows( c.y, a.y ) || SubOverflows( c.x, a.x ) || AddOverflows( a.y, b.y )
        || SubOverflows( a.x, b.x ) || AddOverflows( b.y, c.y ) || SubOverflows( b.x, c.x ) )
    {
        Undefined( aOp );
    }
}

// PtsReallyClose: std::llabs(pt1.x - pt2.x) < 2 && std::llabs(pt1.y - pt2.y) < 2.
void GuardPtsReallyClose( const c2::Point64& a, const c2::Point64& b, const std::string& aOp )
{
    const int64_t min = std::numeric_limits<int64_t>::min();

    if( SubOverflows( a.x, b.x ) || a.x - b.x == min )
        Undefined( aOp );

    if( std::llabs( a.x - b.x ) >= 2 )
        return;

    if( SubOverflows( a.y, b.y ) || a.y - b.y == min )
        Undefined( aOp );
}

// --- stripe_index --------------------------------------------------------------------------

struct EDGE
{
    c2::Point64 a, b;
};

// StripeIndex::Build, PointInPolygon and YToStripe: every int64_t step, refused when it
// would overflow, for the queries and ys the request makes afterwards.
void GuardStripeIndex( const std::vector<EDGE>& aEdges, const c2::Path64& aQueries,
                       const std::vector<int64_t>& aYs, const std::string& aOp )
{
    if( aEdges.empty() )
        return; // y_min_ = 0, stripe_height_ = 1: no step can overflow

    int64_t yMin = aEdges[0].a.y, yMax = aEdges[0].a.y;

    for( const EDGE& e : aEdges )
    {
        yMin = std::min( yMin, std::min( e.a.y, e.b.y ) );
        yMax = std::max( yMax, std::max( e.a.y, e.b.y ) );
    }

    int count = static_cast<int>( std::sqrt( static_cast<double>( aEdges.size() ) ) );
    count = std::clamp( count, 1, 65536 );

    if( SubOverflows( yMax, yMin ) )
        Undefined( aOp );

    const int64_t range = yMax - yMin;

    if( range > 0 && AddOverflows( range, count ) )
        Undefined( aOp );

    const int64_t height = range <= 0 ? 1 : ( range + count - 1 ) / count;

    auto stripe = [&]( int64_t y ) -> int64_t
    {
        if( SubOverflows( y, yMin ) )
            Undefined( aOp );

        const int64_t s = ( y - yMin ) / height;
        return s < 0 ? 0 : ( s >= count ? count - 1 : s );
    };

    int64_t total = 0;

    for( const EDGE& e : aEdges )
        total += stripe( std::max( e.a.y, e.b.y ) ) - stripe( std::min( e.a.y, e.b.y ) ) + 1;

    const bool declined = total > int64_t( aEdges.size() ) * 8;

    if( !declined )
    {
        for( const c2::Point64& q : aQueries )
        {
            const int64_t s = stripe( q.y );

            for( const EDGE& e : aEdges )
            {
                if( s < stripe( std::min( e.a.y, e.b.y ) ) || s > stripe( std::max( e.a.y, e.b.y ) ) )
                    continue;

                if( SubOverflows( e.b.x, e.a.x ) || SubOverflows( q.y, e.b.y )
                    || SubOverflows( e.b.y, e.a.y ) || SubOverflows( q.x, e.b.x ) )
                {
                    Undefined( aOp );
                }

                if( c2::CrossProductSign( e.a, e.b, q ) == 0
                    && q.x >= std::min( e.a.x, e.b.x ) && q.x <= std::max( e.a.x, e.b.x )
                    && q.y >= std::min( e.a.y, e.b.y ) && q.y <= std::max( e.a.y, e.b.y ) )
                {
                    break; // IsOn: the query stops here
                }
            }
        }
    }

    for( int64_t y : aYs )
        stripe( y );
}

JSON RunStripeIndex( const JSON& aIn, const std::string& aOp )
{
    Keys( aIn, { "op", "source", "path", "queries", "ys" } );
    const c2::Path64 path = ReadPath( aIn.at( "path" ) );
    const c2::Path64 queries = ReadPath( aIn.at( "queries" ) );

    if( !aIn.at( "ys" ).is_array() )
        Invalid( "ys" );

    std::vector<int64_t> ys;

    for( const JSON& y : aIn.at( "ys" ) )
        ys.push_back( I64( y ) );

    const std::string&                   source = Text( aIn, "source" );
    std::vector<EDGE>                    edges;
    std::vector<std::unique_ptr<c2::OutPt>> ring;

    if( source == "path" )
    {
        if( path.size() >= 3 )
        {
            for( size_t i = 0, j = path.size() - 1; i < path.size(); j = i++ )
                if( path[j].x != path[i].x || path[j].y != path[i].y )
                    edges.push_back( { path[j], path[i] } );
        }
    }
    else if( source == "ring" )
    {
        for( const c2::Point64& p : path )
            ring.push_back( std::make_unique<c2::OutPt>( p, nullptr ) );

        for( size_t i = 0; i < ring.size(); ++i )
        {
            ring[i]->next = ring[( i + 1 ) % ring.size()].get();
            ring[i]->prev = ring[( i + ring.size() - 1 ) % ring.size()].get();
        }

        if( ring.size() >= 3 )
        {
            for( size_t i = 0; i < ring.size(); ++i )
            {
                const c2::Point64& a = ring[i]->pt;
                const c2::Point64& b = ring[( i + 1 ) % ring.size()]->pt;

                if( a.x != b.x || a.y != b.y )
                    edges.push_back( { a, b } );
            }
        }
    }
    else
    {
        Invalid( "source" );
    }

    GuardStripeIndex( edges, queries, ys, aOp );
    c2::StripeIndex index;

    if( source == "path" )
        index.BuildFromPath( path );
    else
        index.BuildFromRing( ring.empty() ? nullptr : ring[0].get() );

    JSON answers = JSON::array();

    for( const c2::Point64& q : queries )
        answers.push_back( PipName( index.PointInPolygon( q ) ) );

    JSON stripesOf = JSON::array();

    for( int64_t y : ys )
        stripesOf.push_back( ( index.*Get( Y_TO_STRIPE{} ) )( y ) );

    JSON edgeList = JSON::array();

    for( const auto& e : index.*Get( STRIPE_EDGES{} ) )
        edgeList.push_back( JSON::array( { WritePoint( e.a ), WritePoint( e.b ) } ) );

    JSON stripes = JSON::array();

    for( const auto& stripe : index.*Get( STRIPE_STRIPES{} ) )
    {
        JSON s = JSON::array();

        for( int32_t i : stripe )
            s.push_back( i );

        stripes.push_back( s );
    }

    return JSON{ { "y_min", index.*Get( STRIPE_Y_MIN{} ) },
                 { "stripe_height", index.*Get( STRIPE_HEIGHT{} ) },
                 { "stripe_count", index.*Get( STRIPE_COUNT{} ) },
                 { "empty", index.Empty() },
                 { "edge_count", index.EdgeCount() },
                 { "edges", edgeList },
                 { "stripes", stripes },
                 { "queries", answers },
                 { "ys", stripesOf } };
}

// --- pure helpers --------------------------------------------------------------------

JSON RunPure( const std::string& aOp, const JSON& aIn )
{
    if( aOp == "get_dx" )
    {
        Keys( aIn, { "op", "a", "b" } );
        const c2::Point64 a = ReadPoint( aIn.at( "a" ) ), b = ReadPoint( aIn.at( "b" ) );
        GuardGetDx( a, b, aOp );
        return Bits( c2::GetDx( a, b ) );
    }

    if( aOp == "top_x" )
    {
        Keys( aIn, { "op", "bot", "top", "dx", "y" } );
        c2::Active e;
        e.bot = ReadPoint( aIn.at( "bot" ) );
        e.top = ReadPoint( aIn.at( "top" ) );
        e.dx = F64( aIn.at( "dx" ) );
        const int64_t y = I64( aIn.at( "y" ) );
        GuardTopX( e, y, aOp );
        return c2::TopX( e, y );
    }

    if( aOp == "area_triangle" )
    {
        Keys( aIn, { "op", "a", "b", "c" } );
        const c2::Point64 a = ReadPoint( aIn.at( "a" ) ), b = ReadPoint( aIn.at( "b" ) ),
                          c = ReadPoint( aIn.at( "c" ) );
        GuardAreaTriangle( a, b, c, aOp );
        return Bits( c2::AreaTriangle( a, b, c ) );
    }

    if( aOp == "pts_really_close" )
    {
        Keys( aIn, { "op", "a", "b" } );
        const c2::Point64 a = ReadPoint( aIn.at( "a" ) ), b = ReadPoint( aIn.at( "b" ) );
        GuardPtsReallyClose( a, b, aOp );
        return c2::PtsReallyClose( a, b );
    }

    if( aOp == "is_odd" )
    {
        Keys( aIn, { "op", "value" } );
        return c2::IsOdd( I32( aIn.at( "value" ) ) );
    }

    if( aOp == "intersect_list_sort" )
    {
        Keys( aIn, { "op", "a", "b" } );
        c2::Point64 pa = ReadPoint( aIn.at( "a" ) ), pb = ReadPoint( aIn.at( "b" ) );
        const c2::IntersectNode a( nullptr, nullptr, pa ), b( nullptr, nullptr, pb );
        return c2::IntersectListSort()( a, b );
    }

    if( aOp == "loc_min_sorter" )
    {
        Keys( aIn, { "op", "a", "b" } );
        c2::Vertex va, vb;
        va.pt = ReadPoint( aIn.at( "a" ) );
        vb.pt = ReadPoint( aIn.at( "b" ) );
        const c2::LocalMinima_ptr a = std::make_unique<c2::LocalMinima>( &va, c2::PathType::Subject, false );
        const c2::LocalMinima_ptr b = std::make_unique<c2::LocalMinima>( &vb, c2::PathType::Subject, false );
        return c2::LocMinSorter()( a, b );
    }

    if( aOp == "scanline" )
    {
        Keys( aIn, { "op", "y" } );
        const c2::Scanline s( I64( aIn.at( "y" ) ) );
        return JSON{ { "y", s.y }, { "next_null", s.next == nullptr } };
    }

    Invalid( "op" );
}

// --- engine --------------------------------------------------------------------------

class ENGINE
{
public:
    explicit ENGINE( std::string aOp ) : m_op( std::move( aOp ) ) {}

    ENGINE( const ENGINE& ) = delete;
    ENGINE& operator=( const ENGINE& ) = delete;

    // ClipperBase's destructor deletes the edges in the AEL (Clear, CleanUp); the harness
    // deletes the live edges outside it.
    ~ENGINE()
    {
        try
        {
            DeleteOrphans();
        }
        catch( ... )
        {
            // A list the guards found malformed: its edges are left to the process.
        }
    }

    JSON Step( const JSON& aStep )
    {
        const std::string& name = Text( aStep, "s" );
        JSON               result = RunStep( name, aStep );
        Register();
        return result;
    }

    JSON Dump();

private:
    [[noreturn]] void Ub() const { Undefined( m_op ); }

    c2::Active* Edge( const JSON& aValue ) const
    {
        const size_t i = Index( aValue );

        if( i >= m_handles.size() || !m_alive[i] )
            Invalid( "edge" );

        return m_handles[i];
    }

    JSON HandleOf( const c2::Active* aEdge ) const
    {
        if( !aEdge )
            return nullptr;

        auto it = m_live.find( aEdge );
        return it == m_live.end() ? JSON( "unknown" ) : JSON( it->second );
    }

    std::vector<c2::Active*> WalkList( c2::Active* aHead, bool aSel ) const
    {
        std::vector<c2::Active*> list;

        for( c2::Active* e = aHead; e; e = aSel ? e->next_in_sel : e->next_in_ael )
        {
            if( list.size() >= WALK_LIMIT )
                Invalid( "malformed list" );

            list.push_back( e );
        }

        return list;
    }

    std::vector<c2::Active*> Ael() { return WalkList( m_base.Field<ACTIVES>(), false ); }
    std::vector<c2::Active*> Sel() { return WalkList( m_base.Field<SEL>(), true ); }

    void AddHandle( c2::Active* aEdge )
    {
        m_live[aEdge] = m_handles.size();
        m_handles.push_back( aEdge );
        m_alive.push_back( true );
    }

    void Register()
    {
        std::vector<c2::Active*> edges = Ael();

        for( c2::Active* e : Sel() )
            edges.push_back( e );

        for( c2::Active* e : edges )
        {
            if( !m_live.count( e ) )
                AddHandle( e );
        }
    }

    // Delete the live edges outside the AEL (DeleteEdges and the destructor delete only the
    // AEL), given the AEL before CleanUp ran.
    void DeleteOrphans( const std::set<c2::Active*>& aAel )
    {
        for( size_t i = 0; i < m_handles.size(); ++i )
        {
            if( m_alive[i] && !aAel.count( m_handles[i] ) )
                delete m_handles[i];
        }
    }

    void DeleteOrphans()
    {
        std::set<c2::Active*> ael;

        for( c2::Active* e : WalkList( m_base.Field<ACTIVES>(), false ) )
            ael.insert( e );

        DeleteOrphans( ael );
    }

    // CleanUp (directly or through Clear) deletes the AEL; every edge is then forgotten.
    template <typename F>
    void WithCleanUp( F aCleanUp )
    {
        std::set<c2::Active*> ael;

        for( c2::Active* e : Ael() )
            ael.insert( e );

        aCleanUp();
        DeleteOrphans( ael );
        std::fill( m_alive.begin(), m_alive.end(), false );
        m_live.clear();
    }

    size_t OutRecIndex( const JSON& aValue )
    {
        const size_t i = Index( aValue );

        if( i >= m_base.outrec_list_.size() )
            Invalid( "outrec" );

        return i;
    }

    c2::OutRec* OutRec( const JSON& aValue ) { return m_base.outrec_list_[OutRecIndex( aValue )]; }

    c2::OutRec* OptOutRec( const JSON& aValue )
    {
        return aValue.is_null() ? nullptr : OutRec( aValue );
    }

    JSON OutRecOf( const c2::OutRec* aRec ) const
    {
        if( !aRec )
            return nullptr;

        for( size_t i = 0; i < m_base.outrec_list_.size(); ++i )
        {
            if( m_base.outrec_list_[i] == aRec )
                return i;
        }

        return "unknown";
    }

    std::vector<c2::OutPt*> Ring( c2::OutPt* aStart ) const
    {
        std::vector<c2::OutPt*> ring{ aStart };

        for( c2::OutPt* op = aStart->next; op != aStart; op = op->next )
        {
            if( ring.size() >= WALK_LIMIT )
                Invalid( "malformed ring" );

            ring.push_back( op );
        }

        return ring;
    }

    c2::OutPt* Pts( c2::OutRec* aRec ) const
    {
        if( !aRec->pts )
            Invalid( "ring" );

        return aRec->pts;
    }

    c2::OutPt* OutPt( const JSON& aValue )
    {
        if( !aValue.is_array() || aValue.size() != 2 )
            Invalid( "outpt" );

        c2::OutRec*                   rec = OutRec( aValue[0] );
        const std::vector<c2::OutPt*> ring = Ring( Pts( rec ) );
        const size_t                  k = Index( aValue[1] );

        if( k >= ring.size() )
            Invalid( "outpt" );

        return ring[k];
    }

    JSON OutPtJson( const c2::OutPt* aOp ) const
    {
        if( !aOp )
            return nullptr;

        return JSON{ { "pt", WritePoint( aOp->pt ) }, { "outrec", OutRecOf( aOp->outrec ) } };
    }

    bool InList( c2::Active* aEdge, bool aSel )
    {
        const std::vector<c2::Active*> list = aSel ? Sel() : Ael();
        return std::find( list.begin(), list.end(), aEdge ) != list.end();
    }

    void Consistent() const
    {
        for( size_t i = 0; i < m_handles.size(); ++i )
        {
            if( !m_alive[i] || !m_handles[i]->outrec )
                continue;

            const c2::OutRec* rec = m_handles[i]->outrec;

            if( !rec->pts || ( !rec->is_open && ( !rec->front_edge || !rec->back_edge ) ) )
                Invalid( "inconsistent output record" );
        }
    }

    void Domain( std::initializer_list<c2::Point64> aPoints )
    {
        const std::string outside = "outside the guarded engine domain: engine";

        for( const c2::Point64& p : aPoints )
        {
            if( !InDomain( p ) )
                Invalid( outside );
        }

        auto arrays = [&]( const std::vector<c2::Vertex*>& aLists, const std::vector<size_t>& aSizes )
        {
            for( size_t k = 0; k < aLists.size(); ++k )
            {
                for( size_t i = 0; i < aSizes[k]; ++i )
                {
                    if( !InDomain( aLists[k][i].pt ) )
                        Invalid( outside );
                }
            }
        };

        arrays( m_base.Field<VERTEX_LISTS>(), m_baseSizes );
        arrays( m_reuse.*Get( REUSE_VERTEX_LISTS{} ), m_reuseSizes );

        for( c2::OutRec* rec : m_base.outrec_list_ )
        {
            if( !rec->pts )
                continue;

            for( c2::OutPt* op : Ring( rec->pts ) )
            {
                if( !InDomain( op->pt ) )
                    Invalid( outside );
            }
        }
    }

    // A vertex pointer as [tag, list, index] in the base's own arrays, then the container's.
    JSON VRef( const c2::Vertex* aVertex )
    {
        if( !aVertex )
            return nullptr;

        auto find = [&]( const char* aTag, const std::vector<c2::Vertex*>& aLists,
                         const std::vector<size_t>& aSizes ) -> JSON
        {
            const std::less<const c2::Vertex*> less;

            for( size_t k = 0; k < aLists.size(); ++k )
            {
                if( !less( aVertex, aLists[k] ) && less( aVertex, aLists[k] + aSizes[k] ) )
                    return JSON::array( { aTag, k, size_t( aVertex - aLists[k] ) } );
            }

            return nullptr;
        };

        JSON ref = find( "b", m_base.Field<VERTEX_LISTS>(), m_baseSizes );

        if( ref.is_null() )
            ref = find( "r", m_reuse.*Get( REUSE_VERTEX_LISTS{} ), m_reuseSizes );

        return ref.is_null() ? JSON( "dangling" ) : ref;
    }

    JSON MinimaPosition( const c2::LocalMinima* aMinima )
    {
        if( !aMinima )
            return nullptr;

        const c2::LocalMinimaList& list = m_base.Field<MINIMA>();

        for( size_t i = 0; i < list.size(); ++i )
        {
            if( list[i].get() == aMinima )
                return i;
        }

        return "unknown";
    }

    c2::LocalMinima* MinimaIndex( const JSON& aValue )
    {
        const size_t i = Index( aValue );
        c2::LocalMinimaList& list = m_base.Field<MINIMA>();

        if( i >= list.size() )
            Invalid( "minima" );

        return list[i].get();
    }

    static size_t Total( const c2::Paths64& aPaths )
    {
        size_t total = 0;

        for( const c2::Path64& p : aPaths )
            total += p.size();

        return total;
    }

    // Whether walking from the edge's top vertex in its direction while the y stays the same
    // (and, for the open variant, no OpenEnd/LocalMax flag stops it) ever ends: the reference
    // loops forever otherwise.
    void GuardCurrYMaxima( const c2::Active* aEdge, bool aOpen )
    {
        const c2::Vertex* result = aEdge->vertex_top;
        const c2::Vertex* start = result;
        const c2::VertexFlags stop = c2::VertexFlags::OpenEnd | c2::VertexFlags::LocalMax;

        for( ;; )
        {
            const c2::Vertex* next = aEdge->wind_dx > 0 ? result->next : result->prev;

            if( next->pt.y != result->pt.y )
                return;

            if( aOpen && ( result->flags & stop ) != c2::VertexFlags::Empty )
                return;

            result = next;

            if( result == start )
                Ub();
        }
    }

    // TrimHorz's walk (and UpdateEdgeIntoAEL's call of it on closed horizontals) ends.
    void GuardTrimHorz( const c2::Active* aEdge, bool aPreserve )
    {
        c2::Active  e = *aEdge;
        c2::Point64 pt = c2::NextVertex( e )->pt;
        size_t      steps = 0;
        size_t      limit = 2;

        for( size_t s : m_baseSizes )
            limit += s;

        for( size_t s : m_reuseSizes )
            limit += s;

        while( pt.y == e.top.y )
        {
            if( aPreserve && ( ( pt.x < e.top.x ) != ( e.bot.x < e.top.x ) ) )
                return;

            e.vertex_top = c2::NextVertex( e );
            e.top = pt;

            if( c2::IsMaxima( e ) )
                return;

            pt = c2::NextVertex( e )->pt;

            if( ++steps > limit )
                Ub();
        }
    }

    JSON RunStep( const std::string& aName, const JSON& s );

    std::string                              m_op;
    PROBE                                    m_base;
    c2::ReuseableDataContainer64             m_reuse;
    std::vector<c2::Active*>                 m_handles;
    std::vector<bool>                        m_alive;
    std::map<const c2::Active*, size_t>      m_live;
    std::vector<size_t>                      m_baseSizes;
    std::vector<size_t>                      m_reuseSizes;
    bool                                     m_baseBorrows = false;
    bool                                     m_iterValid = false;
    std::shared_ptr<std::vector<JSON>>       m_calls = std::make_shared<std::vector<JSON>>();
};


JSON ENGINE::RunStep( const std::string& aName, const JSON& s )
{
    PROBE& base = m_base;

    if( aName == "add" )
    {
        Keys( s, { "s", "target", "path_type", "is_open", "paths" } );
        const c2::PathType polytype = ReadPathType( s.at( "path_type" ) );
        const bool         isOpen = Bool( s.at( "is_open" ) );
        const c2::Paths64  paths = ReadPaths( s.at( "paths" ) );
        const std::string& target = Text( s, "target" );

        if( target == "base" )
        {
            base.AddPaths( paths, polytype, isOpen );

            if( Total( paths ) )
                m_baseSizes.push_back( Total( paths ) );

            m_iterValid = false;
        }
        else if( target == "reuse" )
        {
            m_reuse.AddPaths( paths, polytype, isOpen );

            if( Total( paths ) )
                m_reuseSizes.push_back( Total( paths ) );
        }
        else
        {
            Invalid( "target" );
        }

        return nullptr;
    }

    if( aName == "add_path" )
    {
        Keys( s, { "s", "path_type", "is_open", "path" } );
        const c2::PathType polytype = ReadPathType( s.at( "path_type" ) );
        const bool         isOpen = Bool( s.at( "is_open" ) );
        const c2::Path64   path = ReadPath( s.at( "path" ) );
        base.AddPath( path, polytype, isOpen );

        if( !path.empty() )
            m_baseSizes.push_back( path.size() );

        m_iterValid = false;
        return nullptr;
    }

    if( aName == "add_reuseable_data" )
    {
        Keys( s, { "s" } );
        base.AddReuseableData( m_reuse );
        m_baseBorrows = true;
        m_iterValid = false;
        return nullptr;
    }

    if( aName == "clear" )
    {
        Keys( s, { "s", "target" } );
        const std::string& target = Text( s, "target" );

        if( target == "base" )
        {
            WithCleanUp( [&]() { base.Clear(); } );
            m_baseSizes.clear();
            m_baseBorrows = false;
            m_iterValid = true;
        }
        else if( target == "reuse" )
        {
            if( m_baseBorrows )
                Invalid( "reuse cleared while borrowed" );

            m_reuse.Clear();
            m_reuseSizes.clear();
        }
        else
        {
            Invalid( "target" );
        }

        return nullptr;
    }

    if( aName == "clean_up" )
    {
        Keys( s, { "s" } );
        WithCleanUp( [&]() { base.CleanUp(); } );
        return nullptr;
    }

    if( aName == "reset" )
    {
        Keys( s, { "s" } );
        base.Call<RESET>();
        m_iterValid = true;
        return nullptr;
    }

    if( aName == "settings" )
    {
        Keys( s, { "s", "clip_type", "fill_rule", "using_polytree", "preserve_collinear", "z",
                   "default_z" } );
        const c2::ClipType ct = ReadClipType( s.at( "clip_type" ) );
        const c2::FillRule fr = ReadFillRule( s.at( "fill_rule" ) );
        const bool         polytree = Bool( s.at( "using_polytree" ) );
        const bool         preserve = Bool( s.at( "preserve_collinear" ) );
        const int64_t      defaultZ = I64( s.at( "default_z" ) );
        c2::ZCallback64    callback = nullptr;

        if( !s.at( "z" ).is_null() )
        {
            const int64_t zBase = I64( s.at( "z" ) );

            if( zBase < -Z_LIMIT || zBase > Z_LIMIT )
                Invalid( "z" );

            std::shared_ptr<std::vector<JSON>> calls = m_calls;
            callback = [calls, zBase]( const c2::Point64& e1b, const c2::Point64& e1t,
                                       const c2::Point64& e2b, const c2::Point64& e2t, c2::Point64& pt )
            {
                calls->push_back( JSON::array( { WritePoint( e1b ), WritePoint( e1t ), WritePoint( e2b ),
                                                 WritePoint( e2t ), WritePoint( pt ) } ) );
                pt.z = zBase + int64_t( calls->size() );
            };
        }

        base.Field<CLIPTYPE>() = ct;
        base.Field<FILLRULE>() = fr;
        base.Field<USING_POLYTREE>() = polytree;
        base.preserve_collinear_ = preserve;
        base.zCallback_ = callback;
        base.DefaultZ = defaultZ;
        return nullptr;
    }

    if( aName == "insert_scanline" )
    {
        Keys( s, { "s", "y" } );
        base.Call<INSERT_SCANLINE>( I64( s.at( "y" ) ) );
        return nullptr;
    }

    if( aName == "pop_scanline" )
    {
        Keys( s, { "s" } );
        int64_t y = 0;
        const bool popped = base.Call<POP_SCANLINE>( y );
        return JSON{ { "y", popped ? JSON( y ) : JSON( nullptr ) } };
    }

    if( aName == "pop_local_minima" )
    {
        Keys( s, { "s", "y" } );

        if( !m_iterValid )
            Invalid( "iterator" );

        c2::LocalMinima* lm = nullptr;

        if( !base.Call<POP_LOCAL_MINIMA>( I64( s.at( "y" ) ), lm ) )
            return nullptr;

        return MinimaPosition( lm );
    }

    if( aName == "insert_local_minima" )
    {
        Keys( s, { "s", "y" } );

        if( !m_iterValid )
            Invalid( "iterator" );

        Domain( {} );
        Consistent();
        base.Call<INSERT_LOCAL_MINIMA>( I64( s.at( "y" ) ) );
        return nullptr;
    }

    if( aName == "new_active" )
    {
        Keys( s, { "s", "minima", "bound" } );
        c2::LocalMinima*   lm = MinimaIndex( s.at( "minima" ) );
        const std::string& bound = Text( s, "bound" );

        if( bound != "left" && bound != "right" )
            Invalid( "bound" );

        const bool        descending = bound == "left";
        const c2::Vertex* top = descending ? lm->vertex->prev : lm->vertex->next;
        GuardGetDx( lm->vertex->pt, top->pt, m_op );
        // The construction InsertLocalMinimaIntoAEL makes for each bound.
        c2::Active* e = new c2::Active();
        e->bot = lm->vertex->pt;
        e->curr_x = e->bot.x;
        e->wind_dx = descending ? -1 : 1;
        e->vertex_top = descending ? lm->vertex->prev : lm->vertex->next;
        e->top = e->vertex_top->pt;
        e->local_min = lm;
        c2::SetDx( *e );
        AddHandle( e );
        return m_handles.size() - 1;
    }

    if( aName == "set_active" )
    {
        Keys( s, { "s", "e", "wind_cnt", "wind_cnt2", "curr_x", "is_left_bound", "dx" } );
        c2::Active* e = Edge( s.at( "e" ) );

        auto wind = [&]( const JSON& v ) -> std::optional<int>
        {
            if( v.is_null() )
                return std::nullopt;

            const int64_t w = I64( v );

            if( w < -WIND_LIMIT || w > WIND_LIMIT )
                Invalid( "wind" );

            return int( w );
        };

        const std::optional<int>     wc = wind( s.at( "wind_cnt" ) );
        const std::optional<int>     wc2 = wind( s.at( "wind_cnt2" ) );
        const std::optional<int64_t> currX =
                s.at( "curr_x" ).is_null() ? std::nullopt : std::optional<int64_t>( I64( s.at( "curr_x" ) ) );
        const std::optional<bool> left = s.at( "is_left_bound" ).is_null()
                                                 ? std::nullopt
                                                 : std::optional<bool>( Bool( s.at( "is_left_bound" ) ) );
        const std::optional<double> dx =
                s.at( "dx" ).is_null() ? std::nullopt : std::optional<double>( F64( s.at( "dx" ) ) );

        if( wc )
            e->wind_cnt = *wc;
        if( wc2 )
            e->wind_cnt2 = *wc2;
        if( currX )
            e->curr_x = *currX;
        if( left )
            e->is_left_bound = *left;
        if( dx )
            e->dx = *dx;

        return nullptr;
    }

    if( aName == "insert_left_edge" )
    {
        Keys( s, { "s", "e" } );
        c2::Active* e = Edge( s.at( "e" ) );

        if( InList( e, false ) )
            Invalid( "list" );

        Domain( {} );
        base.Call<INSERT_LEFT_EDGE>( *e );
        return nullptr;
    }

    if( aName == "insert_right_edge" )
    {
        Keys( s, { "s", "e", "e2" } );
        c2::Active* e = Edge( s.at( "e" ) );
        c2::Active* e2 = Edge( s.at( "e2" ) );

        if( e == e2 || InList( e2, false ) )
            Invalid( "list" );

        c2::InsertRightEdge( *e, *e2 );
        return nullptr;
    }

    if( aName == "push_horz" )
    {
        Keys( s, { "s", "e" } );
        c2::Active* e = Edge( s.at( "e" ) );

        if( InList( e, true ) )
            Invalid( "list" );

        base.Call<PUSH_HORZ>( *e );
        return nullptr;
    }

    if( aName == "pop_horz" )
    {
        Keys( s, { "s" } );
        c2::Active* e = nullptr;
        base.Call<POP_HORZ>( e );
        return HandleOf( e );
    }

    if( aName == "extract_from_sel" )
    {
        Keys( s, { "s", "e" } );
        c2::Active*                    e = Edge( s.at( "e" ) );
        const std::vector<c2::Active*> sel = Sel();
        auto                           it = std::find( sel.begin(), sel.end(), e );

        if( it == sel.end() )
            Invalid( "list" );

        if( !e->prev_in_sel )
            Ub();

        if( it == sel.begin() || e->prev_in_sel != *( it - 1 ) )
            Invalid( "list" );

        return HandleOf( c2::ExtractFromSEL( e ) );
    }

    if( aName == "insert_before_sel" )
    {
        Keys( s, { "s", "e1", "e2" } );
        c2::Active*                    e1 = Edge( s.at( "e1" ) );
        c2::Active*                    e2 = Edge( s.at( "e2" ) );
        const std::vector<c2::Active*> sel = Sel();
        auto                           it = std::find( sel.begin(), sel.end(), e2 );

        if( it == sel.end() )
            Invalid( "list" );

        c2::Active* expected = it == sel.begin() ? nullptr : *( it - 1 );

        if( std::find( sel.begin(), sel.end(), e1 ) != sel.end() || e2->prev_in_sel != expected )
            Invalid( "list" );

        c2::Insert1Before2InSEL( e1, e2 );
        return nullptr;
    }

    if( aName == "wind_closed" || aName == "wind_open" )
    {
        Keys( s, { "s", "e" } );
        c2::Active* e = Edge( s.at( "e" ) );

        if( !InList( e, false ) )
            Invalid( "list" );

        if( aName == "wind_closed" )
            base.Call<WIND_CLOSED>( *e );
        else
            base.Call<WIND_OPEN>( *e );

        return nullptr;
    }

    if( aName == "contributing" )
    {
        Keys( s, { "s", "e" } );
        c2::Active* e = Edge( s.at( "e" ) );
        const bool  closed = base.Call<CONTRIBUTING_CLOSED>( *e );
        const bool  open = base.Call<CONTRIBUTING_OPEN>( *e );
        return JSON{ { "closed", closed }, { "open", open } };
    }

    if( aName == "valid_ael_order" )
    {
        Keys( s, { "s", "a", "b" } );
        c2::Active* a = Edge( s.at( "a" ) );
        c2::Active* b = Edge( s.at( "b" ) );
        Domain( {} );
        return c2::IsValidAelOrder( *a, *b );
    }

    if( aName == "local_min_poly" )
    {
        Keys( s, { "s", "e1", "e2", "pt", "is_new" } );
        c2::Active*       e1 = Edge( s.at( "e1" ) );
        c2::Active*       e2 = Edge( s.at( "e2" ) );
        const c2::Point64 pt = ReadPoint( s.at( "pt" ) );

        // The default argument is is_new = false.
        if( s.at( "is_new" ).is_null() )
            return OutPtJson( base.Call<LOCAL_MIN_POLY>( *e1, *e2, pt, false ) );

        return OutPtJson( base.Call<LOCAL_MIN_POLY>( *e1, *e2, pt, Bool( s.at( "is_new" ) ) ) );
    }

    if( aName == "local_max_poly" || aName == "intersect_edges" )
    {
        Keys( s, { "s", "e1", "e2", "pt" } );
        c2::Active*       e1 = Edge( s.at( "e1" ) );
        c2::Active*       e2 = Edge( s.at( "e2" ) );
        const c2::Point64 pt = ReadPoint( s.at( "pt" ) );

        if( e1 == e2 )
            Invalid( "edge" );

        Consistent();

        if( aName == "intersect_edges" )
        {
            base.Call<INTERSECT_EDGES>( *e1, *e2, pt );
            return nullptr;
        }

        if( !e1->outrec || !e2->outrec )
            Ub();

        if( e1->outrec == e2->outrec && ( !e1->outrec->front_edge || !e1->outrec->back_edge ) )
            Ub();

        return OutPtJson( base.Call<LOCAL_MAX_POLY>( *e1, *e2, pt ) );
    }

    if( aName == "join_outrec_paths" )
    {
        Keys( s, { "s", "e1", "e2" } );
        c2::Active* e1 = Edge( s.at( "e1" ) );
        c2::Active* e2 = Edge( s.at( "e2" ) );
        Consistent();

        if( !e1->outrec || !e2->outrec )
            Ub();

        if( e1->outrec == e2->outrec )
            Invalid( "owner" );

        base.Call<JOIN_OUTREC_PATHS>( *e1, *e2 );
        return nullptr;
    }

    if( aName == "add_out_pt" )
    {
        Keys( s, { "s", "e", "pt" } );
        c2::Active*       e = Edge( s.at( "e" ) );
        const c2::Point64 pt = ReadPoint( s.at( "pt" ) );
        Consistent();

        if( !e->outrec )
            Ub();

        return OutPtJson( base.Call<ADD_OUT_PT>( *e, pt ) );
    }

    if( aName == "start_open_path" )
    {
        Keys( s, { "s", "e", "pt" } );
        c2::Active*       e = Edge( s.at( "e" ) );
        const c2::Point64 pt = ReadPoint( s.at( "pt" ) );
        return OutPtJson( base.Call<START_OPEN_PATH>( *e, pt ) );
    }

    if( aName == "swap_outrecs" )
    {
        Keys( s, { "s", "e1", "e2" } );
        c2::Active* e1 = Edge( s.at( "e1" ) );
        c2::Active* e2 = Edge( s.at( "e2" ) );

        if( !e1->outrec && !e2->outrec )
            Ub();

        c2::SwapOutrecs( *e1, *e2 );
        return nullptr;
    }

    if( aName == "uncouple" )
    {
        Keys( s, { "s", "e" } );
        c2::Active* e = Edge( s.at( "e" ) );

        if( e->outrec && ( !e->outrec->front_edge || !e->outrec->back_edge ) )
            Ub();

        c2::UncoupleOutRec( *e );
        return nullptr;
    }

    if( aName == "prev_hot_edge" || aName == "matching_loc_min" || aName == "maxima_pair" )
    {
        Keys( s, { "s", "e" } );
        c2::Active* e = Edge( s.at( "e" ) );

        if( aName == "prev_hot_edge" )
            return HandleOf( c2::GetPrevHotEdge( *e ) );

        if( aName == "matching_loc_min" )
            return HandleOf( c2::FindEdgeWithMatchingLocMin( e ) );

        return HandleOf( c2::GetMaximaPair( *e ) );
    }

    if( aName == "curr_y_maxima" )
    {
        Keys( s, { "s", "e", "open" } );
        c2::Active* e = Edge( s.at( "e" ) );
        const bool  open = Bool( s.at( "open" ) );
        GuardCurrYMaxima( e, open );
        return VRef( open ? c2::GetCurrYMaximaVertex_Open( *e ) : c2::GetCurrYMaximaVertex( *e ) );
    }

    if( aName == "next_vertex" || aName == "prev_prev_vertex" )
    {
        Keys( s, { "s", "e" } );
        c2::Active* e = Edge( s.at( "e" ) );
        return VRef( aName == "next_vertex" ? c2::NextVertex( *e ) : c2::PrevPrevVertex( *e ) );
    }

    if( aName == "trim_horz" )
    {
        Keys( s, { "s", "e", "preserve_collinear" } );
        c2::Active* e = Edge( s.at( "e" ) );
        const bool  preserve = Bool( s.at( "preserve_collinear" ) );
        Domain( {} );
        GuardTrimHorz( e, preserve );
        c2::TrimHorz( *e, preserve );
        return nullptr;
    }

    if( aName == "update_edge" )
    {
        Keys( s, { "s", "e" } );
        c2::Active* e = Edge( s.at( "e" ) );
        Domain( {} );
        Consistent();

        // UpdateEdgeIntoAEL trims closed horizontals: guard the walk on the updated edge.
        c2::Active next = *e;
        next.bot = next.top;
        next.vertex_top = c2::NextVertex( *e );
        next.top = next.vertex_top->pt;
        GuardGetDx( next.bot, next.top, m_op );

        if( c2::IsHorizontal( next ) && !c2::IsOpen( next ) )
        {
            c2::SetDx( next );
            GuardTrimHorz( &next, base.preserve_collinear_ );
        }

        base.Call<UPDATE_EDGE>( e );
        return nullptr;
    }

    if( aName == "top_x" )
    {
        Keys( s, { "s", "e", "y" } );
        c2::Active*   e = Edge( s.at( "e" ) );
        const int64_t y = I64( s.at( "y" ) );
        GuardTopX( *e, y, m_op );
        return c2::TopX( *e, y );
    }

    if( aName == "set_z" )
    {
        Keys( s, { "s", "e1", "e2", "pt" } );
        c2::Active* e1 = Edge( s.at( "e1" ) );
        c2::Active* e2 = Edge( s.at( "e2" ) );
        c2::Point64 pt = ReadPoint( s.at( "pt" ) );
        base.SetZ( *e1, *e2, pt );
        return WritePoint( pt );
    }

    if( aName == "edges_adjacent" )
    {
        Keys( s, { "s", "e1", "e2" } );
        c2::Active*             e1 = Edge( s.at( "e1" ) );
        c2::Active*             e2 = Edge( s.at( "e2" ) );
        c2::Point64             origin;
        const c2::IntersectNode node( e1, e2, origin );
        return c2::EdgesAdjacentInAEL( node );
    }

    if( aName == "new_outrec" )
    {
        Keys( s, { "s", "path", "owner", "is_open" } );
        const c2::Path64 path = ReadPath( s.at( "path" ) );
        c2::OutRec*      owner = OptOutRec( s.at( "owner" ) );
        const bool       isOpen = Bool( s.at( "is_open" ) );
        c2::OutRec*      rec = base.Call<NEW_OUTREC>();
        rec->owner = owner;
        rec->is_open = isOpen;
        c2::OutPt* first = nullptr;
        c2::OutPt* last = nullptr;

        for( const c2::Point64& pt : path )
        {
            c2::OutPt* op = new c2::OutPt( pt, rec );

            if( first )
            {
                op->prev = last;
                op->next = first;
                last->next = op;
                first->prev = op;
            }
            else
            {
                first = op;
            }

            last = op;
        }

        rec->pts = first;
        return base.outrec_list_.size() - 1;
    }

    if( aName == "set_path" )
    {
        Keys( s, { "s", "o", "path" } );
        c2::OutRec* rec = OutRec( s.at( "o" ) );
        rec->path = ReadPath( s.at( "path" ) );
        return nullptr;
    }

    if( aName == "set_owner" )
    {
        Keys( s, { "s", "o", "new_owner" } );
        c2::OutRec* o = OutRec( s.at( "o" ) );
        c2::OutRec* n = OutRec( s.at( "new_owner" ) );

        if( o == n )
            Invalid( "owner" );

        c2::SetOwner( o, n );
        return nullptr;
    }

    if( aName == "real_outrec" )
    {
        Keys( s, { "s", "o" } );
        return OutRecOf( c2::GetRealOutRec( OptOutRec( s.at( "o" ) ) ) );
    }

    if( aName == "valid_owner" )
    {
        Keys( s, { "s", "o", "test" } );
        c2::OutRec* o = OptOutRec( s.at( "o" ) );
        c2::OutRec* t = OptOutRec( s.at( "test" ) );
        return c2::IsValidOwner( o, t );
    }

    if( aName == "clean_collinear" )
    {
        Keys( s, { "s", "o" } );
        c2::OutRec* o = OptOutRec( s.at( "o" ) );
        Domain( {} );
        base.CleanCollinear( o );
        return nullptr;
    }

    if( aName == "fix_self_intersects" || aName == "do_split_op" )
    {
        if( aName == "do_split_op" )
            Keys( s, { "s", "o", "at" } );
        else
            Keys( s, { "s", "o" } );

        c2::OutRec*                   o = OutRec( s.at( "o" ) );
        const std::vector<c2::OutPt*> ring = Ring( Pts( o ) );

        if( ring.size() < 3 )
            Invalid( "ring" );

        Domain( {} );

        if( aName == "fix_self_intersects" )
        {
            base.Call<FIX_SELF_INTERSECTS>( o );
        }
        else
        {
            const size_t k = Index( s.at( "at" ) );

            if( k >= ring.size() )
                Invalid( "outpt" );

            base.Call<DO_SPLIT_OP>( o, ring[k] );
        }

        return nullptr;
    }

    if( aName == "area" || aName == "point_count" || aName == "very_small_triangle"
        || aName == "swap_sides" || aName == "update_outrec_owner" || aName == "dispose_out_pts"
        || aName == "clean_path" )
    {
        Keys( s, { "s", "o" } );
        c2::OutRec* o = OutRec( s.at( "o" ) );
        c2::OutPt*  pts = Pts( o );

        if( aName == "area" )
        {
            Domain( {} );
            return Bits( c2::Area( pts ) );
        }

        if( aName == "point_count" )
            return c2::PointCount( pts );

        if( aName == "very_small_triangle" )
        {
            Domain( {} );
            return c2::IsVerySmallTriangle( *pts );
        }

        if( aName == "swap_sides" )
        {
            c2::SwapSides( *o );
            return nullptr;
        }

        if( aName == "update_outrec_owner" )
        {
            c2::UpdateOutrecOwner( o );
            return nullptr;
        }

        if( aName == "dispose_out_pts" )
        {
            c2::DisposeOutPts( o );
            return nullptr;
        }

        return WritePath( c2::GetCleanPath( pts ) );
    }

    if( aName == "valid_closed_path" || aName == "reverse_out_pts" || aName == "invalid_path" )
    {
        Keys( s, { "s", "o" } );
        c2::OutRec* o = OutRec( s.at( "o" ) );
        c2::OutPt*  pts = o->pts;

        if( aName == "valid_closed_path" )
        {
            Domain( {} );
            return c2::IsValidClosedPath( pts );
        }

        if( aName == "invalid_path" )
            return c2::IsInvalidPath( pts );

        c2::ReverseOutPts( pts );
        return nullptr;
    }

    if( aName == "duplicate_op" )
    {
        Keys( s, { "s", "op", "after" } );
        c2::OutPt* op = OutPt( s.at( "op" ) );
        const bool after = Bool( s.at( "after" ) );
        return OutPtJson( c2::DuplicateOp( op, after ) );
    }

    if( aName == "dispose_out_pt" )
    {
        Keys( s, { "s", "op" } );
        c2::OutPt* op = OutPt( s.at( "op" ) );

        for( c2::OutRec* rec : base.outrec_list_ )
        {
            if( rec->pts == op )
                Invalid( "outpt" );
        }

        return OutPtJson( c2::DisposeOutPt( op ) );
    }

    if( aName == "point_in_op_polygon" )
    {
        Keys( s, { "s", "pt", "o" } );
        const c2::Point64 pt = ReadPoint( s.at( "pt" ) );
        c2::OutRec*       o = OutRec( s.at( "o" ) );
        c2::OutPt*        pts = Pts( o );
        Domain( { pt } );
        return PipName( c2::PointInOpPolygon( pt, pts ) );
    }

    if( aName == "contains" || aName == "contains_indexed" )
    {
        const bool indexed = aName == "contains_indexed";

        if( indexed )
            Keys( s, { "s", "o1", "o2", "threshold" } );
        else
            Keys( s, { "s", "o1", "o2" } );

        c2::OutRec* o1 = OutRec( s.at( "o1" ) );
        c2::OutRec* o2 = OutRec( s.at( "o2" ) );
        c2::OutPt*  p1 = Pts( o1 );
        c2::OutPt*  p2 = Pts( o2 );
        Domain( {} );

        if( indexed )
        {
            const uint64_t threshold = U64( s.at( "threshold" ) );
            return c2::Path2ContainsPath1Indexed( p1, o2, size_t( threshold ) );
        }

        return c2::Path2ContainsPath1( p1, p2 );
    }

    if( aName == "horz_seg_sorter" )
    {
        Keys( s, { "s", "a", "b" } );

        auto seg = [&]( const JSON& v ) -> c2::HorzSegment
        {
            Keys( v, { "left", "right" } );
            auto op = [&]( const JSON& x ) -> c2::OutPt* { return x.is_null() ? nullptr : OutPt( x ); };
            c2::HorzSegment result;
            result.left_op = op( v.at( "left" ) );
            result.right_op = op( v.at( "right" ) );
            return result;
        };

        const c2::HorzSegment a = seg( s.at( "a" ) );
        const c2::HorzSegment b = seg( s.at( "b" ) );

        if( a.right_op && b.right_op && ( !a.left_op || !b.left_op ) )
            Ub();

        return c2::HorzSegSorter()( a, b );
    }

    Invalid( "step" );
}


JSON ENGINE::Dump()
{
    PROBE& base = m_base;

    auto arrays = [&]( const char* aTag, const std::vector<c2::Vertex*>& aLists,
                       const std::vector<size_t>& aSizes ) -> JSON
    {
        JSON out = JSON::array();

        for( size_t k = 0; k < aLists.size(); ++k )
        {
            JSON array = JSON::array();
            auto link = [&]( const c2::Vertex* aLink ) -> JSON
            {
                if( !aLink )
                    return nullptr;

                return JSON::array( { aTag, k, size_t( aLink - aLists[k] ) } );
            };

            for( size_t i = 0; i < aSizes[k]; ++i )
            {
                const c2::Vertex& v = aLists[k][i];
                array.push_back( JSON{ { "pt", WritePoint( v.pt ) },
                                       { "next", link( v.next ) },
                                       { "prev", link( v.prev ) },
                                       { "flags", static_cast<uint32_t>( v.flags ) } } );
            }

            out.push_back( array );
        }

        return out;
    };

    const std::vector<c2::Vertex*>& reuseLists = m_reuse.*Get( REUSE_VERTEX_LISTS{} );
    JSON vertices{ { "base", arrays( "b", base.Field<VERTEX_LISTS>(), m_baseSizes ) },
                   { "reuse", arrays( "r", reuseLists, m_reuseSizes ) } };

    JSON minima = JSON::array();

    for( const c2::LocalMinima_ptr& lm : base.Field<MINIMA>() )
    {
        minima.push_back( JSON{ { "vertex", VRef( lm->vertex ) },
                                { "polytype", PathTypeName( lm->polytype ) },
                                { "is_open", lm->is_open } } );
    }

    JSON reuseMinima = JSON::array();

    for( const c2::LocalMinima_ptr& lm : m_reuse.*Get( REUSE_MINIMA{} ) )
    {
        JSON ref = nullptr;

        for( size_t k = 0; k < reuseLists.size(); ++k )
        {
            const std::less<const c2::Vertex*> less;

            if( !less( lm->vertex, reuseLists[k] ) && less( lm->vertex, reuseLists[k] + m_reuseSizes[k] ) )
                ref = JSON::array( { "r", k, size_t( lm->vertex - reuseLists[k] ) } );
        }

        reuseMinima.push_back( JSON{ { "vertex", ref },
                                     { "polytype", PathTypeName( lm->polytype ) },
                                     { "is_open", lm->is_open } } );
    }

    JSON scanlines = JSON::array();
    std::priority_queue<int64_t> queue = base.Field<SCANLINES>();

    while( !queue.empty() )
    {
        scanlines.push_back( queue.top() );
        queue.pop();
    }

    auto handles = [&]( const std::vector<c2::Active*>& aList ) -> JSON
    {
        JSON out = JSON::array();

        for( c2::Active* e : aList )
            out.push_back( HandleOf( e ) );

        return out;
    };

    JSON edges = JSON::array();

    for( size_t i = 0; i < m_handles.size(); ++i )
    {
        if( !m_alive[i] )
        {
            edges.push_back( nullptr );
            continue;
        }

        const c2::Active* a = m_handles[i];
        edges.push_back( JSON{ { "bot", WritePoint( a->bot ) },
                               { "top", WritePoint( a->top ) },
                               { "curr_x", a->curr_x },
                               { "dx", Bits( a->dx ) },
                               { "wind_dx", a->wind_dx },
                               { "wind_cnt", a->wind_cnt },
                               { "wind_cnt2", a->wind_cnt2 },
                               { "outrec", OutRecOf( a->outrec ) },
                               { "prev_in_ael", HandleOf( a->prev_in_ael ) },
                               { "next_in_ael", HandleOf( a->next_in_ael ) },
                               { "prev_in_sel", HandleOf( a->prev_in_sel ) },
                               { "next_in_sel", HandleOf( a->next_in_sel ) },
                               { "jump", HandleOf( a->jump ) },
                               { "vertex_top", VRef( a->vertex_top ) },
                               { "local_min", MinimaPosition( a->local_min ) },
                               { "is_left_bound", a->is_left_bound },
                               { "join_with", JoinName( a->join_with ) } } );
    }

    JSON outrecs = JSON::array();

    for( c2::OutRec* rec : base.outrec_list_ )
    {
        JSON ring = nullptr;
        bool prevOk = true;

        if( rec->pts )
        {
            ring = JSON::array();

            for( c2::OutPt* op : Ring( rec->pts ) )
            {
                prevOk = prevOk && op->next->prev == op;
                const JSON owner = OutRecOf( op->outrec );
                ring.push_back( JSON::array( { op->pt.x, op->pt.y, op->pt.z,
                                               owner.is_null() ? JSON( -1 ) : owner } ) );
            }
        }

        JSON splits = nullptr;

        if( rec->splits )
        {
            splits = JSON::array();

            for( c2::OutRec* split : *rec->splits )
                splits.push_back( OutRecOf( split ) );
        }

        JSON pip = nullptr;

        if( rec->pip_index )
            pip = JSON{ { "edge_count", rec->pip_index->EdgeCount() }, { "empty", rec->pip_index->Empty() } };

        outrecs.push_back( JSON{ { "idx", rec->idx },
                                 { "owner", OutRecOf( rec->owner ) },
                                 { "front_edge", HandleOf( rec->front_edge ) },
                                 { "back_edge", HandleOf( rec->back_edge ) },
                                 { "is_open", rec->is_open },
                                 { "pts", ring },
                                 { "prev_ok", prevOk },
                                 { "splits", splits },
                                 { "pip_index", pip },
                                 { "path", WritePath( rec->path ) } } );
    }

    JSON locminIter = nullptr;

    if( m_iterValid )
        locminIter = base.Field<LOCMIN_ITER>() - base.Field<MINIMA>().begin();

    return JSON{ { "vertices", vertices },
                 { "minima", minima },
                 { "reuse_minima", reuseMinima },
                 { "locmin_iter", locminIter },
                 { "sorted", base.Field<SORTED>() },
                 { "has_open_paths", base.has_open_paths_ },
                 { "succeeded", base.succeeded_ },
                 { "scanlines", scanlines },
                 { "ael", handles( Ael() ) },
                 { "sel", handles( Sel() ) },
                 { "edges", edges },
                 { "outrecs", outrecs },
                 { "z_calls", JSON( *m_calls ) } };
}


JSON RunEngine( const JSON& aIn, const std::string& aOp )
{
    Keys( aIn, { "op", "steps" } );

    if( !aIn.at( "steps" ).is_array() || aIn.at( "steps" ).size() > MAX_STEPS )
        Invalid( "steps" );

    ENGINE engine( aOp );
    JSON   results = JSON::array();

    for( const JSON& step : aIn.at( "steps" ) )
        results.push_back( engine.Step( step ) );

    return JSON{ { "results", results }, { "state", engine.Dump() } };
}


JSON Run( const JSON& aIn )
{
    const std::string& op = Text( aIn, "op" );

    if( op == "stripe_index" )
        return RunStripeIndex( aIn, op );

    if( op == "engine" )
        return RunEngine( aIn, op );

    return RunPure( op, aIn );
}
} // namespace


namespace oracle
{
void RegisterClipper2EngineP001( REGISTRY& aRegistry )
{
    aRegistry.Add( ENGINE_P001_UNIT, "thirdparty/clipper2/Clipper2Lib/src/clipper.engine.cpp", Run );
}
} // namespace oracle
