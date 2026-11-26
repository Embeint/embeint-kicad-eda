// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned KIGEOM arc construction helpers (libs/kimath/src/geometry/arc_construction.cpp).
// The adapter only decodes 32-bit integer points, exact IEEE tangent bits and the major flag, and
// encodes every ARC_SOLUTION field (doubles as exact IEEE bits); it never computes a reply itself.
#include "registry.h"
#include <geometry/arc_construction.h>
#include <bit>
#include <climits>
#include <cstdint>
#include <iomanip>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>

namespace
{
using oracle::JSON;
constexpr const char* ARC_CONSTRUCTION_UNIT = "libs.kimath.src.geometry.arc_construction.application.ui.p001";


void checkKeys( const JSON& aInput, std::initializer_list<const char*> aFields )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );
    if( aInput.size() != aFields.size() )
        throw std::invalid_argument( "unexpected or missing field" );
    for( const char* field : aFields )
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
}


std::string readText( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );
    return aInput.at( aName ).get<std::string>();
}


bool readFlag( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_boolean() )
        throw std::invalid_argument( std::string( "expected boolean: " ) + aName );
    return aInput.at( aName ).get<bool>();
}


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


JSON intPoint( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


VECTOR2I readPoint( const JSON& aInput, const char* aName )
{
    const JSON& v = aInput.at( aName );
    if( !v.is_array() || v.size() != 2 )
        throw std::invalid_argument( "expected point" );
    int values[2];
    for( int i = 0; i < 2; ++i )
    {
        if( !v[i].is_number_integer() || ( v[i].is_number_unsigned() && v[i].get<uint64_t>() > INT_MAX )
            || v[i].get<int64_t>() < INT_MIN || v[i].get<int64_t>() > INT_MAX )
            throw std::invalid_argument( "point out of range" );
        values[i] = v[i].get<int>();
    }
    return VECTOR2I( values[0], values[1] );
}


// A double given as exactly 16 lower-case hexadecimal digits of its IEEE bits.
double readDoubleBits( const JSON& aInput )
{
    if( !aInput.is_string() )
        throw std::invalid_argument( "expected double bits" );
    const std::string s = aInput.get<std::string>();
    if( s.size() != 16 )
        throw std::invalid_argument( "expected double bits" );
    uint64_t value = 0;
    for( char c : s )
    {
        int digit;
        if( c >= '0' && c <= '9' )
            digit = c - '0';
        else if( c >= 'a' && c <= 'f' )
            digit = c - 'a' + 10;
        else
            throw std::invalid_argument( "expected double bits" );
        value = ( value << 4 ) | static_cast<uint64_t>( digit );
    }
    return std::bit_cast<double>( value );
}


VECTOR2D readDoubleVector( const JSON& aInput, const char* aName )
{
    const JSON& v = aInput.at( aName );
    if( !v.is_array() || v.size() != 2 )
        throw std::invalid_argument( "expected vector" );
    const double x = readDoubleBits( v[0] );
    const double y = readDoubleBits( v[1] );
    return VECTOR2D( x, y );
}


JSON solution( const KIGEOM::ARC_SOLUTION& aSol )
{
    JSON out = JSON::object();
    out["valid"] = aSol.valid;
    out["start"] = intPoint( aSol.start );
    out["mid"] = intPoint( aSol.mid );
    out["end"] = intPoint( aSol.end );
    out["center"] = JSON::array( { bits( aSol.center.x ), bits( aSol.center.y ) } );
    out["radius"] = bits( aSol.radius );
    return out;
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    // Points are decoded in field order, each before the next, as the Rust adapter does.
    if( op == "project" )
    {
        checkKeys( aInput, { "op", "start", "end", "cursor" } );
        const VECTOR2I start = readPoint( aInput, "start" );
        const VECTOR2I end = readPoint( aInput, "end" );
        const VECTOR2I cursor = readPoint( aInput, "cursor" );
        return intPoint( KIGEOM::ProjectToChordBisector( start, end, cursor ) );
    }
    if( op == "through" )
    {
        checkKeys( aInput, { "op", "start", "on_arc", "end" } );
        const VECTOR2I start = readPoint( aInput, "start" );
        const VECTOR2I onArc = readPoint( aInput, "on_arc" );
        const VECTOR2I end = readPoint( aInput, "end" );
        return solution( KIGEOM::ArcThroughPoints( start, onArc, end ) );
    }
    if( op == "mid_drag" )
    {
        checkKeys( aInput, { "op", "start", "end", "cursor" } );
        const VECTOR2I start = readPoint( aInput, "start" );
        const VECTOR2I end = readPoint( aInput, "end" );
        const VECTOR2I cursor = readPoint( aInput, "cursor" );
        return solution( KIGEOM::ArcFromStartEndMidDrag( start, end, cursor ) );
    }
    if( op == "center_drag" )
    {
        checkKeys( aInput, { "op", "start", "end", "cursor", "major" } );
        const VECTOR2I start = readPoint( aInput, "start" );
        const VECTOR2I end = readPoint( aInput, "end" );
        const VECTOR2I cursor = readPoint( aInput, "cursor" );
        const bool     major = readFlag( aInput, "major" );
        return solution( KIGEOM::ArcFromStartEndCenterDrag( start, end, cursor, major ) );
    }
    if( op == "tangent" )
    {
        checkKeys( aInput, { "op", "start", "tangent", "end" } );
        const VECTOR2I start = readPoint( aInput, "start" );
        const VECTOR2D tangent = readDoubleVector( aInput, "tangent" );
        const VECTOR2I end = readPoint( aInput, "end" );
        return solution( KIGEOM::ArcFromStartTangentEnd( start, tangent, end ) );
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterArcConstruction( REGISTRY& aRegistry )
{
    aRegistry.Add( ARC_CONSTRUCTION_UNIT, "geometry/arc_construction.cpp", run );
}
} // namespace oracle
