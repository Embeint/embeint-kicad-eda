// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned TRANSFORM bodies (libs/kimath/src/transform.cpp).
// The adapter only decodes integer matrices, points, boxes and exact IEEE angle bits and
// encodes the returned values. It never computes a reply itself: inputs whose pinned
// evaluation is undefined (signed overflow, division by zero) are refused before the
// reference body is called, with the classification the Rust port reports.
#include "registry.h"
#include <transform.h>
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
constexpr const char* TRANSFORM_UNIT = "libs.kimath.src.transform.application.ui.p001";
constexpr const char* UNDEFINED_OVERFLOW = "undefined integer overflow";
constexpr const char* UNDEFINED_DIVISION = "undefined division by zero";


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


int64_t readInteger( const JSON& aValue, int64_t aMin, int64_t aMax, const char* aName )
{
    if( !aValue.is_number_integer()
        || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > static_cast<uint64_t>( aMax ) ) )
        throw std::invalid_argument( std::string( "integer out of range: " ) + aName );
    const int64_t value = aValue.get<int64_t>();
    if( value < aMin || value > aMax )
        throw std::invalid_argument( std::string( "integer out of range: " ) + aName );
    return value;
}


// A fixed-length array of integers in [aMin, aMax].
void readIntegers( const JSON& aInput, const char* aName, size_t aCount, int64_t aMin,
                   int64_t aMax, int64_t* aOut )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array()
        || aInput.at( aName ).size() != aCount )
        throw std::invalid_argument( std::string( "expected integer array: " ) + aName );
    for( size_t i = 0; i < aCount; ++i )
        aOut[i] = readInteger( aInput.at( aName )[i], aMin, aMax, aName );
}


TRANSFORM readTransform( const JSON& aInput, const char* aName )
{
    int64_t v[4];
    readIntegers( aInput, aName, 4, INT_MIN, INT_MAX, v );
    return TRANSFORM( int( v[0] ), int( v[1] ), int( v[2] ), int( v[3] ) );
}


VECTOR2I readPoint( const JSON& aInput, const char* aName )
{
    int64_t v[2];
    readIntegers( aInput, aName, 2, INT_MIN, INT_MAX, v );
    return VECTOR2I( int( v[0] ), int( v[1] ) );
}


// Doubles travel as 16 lower-case hex digits of their IEEE bits.
double readNumber( const JSON& aInput, const char* aName )
{
    const std::string text = readText( aInput, aName );
    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        throw std::invalid_argument( std::string( "invalid hex: " ) + aName );
    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


JSON point( const VECTOR2I& aPoint )
{
    return JSON::array( { aPoint.x, aPoint.y } );
}


int checkedMul( int a, int b )
{
    int r;
    if( __builtin_mul_overflow( a, b, &r ) )
        throw std::invalid_argument( UNDEFINED_OVERFLOW );
    return r;
}


int checkedAdd( int a, int b )
{
    int r;
    if( __builtin_add_overflow( a, b, &r ) )
        throw std::invalid_argument( UNDEFINED_OVERFLOW );
    return r;
}


// Refuses a point whose TransformCoordinate would overflow int (every product and sum).
void guardPoint( const TRANSFORM& t, const VECTOR2I& p )
{
    checkedAdd( checkedMul( t.x1, p.x ), checkedMul( t.y1, p.y ) );
    checkedAdd( checkedMul( t.x2, p.x ), checkedMul( t.y2, p.y ) );
}


// Refusal order matches the Rust body: determinant products and difference, then each
// quotient in source order (zero divisor, negation of INT_MIN, INT_MIN / -1).
void guardInverse( const TRANSFORM& t )
{
    int det;
    if( __builtin_sub_overflow( checkedMul( t.x1, t.y2 ), checkedMul( t.x2, t.y1 ), &det ) )
        throw std::invalid_argument( UNDEFINED_OVERFLOW );
    auto quotient = [det]( int aNumerator )
    {
        if( det == 0 )
            throw std::invalid_argument( UNDEFINED_DIVISION );
        if( aNumerator == INT_MIN && det == -1 )
            throw std::invalid_argument( UNDEFINED_OVERFLOW );
    };
    auto negated = []( int aValue )
    {
        if( aValue == INT_MIN )
            throw std::invalid_argument( UNDEFINED_OVERFLOW );
        return -aValue;
    };
    quotient( t.y2 );
    quotient( negated( t.x2 ) );
    quotient( negated( t.y1 ) );
    quotient( t.x1 );
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "eq" )
    {
        checkKeys( aInput, { "op", "a", "b" } );
        const TRANSFORM a = readTransform( aInput, "a" );
        const TRANSFORM b = readTransform( aInput, "b" );
        return a == b;
    }
    if( op == "hash" )
    {
        checkKeys( aInput, { "op", "t" } );
        const TRANSFORM t = readTransform( aInput, "t" );
        return std::to_string( std::hash<TRANSFORM>{}( t ) );
    }
    if( op == "point" )
    {
        checkKeys( aInput, { "op", "t", "p" } );
        const TRANSFORM t = readTransform( aInput, "t" );
        const VECTOR2I  p = readPoint( aInput, "p" );
        guardPoint( t, p );
        return point( t.TransformCoordinate( p ) );
    }
    if( op == "box" )
    {
        checkKeys( aInput, { "op", "t", "origin", "size" } );
        const TRANSFORM t = readTransform( aInput, "t" );
        const VECTOR2I  origin = readPoint( aInput, "origin" );
        int64_t         size[2];
        readIntegers( aInput, "size", 2, INT64_MIN, INT64_MAX, size );
        BOX2I rect;
        rect.SetOrigin( origin );
        rect.SetSize( VECTOR2L( size[0], size[1] ) );

        // Source order: the origin is transformed, then GetEnd adds the int64_t size (an
        // overflow there is undefined) and the narrowed end is transformed.
        guardPoint( t, origin );
        int64_t right, bottom;
        if( __builtin_add_overflow( int64_t( origin.x ), size[0], &right )
            || __builtin_add_overflow( int64_t( origin.y ), size[1], &bottom ) )
            throw std::invalid_argument( UNDEFINED_OVERFLOW );
        guardPoint( t, rect.GetEnd() );

        const BOX2I out = t.TransformCoordinate( rect );
        return JSON{ { "origin", point( out.GetOrigin() ) },
                     { "size", JSON::array( { out.GetSize().x, out.GetSize().y } ) },
                     { "valid", out.IsValid() } };
    }
    if( op == "inverse" )
    {
        checkKeys( aInput, { "op", "t" } );
        const TRANSFORM t = readTransform( aInput, "t" );
        guardInverse( t );
        const TRANSFORM inv = t.InverseTransform();
        return JSON::array( { inv.x1, inv.y1, inv.x2, inv.y2 } );
    }
    if( op == "map_angles" )
    {
        // Both pointers are always valid here; the wxCHECK_MSG null branch is not reachable
        // from the Rust reference signature and is not exercised.
        checkKeys( aInput, { "op", "t", "a1", "a2" } );
        const TRANSFORM t = readTransform( aInput, "t" );
        EDA_ANGLE       a1( readNumber( aInput, "a1" ), DEGREES_T );
        EDA_ANGLE       a2( readNumber( aInput, "a2" ), DEGREES_T );
        const bool      swap = t.MapAngles( &a1, &a2 );
        return JSON{ { "swap", swap }, { "a1", bits( a1.AsDegrees() ) },
                     { "a2", bits( a2.AsDegrees() ) } };
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterTransformSource( REGISTRY& aRegistry )
{
    aRegistry.Add( TRANSFORM_UNIT, "transform.cpp", run );
}
} // namespace oracle
