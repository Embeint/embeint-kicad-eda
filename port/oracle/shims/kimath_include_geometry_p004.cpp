// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned shape_line_chain.h, shape_index.h and shape_index_list.h inline bodies (unit
// libs.kimath.include.geometry.application.ui.p004), compiled unchanged into this adapter and
// calling, through them, the genuine kimath library (shape_line_chain.cpp, shape_collisions.cpp)
// and the pinned COW R-tree header.
//
// The adapter decodes 32-bit points and 64-bit sizes, drives line chains, shape indices and
// shape index lists by scripts of calls, and encodes every observable value exactly; it never
// computes a reply itself. Before each call a guard walks the pinned evaluation and refuses the
// request when it would reach undefined behaviour (signed overflow, an index outside a vector, a
// dereference or increment of an end or null iterator, an out-of-range floating-to-integer
// conversion, copying an indeterminate value).
//
// The accepted KIRTREE::COW_RTREE is guarded exactly: before an insertion the guard walks the
// genuine tree read-only along the path insertSimple takes and evaluates ChildEnlargement's
// int64_t products and difference and, at a full leaf, simpleSplit's ELEMTYPE extents; Reindex
// is replayed on a scratch tree; BulkLoad's Hilbert coordinates are checked before their
// floating-to-integer conversion.
//
// Calls into out-of-line bodies the Rust port defers (it stops before them) and whose undefined
// domain is not replayed here are accepted only inside a verified domain, and anything else is
// refused as "outside the verified domain" before the call:
// - shape_line_chain.cpp on chains without arcs (every chain this adapter builds): Segment with
//   an index its wxCHECK accepts, Remove with an index of at least -PointCount(), and
//   SquaredDistance with every point and the query point within 2^20;
// - shape_collisions.cpp (SHAPE::Collide( const SHAPE* )): a circle against a circle, both
//   within 2^20, clearance in [0, 2^20]; SHAPE_CIRCLE and SHAPE_LINE_CHAIN Collide( SEG ) or
//   ( VECTOR2I ) with every coordinate, radius, width and clearance within 2^20.
//
// One harness limit is not a claim about the reference: ReservePoints above 2^22 points and up to
// std::vector<VECTOR2I>::max_size() is a defined allocation whose success depends on the host, so
// it is refused as "harness allocation budget exceeded". Sizes above max_size() throw
// std::length_error before allocating and are compared.
//
// A failed wxCHECK in shapeFunctor, boundingBox or collide calls wxOnAssert; the calls are
// recorded around the process's assertion handler and returned with the reply.
#include "registry.h"
#include <geometry/seg.h>
#include <geometry/shape.h>
#include <geometry/shape_circle.h>
#include <geometry/shape_index.h>
#include <geometry/shape_index_list.h>
#include <geometry/shape_line_chain.h>
#include <math/box2.h>
#include <wx/debug.h>
#include <wx/string.h>
#include <algorithm>
#include <array>
#include <climits>
#include <cmath>
#include <cstdint>
#include <limits>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

namespace
{
using oracle::JSON;
constexpr const char* P004_UNIT = "libs.kimath.include.geometry.application.ui.p004";
constexpr int64_t     DOMAIN = int64_t( 1 ) << 20;
constexpr uint64_t    RESERVE_BUDGET = uint64_t( 1 ) << 22;
constexpr int         CHAINS = 4;
constexpr int         INDICES = 3;
constexpr int         LISTS = 2;
constexpr size_t      ITEMS = 64;
constexpr size_t      STEPS = 256;

[[noreturn]] void undefined()
{
    throw std::invalid_argument( "undefined p004 operation" );
}


[[noreturn]] void outside()
{
    throw std::invalid_argument( "outside the verified domain" );
}


// A host resource limit of this harness, not a property of the reference input.
[[noreturn]] void budget()
{
    throw std::invalid_argument( "harness allocation budget exceeded" );
}


// ---------------------------------------------------------------------------------------------
// Decoding and encoding.

void checkKeys( const JSON& aInput, std::initializer_list<const char*> aRequired,
                std::initializer_list<const char*> aOptional = {} )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );
    for( const char* field : aRequired )
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
    for( const auto& [key, value] : aInput.items() )
    {
        bool known = false;
        for( const char* field : aRequired )
            known = known || key == field;
        for( const char* field : aOptional )
            known = known || key == field;
        if( !known )
            throw std::invalid_argument( "unexpected or missing field" );
    }
}


std::string readText( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );
    return aInput.at( aName ).get<std::string>();
}


bool readBool( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_boolean() )
        throw std::invalid_argument( std::string( "expected bool: " ) + aName );
    return aInput.at( aName ).get<bool>();
}


bool readBool( const JSON& aInput, const char* aName, bool aDefault )
{
    return aInput.contains( aName ) ? readBool( aInput, aName ) : aDefault;
}


int64_t readI64( const JSON& aValue )
{
    if( !aValue.is_number_integer()
        || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > uint64_t( INT64_MAX ) ) )
        throw std::invalid_argument( "integer out of range" );
    return aValue.get<int64_t>();
}


int readInt( const JSON& aValue )
{
    const int64_t v = readI64( aValue );
    if( v < INT_MIN || v > INT_MAX )
        throw std::invalid_argument( "integer out of range" );
    return static_cast<int>( v );
}


int readInt( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( "unexpected or missing field" );
    return readInt( aInput.at( aName ) );
}


int64_t readI64( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( "unexpected or missing field" );
    return readI64( aInput.at( aName ) );
}


uint64_t readU64( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( "unexpected or missing field" );
    const JSON& v = aInput.at( aName );
    if( !v.is_number_integer() || ( !v.is_number_unsigned() && v.get<int64_t>() < 0 ) )
        throw std::invalid_argument( "integer out of range" );
    return v.get<uint64_t>();
}


int readSlot( const JSON& aInput, const char* aName, int aCount )
{
    const int slot = readInt( aInput, aName );
    if( slot < 0 || slot >= aCount )
        throw std::invalid_argument( "invalid slot" );
    return slot;
}


VECTOR2I readPoint( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    return VECTOR2I( readInt( aValue[0] ), readInt( aValue[1] ) );
}


VECTOR2I readPoint( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( "unexpected or missing field" );
    return readPoint( aInput.at( aName ) );
}


const JSON& readArray( const JSON& aInput, const char* aName, size_t aLimit )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() || aInput.at( aName ).size() > aLimit )
        throw std::invalid_argument( std::string( "expected array: " ) + aName );
    return aInput.at( aName );
}


JSON point( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


JSON boxJson( const BOX2I& aBox )
{
    return JSON{ { "pos", point( aBox.GetPosition() ) },
                 { "size", JSON::array( { aBox.GetSize().x, aBox.GetSize().y } ) },
                 { "valid", aBox.IsValid() } };
}


JSON segJson( const SEG& aSeg )
{
    return JSON{ { "a", point( aSeg.A ) }, { "b", point( aSeg.B ) }, { "index", aSeg.Index() } };
}


// ---------------------------------------------------------------------------------------------
// Integer guards.

int64_t L( __int128 aValue )
{
    if( aValue < INT64_MIN || aValue > INT64_MAX )
        undefined();
    return static_cast<int64_t>( aValue );
}


int I( int64_t aValue )
{
    if( aValue < INT_MIN || aValue > INT_MAX )
        undefined();
    return static_cast<int>( aValue );
}


VECTOR2I addI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2I( I( int64_t( aA.x ) + aB.x ), I( int64_t( aA.y ) + aB.y ) );
}


VECTOR2I subI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2I( I( int64_t( aA.x ) - aB.x ), I( int64_t( aA.y ) - aB.y ) );
}


bool within( int64_t aValue, int64_t aLimit = DOMAIN )
{
    return aValue >= -aLimit && aValue <= aLimit;
}


bool within( const VECTOR2I& aPoint, int64_t aLimit = DOMAIN )
{
    return within( aPoint.x, aLimit ) && within( aPoint.y, aLimit );
}


// BOX2::Compute: SetSize( vmax - vmin ) subtracts VECTOR2I components as int.
void guardCompute( const std::vector<VECTOR2I>& aPoints )
{
    if( aPoints.empty() )
        return;

    VECTOR2I lo = aPoints[0], hi = aPoints[0];
    for( const VECTOR2I& p : aPoints )
    {
        lo.x = std::min( lo.x, p.x );
        lo.y = std::min( lo.y, p.y );
        hi.x = std::max( hi.x, p.x );
        hi.y = std::max( hi.y, p.y );
    }
    subI( hi, lo );
}


// SHAPE_LINE_CHAIN::BBox: Compute, then the int sum aClearance + m_width. BOX2::Inflate works in
// int64_t on sizes below 2^34 and cannot overflow.
void guardChainBBox( const SHAPE_LINE_CHAIN& aChain, int aClearance )
{
    guardCompute( aChain.CPoints() );
    if( aClearance != 0 || aChain.Width() != 0 )
        I( int64_t( aClearance ) + aChain.Width() );
}


// SHAPE_CIRCLE::BBox, as the p003 adapter guards it.
void guardCircleBBox( const SHAPE_CIRCLE& aCircle, int aClearance )
{
    const int r = I( int64_t( aCircle.GetRadius() ) + aClearance );
    subI( aCircle.GetCenter(), VECTOR2I( r, r ) );
    I( int64_t( r ) * 2 );
}


void guardShapeBBox( const SHAPE& aShape )
{
    if( aShape.Type() == SH_CIRCLE )
        guardCircleBBox( static_cast<const SHAPE_CIRCLE&>( aShape ), 0 );
    else if( aShape.Type() == SH_LINE_CHAIN )
        guardChainBBox( static_cast<const SHAPE_LINE_CHAIN&>( aShape ), 0 );
    else
        throw std::logic_error( "unexpected shape" );
}


// BOX2::SquaredDistance( const BOX2& ): the int64_t squares and sums.
int64_t guardSquaredDistance( const BOX2I& aThis, const BOX2I& aBox )
{
    int64_t    s = 0;
    const auto axis = [&]( int aP, int64_t aZ, int aBp, int64_t aBz )
    {
        if( aBp + aBz < aP )
        {
            const int64_t d = aBp + aBz - aP;
            s = L( __int128( s ) + L( __int128( d ) * d ) );
        }
        else if( aBp > aP + aZ )
        {
            const int64_t d = aBp - aZ - aP;
            s = L( __int128( s ) + L( __int128( d ) * d ) );
        }
    };
    axis( aThis.GetX(), aThis.GetWidth(), aBox.GetX(), aBox.GetWidth() );
    axis( aThis.GetY(), aThis.GetHeight(), aBox.GetY(), aBox.GetHeight() );
    return s;
}


// The domain of an out-of-line call on a shape: a circle or an arc-free chain within 2^20.
void shapeDomain( const SHAPE& aShape )
{
    if( aShape.Type() == SH_CIRCLE )
    {
        const auto& c = static_cast<const SHAPE_CIRCLE&>( aShape );
        if( !within( c.GetCenter() ) || c.GetRadius() < 0 || c.GetRadius() > DOMAIN )
            outside();
        return;
    }
    const auto& chain = static_cast<const SHAPE_LINE_CHAIN&>( aShape );
    if( aShape.Type() != SH_LINE_CHAIN || chain.ArcCount() != 0 || !within( chain.Width() ) )
        outside();
    for( const VECTOR2I& p : chain.CPoints() )
        if( !within( p ) )
            outside();
}


// SHAPE::Collide( const SHAPE*, int ) (shape_collisions.cpp): circles within 2^20.
void collideShapeDomain( const SHAPE& aShape, const SHAPE* aOther, int aClearance )
{
    if( !aOther || aShape.Type() != SH_CIRCLE || aOther->Type() != SH_CIRCLE || aClearance < 0
        || aClearance > DOMAIN )
        outside();
    shapeDomain( aShape );
    shapeDomain( *aOther );
}



// ---------------------------------------------------------------------------------------------
// Line chains.

// Protected members, reached through a derived class's member pointers.
struct CHAIN_ACCESS : SHAPE_LINE_CHAIN
{
    static ssize_t ReversedArcIndex( const SHAPE_LINE_CHAIN& aChain, size_t aSegment )
    {
        return ( aChain.*&CHAIN_ACCESS::reversedArcIndex )( aSegment );
    }

    static void AmendArcStart( SHAPE_LINE_CHAIN& aChain, size_t aArc, const VECTOR2I& aPoint )
    {
        ( aChain.*&CHAIN_ACCESS::amendArcStart )( aArc, aPoint );
    }

    static void AmendArcEnd( SHAPE_LINE_CHAIN& aChain, size_t aArc, const VECTOR2I& aPoint )
    {
        ( aChain.*&CHAIN_ACCESS::amendArcEnd )( aArc, aPoint );
    }
};


JSON chainState( const SHAPE_LINE_CHAIN& aChain )
{
    JSON points = JSON::array();
    for( const VECTOR2I& p : aChain.CPoints() )
        points.push_back( point( p ) );
    JSON shapes = JSON::array();
    for( const auto& [first, second] : aChain.CShapes() )
        shapes.push_back( JSON::array( { first, second } ) );
    return JSON{ { "points", points },
                 { "shapes", shapes },
                 { "arcs", aChain.ArcCount() },
                 { "closed", aChain.IsClosed() },
                 { "width", aChain.Width() },
                 { "cached_bbox", boxJson( *aChain.GetCachedBBox() ) } };
}


// m_points[i] for a CPoint index.
void guardCPoint( const SHAPE_LINE_CHAIN& aChain, int aIndex )
{
    const int count = aChain.PointCount();
    int       index = aIndex;
    if( index < 0 )
        index += count;
    else if( index >= count )
        index -= count;
    if( index < 0 || index >= count )
        undefined();
}


// Segment( aIndex ): the wxCHECK accepts an adjusted index in [0, SegmentCount()).
void segmentDomain( const SHAPE_LINE_CHAIN& aChain, int aIndex )
{
    const int segCount = aChain.SegmentCount();
    const int index = aIndex < 0 ? aIndex + segCount : aIndex;
    if( index < 0 || index >= segCount )
        outside();
}


// SHAPE_LINE_CHAIN::Move: every point, then the arcs (SHAPE_ARC::Move, none here), then the box.
void guardMove( const SHAPE_LINE_CHAIN& aChain, const VECTOR2I& aVector )
{
    for( const VECTOR2I& p : aChain.CPoints() )
        addI( p, aVector );
    if( aChain.ArcCount() != 0 )
        outside();
    addI( aChain.GetCachedBBox()->GetPosition(), aVector );
}


JSON chainCall( std::array<std::unique_ptr<SHAPE_LINE_CHAIN>, CHAINS>& aChains, const JSON& aStep )
{
    const std::string name = readText( aStep, "call" );
    const int         a = readSlot( aStep, "a", CHAINS );
    SHAPE_LINE_CHAIN& c = *aChains[a];
    const auto        plain = [&]() { checkKeys( aStep, { "call", "a" } ); };

    if( name == "new" )
    {
        plain();
        aChains[a] = std::make_unique<SHAPE_LINE_CHAIN>();
        return nullptr;
    }
    if( name == "copy" || name == "assign" || name == "move_assign" || name == "ne" )
    {
        checkKeys( aStep, { "call", "a", "src" } );
        const int src = readSlot( aStep, "src", CHAINS );
        if( aChains[src]->ArcCount() != 0 )
            outside();
        if( name == "copy" )
            aChains[a] = std::make_unique<SHAPE_LINE_CHAIN>( *aChains[src] );
        else if( name == "assign" )
            c = *aChains[src];
        else if( name == "move_assign" )
            c = std::move( *aChains[src] );
        else
            return c != *aChains[src];
        return nullptr;
    }
    if( name == "clear" )
    {
        plain();
        c.Clear();
        return nullptr;
    }
    if( name == "set_closed" )
    {
        checkKeys( aStep, { "call", "a", "closed" } );
        // mergeFirstLastPointIfNeeded on a chain without arcs.
        if( c.ArcCount() != 0 )
            outside();
        c.SetClosed( readBool( aStep, "closed" ) );
        return nullptr;
    }
    if( name == "set_width" )
    {
        checkKeys( aStep, { "call", "a", "width" } );
        c.SetWidth( readInt( aStep, "width" ) );
        return nullptr;
    }
    if( name == "is_closed" || name == "width" || name == "segment_count" || name == "point_count"
        || name == "get_point_count" || name == "get_segment_count" || name == "is_solid"
        || name == "arc_count" || name == "cpoints" || name == "cshapes" || name == "carcs"
        || name == "cached_bbox" || name == "clast_point" || name == "generate_bbox_cache"
        || name == "type" )
    {
        plain();
        if( name == "is_closed" )
            return c.IsClosed();
        if( name == "width" )
            return c.Width();
        if( name == "segment_count" )
            return c.SegmentCount();
        if( name == "point_count" )
            return c.PointCount();
        if( name == "get_point_count" )
            return c.GetPointCount();
        if( name == "get_segment_count" )
            return c.GetSegmentCount();
        if( name == "is_solid" )
            return c.IsSolid();
        if( name == "arc_count" )
            return c.ArcCount();
        if( name == "carcs" )
            return c.CArcs().size();
        if( name == "cached_bbox" )
            return boxJson( *c.GetCachedBBox() );
        if( name == "type" )
            return JSON{ { "type", static_cast<int>( c.Type() ) }, { "width", c.GetWidth() } };
        if( name == "clast_point" )
        {
            if( c.PointCount() == 0 )
                undefined();
            return point( c.CLastPoint() );
        }
        if( name == "generate_bbox_cache" )
        {
            guardCompute( c.CPoints() );
            c.GenerateBBoxCache();
            return nullptr;
        }
        return chainState( c ).at( name == "cpoints" ? "points" : "shapes" );
    }
    if( name == "cpoint" || name == "get_point" )
    {
        checkKeys( aStep, { "call", "a", "index" } );
        const int index = readInt( aStep, "index" );
        guardCPoint( c, index );
        return point( name == "cpoint" ? c.CPoint( index ) : c.GetPoint( index ) );
    }
    if( name == "csegment" || name == "get_segment" )
    {
        checkKeys( aStep, { "call", "a", "index" } );
        const int index = readInt( aStep, "index" );
        segmentDomain( c, index );
        return segJson( name == "csegment" ? c.CSegment( index ) : c.GetSegment( index ) );
    }
    if( name == "bbox" )
    {
        checkKeys( aStep, { "call", "a" }, { "clearance" } );
        if( !aStep.contains( "clearance" ) )
        {
            guardChainBBox( c, 0 );
            return boxJson( c.BBox() );
        }
        const int clearance = readInt( aStep, "clearance" );
        guardChainBBox( c, clearance );
        return boxJson( c.BBox( clearance ) );
    }
    if( name == "reserve_points" )
    {
        checkKeys( aStep, { "call", "a", "size" } );
        const uint64_t size = readU64( aStep, "size" );
        if( size > RESERVE_BUDGET && size <= std::vector<VECTOR2I>().max_size() )
            budget();
        try
        {
            c.ReservePoints( size );
        }
        catch( const std::length_error& )
        {
            return JSON{ { "threw", "length_error" } };
        }
        return nullptr;
    }
    if( name == "append" || name == "append_xy" )
    {
        if( name == "append" )
            checkKeys( aStep, { "call", "a", "point" }, { "allow_duplication" } );
        else
            checkKeys( aStep, { "call", "a", "x", "y" }, { "allow_duplication" } );
        // BOX2::Merge on int boxes works in int64_t and cannot overflow.
        const bool has = aStep.contains( "allow_duplication" );
        const bool dup = readBool( aStep, "allow_duplication", false );
        if( name == "append" )
        {
            const VECTOR2I p = readPoint( aStep, "point" );
            if( has )
                c.Append( p, dup );
            else
                c.Append( p );
        }
        else
        {
            const int x = readInt( aStep, "x" );
            const int y = readInt( aStep, "y" );
            if( has )
                c.Append( x, y, dup );
            else
                c.Append( x, y );
        }
        return nullptr;
    }
    if( name == "remove" )
    {
        checkKeys( aStep, { "call", "a", "index" } );
        const int index = readInt( aStep, "index" );
        if( c.ArcCount() != 0 || int64_t( index ) < -int64_t( c.PointCount() ) )
            outside();
        c.Remove( index );
        return nullptr;
    }
    if( name == "move" )
    {
        checkKeys( aStep, { "call", "a", "vector" } );
        const VECTOR2I vector = readPoint( aStep, "vector" );
        guardMove( c, vector );
        c.Move( vector );
        return nullptr;
    }
    if( name == "arc_index" || name == "reversed_arc_index" )
    {
        checkKeys( aStep, { "call", "a", "segment" } );
        const uint64_t segment = readU64( aStep, "segment" );
        // IsSharedPt checks its own bound; m_shapes[aSegment] follows.
        if( segment >= c.CShapes().size() )
            undefined();
        return name == "arc_index" ? c.ArcIndex( segment )
                                   : CHAIN_ACCESS::ReversedArcIndex( c, segment );
    }
    if( name == "arc" || name == "amend_arc_start" || name == "amend_arc_end" )
    {
        if( name == "arc" )
            checkKeys( aStep, { "call", "a", "index" } );
        else
            checkKeys( aStep, { "call", "a", "index", "point" } );
        const uint64_t index = readU64( aStep, "index" );
        // Every chain here is arc-free: m_arcs[aArc] is outside the vector.
        if( index >= c.ArcCount() )
            undefined();
        outside();
    }
    if( name == "distance" )
    {
        checkKeys( aStep, { "call", "a", "point", "outline_only" } );
        const VECTOR2I p = readPoint( aStep, "point" );
        const bool     outlineOnly = readBool( aStep, "outline_only" );
        shapeDomain( c );
        if( !within( p ) )
            outside();
        // sqrt( SEG::ecoord ) converts to int: undefined at or above 2^31.
        const int64_t squared = c.SquaredDistance( p, outlineOnly );
        if( !( std::trunc( std::sqrt( double( squared ) ) ) < 2147483648.0 ) )
            undefined();
        return c.Distance( p, outlineOnly );
    }
    throw std::invalid_argument( "unknown call" );
}


JSON runChain( const JSON& aInput )
{
    checkKeys( aInput, { "op", "steps" } );
    std::array<std::unique_ptr<SHAPE_LINE_CHAIN>, CHAINS> chains;
    for( auto& c : chains )
        c = std::make_unique<SHAPE_LINE_CHAIN>();

    JSON replies = JSON::array();
    for( const JSON& step : readArray( aInput, "steps", STEPS ) )
        replies.push_back( chainCall( chains, step ) );

    JSON states = JSON::array();
    for( const auto& c : chains )
        states.push_back( chainState( *c ) );
    return JSON{ { "replies", replies }, { "chains", states } };
}


// ---------------------------------------------------------------------------------------------
// Items, shapes and the assertion recorder for the indices.

std::unique_ptr<SHAPE> buildShape( const JSON& aSpec )
{
    const std::string kind = readText( aSpec, "kind" );
    if( kind == "circle" )
    {
        checkKeys( aSpec, { "kind", "center", "radius" } );
        return std::make_unique<SHAPE_CIRCLE>( readPoint( aSpec, "center" ), readInt( aSpec, "radius" ) );
    }
    if( kind == "chain" )
    {
        checkKeys( aSpec, { "kind", "points" }, { "width" } );
        auto chain = std::make_unique<SHAPE_LINE_CHAIN>();
        for( const JSON& p : readArray( aSpec, "points", ITEMS ) )
            chain->Append( readPoint( p ) );
        if( aSpec.contains( "width" ) )
            chain->SetWidth( readInt( aSpec, "width" ) );
        return chain;
    }
    throw std::invalid_argument( "unknown shape" );
}


JSON shapeState( const SHAPE* aShape )
{
    if( !aShape )
        return nullptr;
    if( aShape->Type() == SH_CIRCLE )
    {
        const auto* c = static_cast<const SHAPE_CIRCLE*>( aShape );
        return JSON{ { "type", "SH_CIRCLE" }, { "center", point( c->GetCenter() ) },
                     { "radius", c->GetRadius() } };
    }
    JSON state = chainState( *static_cast<const SHAPE_LINE_CHAIN*>( aShape ) );
    state["type"] = "SH_LINE_CHAIN";
    return state;
}


/// A test item: a shape (or none) that Shape( aLayer ) returns on its layers.
struct ITEM
{
    int                    tag = 0;
    std::unique_ptr<SHAPE> shape;
    std::vector<int>       layers;

    const SHAPE* Shape( int aLayer ) const
    {
        return std::find( layers.begin(), layers.end(), aLayer ) != layers.end() ? shape.get() : nullptr;
    }
};


std::vector<std::unique_ptr<ITEM>> readItems( const JSON& aInput )
{
    std::vector<std::unique_ptr<ITEM>> items;
    for( const JSON& spec : readArray( aInput, "items", ITEMS ) )
    {
        checkKeys( spec, { "shape", "layers" } );
        auto item = std::make_unique<ITEM>();
        item->tag = static_cast<int>( items.size() );
        if( !spec.at( "shape" ).is_null() )
            item->shape = buildShape( spec.at( "shape" ) );
        for( const JSON& layer : readArray( spec, "layers", 8 ) )
            item->layers.push_back( readInt( layer ) );
        items.push_back( std::move( item ) );
    }
    return items;
}


ITEM* readItem( const JSON& aInput, const char* aName, const std::vector<std::unique_ptr<ITEM>>& aItems )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( "unexpected or missing field" );
    const JSON& v = aInput.at( aName );
    if( v.is_null() )
        return nullptr;
    const int64_t n = readI64( v );
    if( n < 0 || n >= int64_t( aItems.size() ) )
        throw std::invalid_argument( "invalid item" );
    return aItems[n].get();
}


JSON wire( const ITEM* aItem )
{
    return aItem ? JSON( aItem->tag ) : JSON( nullptr );
}


/// The wxOnAssert calls of the current operation, and the handler the recorder wraps.
JSON*             g_assertions = nullptr;
wxAssertHandler_t g_wrappedHandler = nullptr;


void RecordAssertion( const wxString& aFile, int aLine, const wxString& aFunc, const wxString& aCond,
                      const wxString& aMsg )
{
    if( g_assertions )
    {
        if( !aFile.EndsWith( wxS( "geometry/shape_index.h" ) ) )
            throw std::logic_error( "unexpected wx assertion site" );

        g_assertions->push_back( JSON{ { "line", aLine },
                                       { "function", aFunc.utf8_string() },
                                       { "condition", aCond.utf8_string() },
                                       { "message", aMsg.utf8_string() } } );
    }

    if( g_wrappedHandler )
        g_wrappedHandler( aFile, aLine, aFunc, aCond, aMsg );
}


/// Installs RecordAssertion around the process's handler for one operation, and only if a
/// handler is installed (wxCHECK calls wxOnAssert only then).
class ASSERTION_SCOPE
{
public:
    explicit ASSERTION_SCOPE( JSON& aCalls )
    {
        g_assertions = &aCalls;
        g_wrappedHandler = wxTheAssertHandler;

        if( g_wrappedHandler )
            wxSetAssertHandler( RecordAssertion );
    }

    ~ASSERTION_SCOPE()
    {
        if( g_wrappedHandler )
            wxSetAssertHandler( g_wrappedHandler );

        g_assertions = nullptr;
        g_wrappedHandler = nullptr;
    }

    ASSERTION_SCOPE( const ASSERTION_SCOPE& ) = delete;
    ASSERTION_SCOPE& operator=( const ASSERTION_SCOPE& ) = delete;
};


/// A visitor recording each call with its own call count; it continues while fewer than
/// m_stop calls were made through this copy.
struct VISITOR
{
    JSON* m_out = nullptr;
    int   m_calls = 0;
    int   m_stop = INT_MAX;

    bool operator()( ITEM* aItem )
    {
        m_out->push_back( JSON::array( { m_calls, wire( aItem ) } ) );
        ++m_calls;
        return m_calls < m_stop;
    }
};


// The box an index computes for an item: boundingBox( item, layer ), guarded.
BOX2I guardItemBox( ITEM* aItem, int aLayer )
{
    if( !aItem )
        return BOX2I();
    const SHAPE* shape = aItem->Shape( aLayer );
    if( !shape )
        return BOX2I();
    guardShapeBBox( *shape );
    return shape->BBox();
}


// collide( T, U, aLayer, aMinDistance ) target.
JSON runCollide( ITEM* aItem, int aLayer, int aMinDistance, const JSON& aTarget )
{
    const SHAPE* shape = aItem ? aItem->Shape( aLayer ) : nullptr;
    checkKeys( aTarget, {}, { "point", "seg", "shape" } );
    if( aTarget.size() != 1 )
        throw std::invalid_argument( "expected one target" );

    if( aTarget.contains( "shape" ) )
    {
        std::unique_ptr<SHAPE> other = buildShape( aTarget.at( "shape" ) );
        if( shape )
            collideShapeDomain( *shape, other.get(), aMinDistance );
        return collide( aItem, static_cast<const SHAPE*>( other.get() ), aLayer, aMinDistance );
    }

    const bool isPoint = aTarget.contains( "point" );
    SEG        seg;
    VECTOR2I   p;
    if( isPoint )
        p = readPoint( aTarget, "point" );
    else
    {
        const JSON& s = aTarget.at( "seg" );
        if( !s.is_array() || s.size() != 2 )
            throw std::invalid_argument( "expected segment: seg" );
        seg = SEG( readPoint( s[0] ), readPoint( s[1] ) );
    }

    if( shape )
    {
        shapeDomain( *shape );
        if( !within( aMinDistance ) || ( isPoint ? !within( p ) : !within( seg.A ) || !within( seg.B ) ) )
            outside();
    }
    return isPoint ? collide( aItem, p, aLayer, aMinDistance ) : collide( aItem, seg, aLayer, aMinDistance );
}


// ---------------------------------------------------------------------------------------------
// SHAPE_INDEX.

using INDEX = SHAPE_INDEX<ITEM*>;
using TREE = KIRTREE::COW_RTREE<ITEM*, int, 2>; // SHAPE_INDEX<ITEM*>::TREE_TYPE
using NODE = TREE::NODE;
constexpr int TREE_FANOUT = sizeof( NODE::children ) / sizeof( NODE::children[0] );


// SHAPE_INDEX::m_tree and COW_RTREE::m_root are private; an explicit instantiation names them
// without an access check, and the guards only read through them.
template <class TAG, auto MEMBER>
struct ACCESS
{
    friend auto Get( TAG ) { return MEMBER; }
};
struct INDEX_TREE_TAG
{
    friend auto Get( INDEX_TREE_TAG );
};
struct TREE_ROOT_TAG
{
    friend auto Get( TREE_ROOT_TAG );
};
template struct ACCESS<INDEX_TREE_TAG, &INDEX::m_tree>;
template struct ACCESS<TREE_ROOT_TAG, &TREE::m_root>;


const TREE& treeOf( const INDEX& aIndex )
{
    return aIndex.*Get( INDEX_TREE_TAG{} );
}


// The corners SHAPE_INDEX passes to the tree for a box.
void corners( const BOX2I& aBox, int aMin[2], int aMax[2] )
{
    aMin[0] = aBox.GetX();
    aMin[1] = aBox.GetY();
    aMax[0] = aBox.GetRight();
    aMax[1] = aBox.GetBottom();
}


// COW_RTREE::Insert( aMin, aMax ) on the tree as it is: insertSimple descends from the root
// through every internal node, computing RTREE_NODE::ChildEnlargement for each child (ChildArea's
// and the enlarged area's int64_t products, then their difference) and taking the first child
// with the least increase; a full leaf is split by simpleSplit, whose per-axis extent hi - lo is
// an ELEMTYPE (int) subtraction. The remaining work (copying shared nodes, int64_t sort keys of
// two ints, min/max bounds) is defined. The walk reads the genuine nodes and changes nothing.
void guardInsert( const TREE& aTree, const int aMin[2], const int aMax[2] )
{
    const NODE* node = aTree.*Get( TREE_ROOT_TAG{} );

    if( !node )
        return; // Insert allocates an empty leaf.

    while( !node->IsLeaf() )
    {
        if( node->count < 1 || node->count > TREE_FANOUT )
            throw std::logic_error( "tree invariant violation" );

        int     best = 0;
        int64_t bestInc = std::numeric_limits<int64_t>::max();

        for( int i = 0; i < node->count; ++i )
        {
            int64_t area = 1;

            for( int d = 0; d < 2; ++d )
                area = L( __int128( area )
                          * ( int64_t( node->bounds[d * 2 + 1][i] ) - node->bounds[d * 2][i] ) );

            int64_t enlarged = 1;

            for( int d = 0; d < 2; ++d )
            {
                const int lo = std::min( node->bounds[d * 2][i], aMin[d] );
                const int hi = std::max( node->bounds[d * 2 + 1][i], aMax[d] );
                enlarged = L( __int128( enlarged ) * ( int64_t( hi ) - lo ) );
            }

            const int64_t inc = L( __int128( enlarged ) - area );

            if( inc < bestInc )
            {
                best = i;
                bestInc = inc;
            }
        }

        node = node->children[best];

        if( !node )
            throw std::logic_error( "tree invariant violation" );
    }

    if( node->IsFull() )
    {
        int nodeMin[2], nodeMax[2];
        node->ComputeEnclosingBounds( nodeMin, nodeMax );

        for( int d = 0; d < 2; ++d )
            I( int64_t( std::max( nodeMax[d], aMax[d] ) ) - std::min( nodeMin[d], aMin[d] ) );
    }
}


// COW_RTREE::BulkLoad: the per-axis range and scale over every entry, then for each entry the
// centre's ( c - globalMin ) * scale, converted to uint32_t (undefined unless in (-1, 2^32)).
// Packing the sorted entries into nodes involves no further arithmetic on coordinates.
void guardBulkLoad( const std::vector<std::pair<ITEM*, BOX2I>>& aItems )
{
    if( aItems.empty() )
        return;

    int lo[2], hi[2];
    corners( aItems[0].second, lo, hi );

    for( const auto& [item, box] : aItems )
    {
        int min[2], max[2];
        corners( box, min, max );

        for( int d = 0; d < 2; ++d )
        {
            lo[d] = std::min( lo[d], min[d] );
            hi[d] = std::max( hi[d], max[d] );
        }
    }

    double scale[2];

    for( int d = 0; d < 2; ++d )
    {
        const double range = static_cast<double>( hi[d] ) - lo[d];
        scale[d] = range > 0 ? 65535.0 / range : 0.0;
    }

    for( const auto& [item, box] : aItems )
    {
        int min[2], max[2];
        corners( box, min, max );

        for( int d = 0; d < 2; ++d )
        {
            const double c = ( static_cast<double>( min[d] ) + max[d] ) * 0.5;
            const double x = ( c - lo[d] ) * scale[d];

            if( !std::isfinite( x ) || x <= -1.0 || x >= 4294967296.0 )
                undefined();
        }
    }
}


// The indices, with the layer each was constructed with (m_shapeLayer, which the guards need).
struct INDICES_STATE
{
    std::array<std::unique_ptr<INDEX>, INDICES> index;
    std::array<int, INDICES>                    layer{};
};


JSON indexCall( INDICES_STATE& aState, const std::vector<std::unique_ptr<ITEM>>& aItems,
                const JSON& aStep )
{
    const std::string name = readText( aStep, "call" );

    if( name == "shape_functor" || name == "bounding_box" )
    {
        checkKeys( aStep, { "call", "item", "layer" } );
        ITEM*     item = readItem( aStep, "item", aItems );
        const int layer = readInt( aStep, "layer" );
        if( name == "shape_functor" )
            return shapeState( shapeFunctor( item, layer ) );
        guardItemBox( item, layer );
        return boxJson( boundingBox( item, layer ) );
    }
    if( name == "collide" )
    {
        checkKeys( aStep, { "call", "item", "layer", "min_distance", "target" } );
        return runCollide( readItem( aStep, "item", aItems ), readInt( aStep, "layer" ),
                           readInt( aStep, "min_distance" ), aStep.at( "target" ) );
    }
    if( name == "query_callback" )
    {
        checkKeys( aStep, { "call", "item" } );
        ITEM*   item = readItem( aStep, "item", aItems );
        JSON    out = JSON::array();
        VISITOR visitor{ &out, 0, INT_MAX };
        const bool returned = queryCallback<ITEM*, VISITOR>( item, &visitor );
        return JSON{ { "returned", returned }, { "visits", out } };
    }

    const int a = readSlot( aStep, "a", INDICES );

    if( name == "new" )
    {
        checkKeys( aStep, { "call", "a", "layer" } );
        aState.layer[a] = readInt( aStep, "layer" );
        aState.index[a] = std::make_unique<INDEX>( aState.layer[a] );
        return nullptr;
    }
    if( name == "clone" )
    {
        checkKeys( aStep, { "call", "a", "src" } );
        const int src = readSlot( aStep, "src", INDICES );
        if( !aState.index[src] )
            throw std::invalid_argument( "missing index" );
        aState.index[a] = std::make_unique<INDEX>( aState.index[src]->Clone() );
        aState.layer[a] = aState.layer[src];
        return nullptr;
    }
    if( !aState.index[a] )
        throw std::invalid_argument( "missing index" );
    INDEX&    index = *aState.index[a];
    const int layer = aState.layer[a];
    if( name == "destroy" )
    {
        checkKeys( aStep, { "call", "a" } );
        aState.index[a].reset();
        return nullptr;
    }
    if( name == "add" || name == "remove" )
    {
        checkKeys( aStep, { "call", "a", "item" } );
        ITEM*       item = readItem( aStep, "item", aItems );
        const BOX2I box = guardItemBox( item, layer );
        if( name == "add" )
        {
            int min[2], max[2];
            corners( box, min, max );
            guardInsert( treeOf( index ), min, max );
            index.Add( item );
        }
        else
            index.Remove( item );
        return nullptr;
    }
    if( name == "add_bbox" )
    {
        checkKeys( aStep, { "call", "a", "item", "pos", "size" } );
        ITEM*          item = readItem( aStep, "item", aItems );
        const VECTOR2I pos = readPoint( aStep, "pos" );
        const VECTOR2I size = readPoint( aStep, "size" );
        // BOX2's constructor and Normalize convert their int64_t sums modularly (defined).
        const BOX2I box( pos, VECTOR2L( size.x, size.y ) );
        int         min[2], max[2];
        corners( box, min, max );
        guardInsert( treeOf( index ), min, max );
        index.Add( item, box );
        return nullptr;
    }
    if( name == "remove_all" || name == "size" || name == "range" || name == "accept" || name == "reindex" )
    {
        checkKeys( aStep, { "call", "a" } );
        if( name == "remove_all" )
        {
            index.RemoveAll();
            return nullptr;
        }
        if( name == "size" )
            return index.Size();
        JSON out = JSON::array();
        if( name == "range" )
        {
            for( ITEM* item : index )
                out.push_back( wire( item ) );
            return out;
        }
        if( name == "accept" )
        {
            index.Accept( VISITOR{ &out, 0, INT_MAX } );
            return out;
        }
        // Reindex empties the tree and adds every item again, in iteration order, under its
        // current box: the same insertions are guarded and made on a scratch tree first.
        TREE scratch;
        for( ITEM* item : index )
        {
            int min[2], max[2];
            corners( guardItemBox( item, layer ), min, max );
            guardInsert( scratch, min, max );
            scratch.Insert( min, max, item );
        }
        index.Reindex();
        return nullptr;
    }
    if( name == "bulk_load" )
    {
        checkKeys( aStep, { "call", "a", "entries" } );
        std::vector<std::pair<ITEM*, BOX2I>> entries;
        for( const JSON& e : readArray( aStep, "entries", ITEMS ) )
        {
            checkKeys( e, { "item", "pos", "size" } );
            ITEM*          item = readItem( e, "item", aItems );
            const VECTOR2I pos = readPoint( e, "pos" );
            const VECTOR2I size = readPoint( e, "size" );
            entries.emplace_back( item, BOX2I( pos, VECTOR2L( size.x, size.y ) ) );
        }
        guardBulkLoad( entries );
        index.BulkLoad( entries );
        return nullptr;
    }
    if( name == "query" )
    {
        checkKeys( aStep, { "call", "a", "shape", "min_distance", "stop" } );
        std::unique_ptr<SHAPE> shape = buildShape( aStep.at( "shape" ) );
        const int              minDistance = readInt( aStep, "min_distance" );
        const int              stop = readInt( aStep, "stop" );
        guardShapeBBox( *shape );
        JSON    out = JSON::array();
        VISITOR visitor{ &out, 0, stop };
        const int found = index.Query( shape.get(), minDistance, visitor );
        return JSON{ { "found", found }, { "visits", out } };
    }
    if( name == "iterate" )
    {
        checkKeys( aStep, { "call", "a", "ops" } );
        INDEX::Iterator it = index.Begin();
        JSON            out = JSON::array();
        for( const JSON& op : readArray( aStep, "ops", STEPS ) )
        {
            if( !op.is_string() )
                throw std::invalid_argument( "expected op" );
            const std::string o = op.get<std::string>();
            if( o == "deref" )
                out.push_back( wire( *it ) );
            else if( o == "inc" )
                out.push_back( ++it );
            else if( o == "inc_post" )
                out.push_back( it++ );
            else if( o == "is_null" )
                out.push_back( it.IsNull() );
            else if( o == "is_not_null" )
                out.push_back( it.IsNotNull() );
            else if( o == "next" )
                out.push_back( wire( it.Next() ) );
            else
                throw std::invalid_argument( "unknown op" );
        }
        return out;
    }
    throw std::invalid_argument( "unknown call" );
}


JSON runIndex( const JSON& aInput )
{
    checkKeys( aInput, { "op", "items", "steps" } );
    std::vector<std::unique_ptr<ITEM>> items = readItems( aInput );
    INDICES_STATE                      indices;

    JSON assertions = JSON::array();
    JSON replies = JSON::array();
    {
        ASSERTION_SCOPE scope( assertions );
        for( const JSON& step : readArray( aInput, "steps", STEPS ) )
            replies.push_back( indexCall( indices, items, step ) );
    }
    return JSON{ { "replies", replies }, { "assertions", assertions } };
}


// ---------------------------------------------------------------------------------------------
// SHAPE_INDEX_LIST.

using LIST = SHAPE_INDEX_LIST<ITEM*>;

// The entries an adapter-side replica tracks: SHAPE_ENTRY's shape and box, in list order.
struct ENTRY
{
    ITEM*        parent;
    const SHAPE* shape;
    BOX2I        bbox;
};


// query_iterator::next() from position aPos: the guard of its Distance and Collide calls.
size_t guardNext( const std::vector<ENTRY>& aEntries, size_t aPos, const BOX2I& aRef,
                  const SHAPE* aShape, int aMinDistance, bool aExact )
{
    while( aPos != aEntries.size() )
    {
        const ENTRY& e = aEntries[aPos];
        guardSquaredDistance( aRef, e.bbox );
        if( aRef.Distance( e.bbox ) <= aMinDistance )
        {
            if( !aExact )
                return aPos;
            collideShapeDomain( *e.shape, aShape, aMinDistance );
            if( e.shape->Collide( aShape, aMinDistance ) )
                return aPos;
        }
        ++aPos;
    }
    return aPos;
}


// A list and the adapter-side replica of its entries.
struct LIST_STATE
{
    LIST               list;
    std::vector<ENTRY> entries;
};


// "compare": operator== and operator!= of two iterators, possibly of different lists, each
// { "l": list, "at": "begin" | "end" | "null" } and, for a normal iterator, an optional "inc"
// count of operator++ from there. A query iterator at "begin" is qbegin( nullptr, 0, false ),
// which finds no match; "null" is a default-constructed iterator, which is compared in place
// (operator== reads only its value-initialised m_current; copying it would read indeterminate
// members).
JSON compareIterators( std::array<LIST_STATE, LISTS>& aLists, const JSON& aStep )
{
    checkKeys( aStep, { "call", "kind", "left", "right" } );
    const std::string kind = readText( aStep, "kind" );

    if( kind == "iterator" )
    {
        auto make = [&]( const JSON& aSpec )
        {
            checkKeys( aSpec, { "l", "at" }, { "inc" } );
            LIST_STATE&       s = aLists[readSlot( aSpec, "l", LISTS )];
            const std::string at = readText( aSpec, "at" );
            const int         inc = aSpec.contains( "inc" ) ? readInt( aSpec, "inc" ) : 0;
            if( inc < 0 || inc > int( STEPS ) )
                throw std::invalid_argument( "invalid increment" );
            LIST::iterator it;
            size_t         pos = 0;
            if( at == "begin" )
                it = s.list.begin();
            else if( at == "end" )
            {
                it = s.list.end();
                pos = s.entries.size();
            }
            else if( at != "null" )
                throw std::invalid_argument( "unknown position" );
            for( int k = 0; k < inc; ++k )
            {
                // A null iterator, or the end (including a never-allocated list's null begin()).
                if( at == "null" || pos >= s.entries.size() )
                    undefined();
                ++it;
                ++pos;
            }
            return it;
        };
        const LIST::iterator left = make( aStep.at( "left" ) );
        const LIST::iterator right = make( aStep.at( "right" ) );
        return JSON{ { "eq", left == right }, { "ne", left != right } };
    }
    if( kind == "query" )
    {
        const LIST::query_iterator                         null;
        std::array<std::optional<LIST::query_iterator>, 2> made;
        auto pick = [&]( const JSON& aSpec, int aSide ) -> const LIST::query_iterator&
        {
            checkKeys( aSpec, { "l", "at" } );
            LIST_STATE&       s = aLists[readSlot( aSpec, "l", LISTS )];
            const std::string at = readText( aSpec, "at" );
            if( at == "null" )
                return null;
            if( at == "begin" )
                made[aSide].emplace( s.list.qbegin( nullptr, 0, false ) );
            else if( at == "end" )
                made[aSide].emplace( s.list.qend() );
            else
                throw std::invalid_argument( "unknown position" );
            return *made[aSide];
        };
        const LIST::query_iterator& left = pick( aStep.at( "left" ), 0 );
        const LIST::query_iterator& right = pick( aStep.at( "right" ), 1 );
        return JSON{ { "eq", left == right }, { "ne", left != right } };
    }
    throw std::invalid_argument( "unknown kind" );
}


JSON listCall( LIST& aList, std::vector<ENTRY>& aEntries, const std::vector<std::unique_ptr<ITEM>>& aItems,
               const JSON& aStep )
{
    const std::string name = readText( aStep, "call" );

    if( name == "add" || name == "remove" )
    {
        checkKeys( aStep, { "call", "item" } );
        ITEM* item = readItem( aStep, "item", aItems );
        if( name == "remove" )
        {
            aList.Remove( item );
            const auto found = std::find_if( aEntries.begin(), aEntries.end(),
                                             [&]( const ENTRY& e ) { return e.parent == item; } );
            if( found != aEntries.end() )
                aEntries.erase( found );
            return nullptr;
        }
        // defaultShapeFunctor dereferences the item; SHAPE_ENTRY dereferences the shape.
        if( !item || !item->Shape( -1 ) )
            undefined();
        const SHAPE* shape = item->Shape( -1 );
        guardShapeBBox( *shape );
        aList.Add( item );
        aEntries.push_back( ENTRY{ item, shape, shape->BBox( 0 ) } );
        return nullptr;
    }
    if( name == "size" || name == "clear" )
    {
        checkKeys( aStep, { "call" } );
        if( name == "size" )
            return aList.Size();
        aList.Clear();
        aEntries.clear();
        return nullptr;
    }
    if( name == "query" )
    {
        checkKeys( aStep, { "call", "shape", "min_distance", "stop" }, { "exact" } );
        std::unique_ptr<SHAPE> shape = buildShape( aStep.at( "shape" ) );
        const int              minDistance = readInt( aStep, "min_distance" );
        const int              stop = readInt( aStep, "stop" );
        const bool             hasExact = aStep.contains( "exact" );
        const bool             exact = readBool( aStep, "exact", true );
        guardShapeBBox( *shape );
        const BOX2I   ref = shape->BBox();
        const int64_t minDistSq = int64_t( minDistance ) * minDistance;
        int           calls = 0;
        for( const ENTRY& e : aEntries )
        {
            if( guardSquaredDistance( ref, e.bbox ) <= minDistSq )
            {
                if( exact )
                    collideShapeDomain( *e.shape, shape.get(), minDistance );
                if( !exact || e.shape->Collide( shape.get(), minDistance ) )
                {
                    if( ++calls >= stop )
                        break;
                }
            }
        }
        JSON    out = JSON::array();
        VISITOR visitor{ &out, 0, stop };
        const int n = hasExact ? aList.Query( shape.get(), minDistance, visitor, exact )
                               : aList.Query( shape.get(), minDistance, visitor );
        return JSON{ { "found", n }, { "visits", out } };
    }
    if( name == "iterate" )
    {
        checkKeys( aStep, { "call", "ops" } );
        LIST::iterator it = aList.begin();
        const LIST::iterator end = aList.end();
        const LIST::iterator null;
        // Positions: begin() and end() of a list that never allocated are null pointers.
        size_t pos = 0;
        JSON   out = JSON::array();
        for( const JSON& op : readArray( aStep, "ops", STEPS ) )
        {
            if( !op.is_string() )
                throw std::invalid_argument( "expected op" );
            const std::string o = op.get<std::string>();
            if( o == "deref" || o == "inc" || o == "inc_post" )
            {
                if( pos >= aEntries.size() )
                    undefined();
                if( o == "deref" )
                    out.push_back( wire( *it ) );
                else
                {
                    if( o == "inc" )
                        ++it;
                    else
                        it++;
                    ++pos;
                    out.push_back( nullptr );
                }
            }
            else if( o == "eq_end" )
                out.push_back( it == end );
            else if( o == "ne_end" )
                out.push_back( it != end );
            else if( o == "eq_null" )
                out.push_back( it == null );
            else if( o == "copy" )
            {
                LIST::iterator copy( it );
                LIST::iterator assigned;
                assigned = copy;
                it = assigned;
                out.push_back( nullptr );
            }
            else
                throw std::invalid_argument( "unknown op" );
        }
        return out;
    }
    if( name == "qiterate" )
    {
        checkKeys( aStep, { "call", "shape", "min_distance", "exact", "ops" } );
        std::unique_ptr<SHAPE> shape;
        if( !aStep.at( "shape" ).is_null() )
            shape = buildShape( aStep.at( "shape" ) );
        const int  minDistance = readInt( aStep, "min_distance" );
        const bool exact = readBool( aStep, "exact" );
        BOX2I      ref;
        size_t     pos = 0;
        if( shape )
        {
            guardShapeBBox( *shape );
            ref = shape->BBox();
            pos = guardNext( aEntries, 0, ref, shape.get(), minDistance, exact );
        }
        LIST::query_iterator       it = aList.qbegin( shape.get(), minDistance, exact );
        const LIST::query_iterator end = aList.qend();
        JSON                       out = JSON::array();
        for( const JSON& op : readArray( aStep, "ops", STEPS ) )
        {
            if( !op.is_string() )
                throw std::invalid_argument( "expected op" );
            const std::string o = op.get<std::string>();
            if( o == "deref" )
            {
                if( pos >= aEntries.size() )
                    undefined();
                out.push_back( wire( *it ) );
            }
            else if( o == "inc" || o == "inc_post" )
            {
                if( pos >= aEntries.size() )
                    undefined();
                pos = guardNext( aEntries, pos + 1, ref, shape.get(), minDistance, exact );
                if( o == "inc" )
                    ++it;
                else
                    it++;
                out.push_back( nullptr );
            }
            else if( o == "eq_end" )
                out.push_back( it == end );
            else if( o == "ne_end" )
                out.push_back( it != end );
            else if( o == "copy" )
            {
                LIST::query_iterator copy( it );
                LIST::query_iterator assigned( end );
                assigned = copy;
                it = assigned;
                out.push_back( nullptr );
            }
            else if( o == "copy_null" )
            {
                // A default-constructed query_iterator leaves m_exact, m_shape and m_minDistance
                // uninitialised; its copy constructor reads them.
                undefined();
            }
            else
                throw std::invalid_argument( "unknown op" );
        }
        return out;
    }
    throw std::invalid_argument( "unknown call" );
}


JSON runList( const JSON& aInput )
{
    checkKeys( aInput, { "op", "items", "steps" } );
    std::vector<std::unique_ptr<ITEM>> items = readItems( aInput );
    std::array<LIST_STATE, LISTS>      lists;
    JSON                               replies = JSON::array();
    for( const JSON& step : readArray( aInput, "steps", STEPS ) )
    {
        if( readText( step, "call" ) == "compare" )
        {
            replies.push_back( compareIterators( lists, step ) );
            continue;
        }
        // Every other call acts on list "l" (0 when absent).
        int  l = 0;
        JSON call = step;
        if( step.is_object() && step.contains( "l" ) )
        {
            l = readSlot( step, "l", LISTS );
            call.erase( "l" );
        }
        replies.push_back( listCall( lists[l].list, lists[l].entries, items, call ) );
    }
    return JSON{ { "replies", replies } };
}


// ---------------------------------------------------------------------------------------------

JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "clipper_z_value" )
    {
        checkKeys( aInput, { "op" }, { "indices", "offset" } );
        CLIPPER_Z_VALUE z;
        if( aInput.contains( "indices" ) )
        {
            const JSON& v = aInput.at( "indices" );
            if( !v.is_array() || v.size() != 2 )
                throw std::invalid_argument( "expected indices" );
            const std::pair<ssize_t, ssize_t> indices( readI64( v[0] ), readI64( v[1] ) );
            const bool    hasOffset = aInput.contains( "offset" );
            const int64_t offset = hasOffset ? readI64( aInput, "offset" ) : 0;
            for( int64_t index : { indices.first, indices.second } )
                if( index >= 0 )
                    L( __int128( index ) + offset );
            z = hasOffset ? CLIPPER_Z_VALUE( indices, offset ) : CLIPPER_Z_VALUE( indices );
        }
        else if( aInput.contains( "offset" ) )
            throw std::invalid_argument( "unexpected or missing field" );
        return JSON{ { "first", z.m_FirstArcIdx }, { "second", z.m_SecondArcIdx } };
    }
    if( op == "intersection" )
    {
        checkKeys( aInput, { "op" } );
        const SHAPE_LINE_CHAIN::INTERSECTION i;
        return JSON{ { "p", point( i.p ) },
                     { "index_our", i.index_our },
                     { "index_their", i.index_their },
                     { "is_corner_our", i.is_corner_our },
                     { "is_corner_their", i.is_corner_their },
                     { "valid", i.valid } };
    }
    if( op == "compare_origin_distance" )
    {
        checkKeys( aInput, { "op", "origin", "a", "b" } );
        SHAPE_LINE_CHAIN::INTERSECTION a, b;
        const VECTOR2I origin = readPoint( aInput, "origin" );
        a.p = readPoint( aInput, "a" );
        b.p = readPoint( aInput, "b" );
        subI( origin, a.p );
        subI( origin, b.p );
        SHAPE_LINE_CHAIN::compareOriginDistance cmp( origin );
        return cmp( a, b );
    }
    if( op == "chain" )
        return runChain( aInput );
    if( op == "index" )
        return runIndex( aInput );
    if( op == "index_list" )
        return runList( aInput );
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterShapeLineChainHeaders( REGISTRY& aRegistry )
{
    aRegistry.Add( P004_UNIT, "shape_line_chain.h, shape_index.h, shape_index_list.h", run );
}
} // namespace oracle
