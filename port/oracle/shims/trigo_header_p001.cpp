// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned trigo.h (libs/kimath/include/trigo.h): its inline bodies, compiled unchanged into
// this adapter from the pinned header, calling the genuine trigo.cpp overloads in the kimath
// archive.
//
// The adapter decodes 32-bit integer points, 64-bit integer angles and IEEE bit strings, calls the
// pinned inline functions and encodes every observable value exactly; it never computes a reply
// itself. Before a VECTOR2I rotation a guard walks the pinned integer evaluation of the trigo.cpp
// overload the wrapper forwards to and refuses the request when it would reach undefined
// behaviour (the negation of INT_MIN in the 90/180/270 degree shortcuts, an int difference or sum
// that overflows). The double bodies, InterceptsPositiveX/NegativeX and the angle templates are
// defined for every input: the template loops never overflow and always terminate.
//
// Harness bound (not an undefined domain): GCC 13.3 compiles the NormalizeAnglePos and
// NormalizeAngle180 loops as loops, one pass per 3600 (unrolled twice), so a long long near its
// limits would run for about 10^15 iterations. A long long angle whose magnitude exceeds
// 3600 * 2^24 is refused as "outside the verified domain" before the call; every int, and every
// long long within the bound, is evaluated.
#include "registry.h"
#include <geometry/eda_angle.h>
#include <math/vector2d.h>
#include <trigo.h>
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
constexpr const char* TRIGO_HEADER_UNIT = "libs.kimath.include.application.ui.p001";

// The largest long long angle magnitude the adapter evaluates: at most 2^24 loop passes.
constexpr long long LOOP_BOUND = 3600LL << 24;

[[noreturn]] void undefined()
{
    throw std::invalid_argument( "undefined trigo.h operation" );
}


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


int readInt( const JSON& aValue )
{
    if( !aValue.is_number_integer() || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > INT_MAX )
        || aValue.get<int64_t>() < INT_MIN || aValue.get<int64_t>() > INT_MAX )
        throw std::invalid_argument( "integer out of range" );
    return aValue.get<int>();
}


long long readLongLong( const JSON& aValue )
{
    if( !aValue.is_number_integer()
        || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > uint64_t( LLONG_MAX ) ) )
        throw std::invalid_argument( "integer out of range" );
    return aValue.get<long long>();
}


double decodeBits( const JSON& aValue )
{
    if( !aValue.is_string() )
        throw std::invalid_argument( "expected hex" );
    const std::string text = aValue.get<std::string>();
    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        throw std::invalid_argument( "invalid hex" );
    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


VECTOR2I readPointI( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    return VECTOR2I( readInt( aValue[0] ), readInt( aValue[1] ) );
}


VECTOR2D readPointD( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    return VECTOR2D( decodeBits( aValue[0] ), decodeBits( aValue[1] ) );
}


// An angle is given as the exact bits of its value in degrees.
EDA_ANGLE readAngle( const JSON& aValue )
{
    return EDA_ANGLE( decodeBits( aValue ), DEGREES_T );
}


JSON pointI( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


JSON pointD( const VECTOR2D& aValue )
{
    return JSON::array( { bits( aValue.x ), bits( aValue.y ) } );
}


int I( int64_t aValue )
{
    if( aValue < INT_MIN || aValue > INT_MAX )
        undefined();
    return static_cast<int>( aValue );
}


// RotatePoint( int*, int*, const EDA_ANGLE& ) (trigo.cpp): the negations of the 90/180/270
// degree branches. The general branch is double arithmetic and KiROUND, which saturates.
void guardRotate( int aX, int aY, const EDA_ANGLE& aAngle )
{
    EDA_ANGLE angle = aAngle;
    angle.Normalize();

    if( ( angle == ANGLE_90 || angle == ANGLE_180 ) && aX == INT_MIN )
        undefined();
    if( ( angle == ANGLE_180 || angle == ANGLE_270 ) && aY == INT_MIN )
        undefined();
}


// RotatePoint( int*, int*, int cx, int cy, const EDA_ANGLE& ) (trigo.cpp): both differences, the
// rotation (the genuine function, called after its own guard) and both sums.
void guardRotateAbout( int aX, int aY, int aCx, int aCy, const EDA_ANGLE& aAngle )
{
    int ox = I( int64_t( aX ) - aCx );
    int oy = I( int64_t( aY ) - aCy );

    guardRotate( ox, oy, aAngle );
    RotatePoint( &ox, &oy, aAngle );
    I( int64_t( ox ) + aCx );
    I( int64_t( oy ) + aCy );
}


// The template's integer type: "int" or "long long".
bool readWide( const JSON& aInput )
{
    const std::string type = readText( aInput, "type" );

    if( type == "int" )
        return false;
    if( type == "long long" )
        return true;
    throw std::invalid_argument( "unknown type" );
}


long long boundedLongLong( const JSON& aValue )
{
    const long long angle = readLongLong( aValue );

    if( angle > LOOP_BOUND || angle < -LOOP_BOUND )
        throw std::invalid_argument( "outside the verified domain" );
    return angle;
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "rotate_i" || op == "get_rotated_i" )
    {
        checkKeys( aInput, { "op", "point", "angle" } );
        VECTOR2I        p = readPointI( aInput.at( "point" ) );
        const EDA_ANGLE angle = readAngle( aInput.at( "angle" ) );
        guardRotate( p.x, p.y, angle );

        if( op == "get_rotated_i" )
            return JSON{ { "point", pointI( GetRotated( p, angle ) ) } };

        RotatePoint( p, angle );
        return JSON{ { "point", pointI( p ) } };
    }
    if( op == "rotate_i_about" || op == "get_rotated_i_about" )
    {
        checkKeys( aInput, { "op", "point", "center", "angle" } );
        VECTOR2I        p = readPointI( aInput.at( "point" ) );
        const VECTOR2I  c = readPointI( aInput.at( "center" ) );
        const EDA_ANGLE angle = readAngle( aInput.at( "angle" ) );
        guardRotateAbout( p.x, p.y, c.x, c.y, angle );

        if( op == "get_rotated_i_about" )
            return JSON{ { "point", pointI( GetRotated( p, c, angle ) ) } };

        RotatePoint( p, c, angle );
        return JSON{ { "point", pointI( p ) } };
    }
    if( op == "rotate_d" || op == "get_rotated_d" )
    {
        // Double arithmetic only: no undefined operation.
        checkKeys( aInput, { "op", "point", "angle" } );
        VECTOR2D        p = readPointD( aInput.at( "point" ) );
        const EDA_ANGLE angle = readAngle( aInput.at( "angle" ) );

        if( op == "get_rotated_d" )
            return JSON{ { "point", pointD( GetRotated( p, angle ) ) } };

        RotatePoint( p, angle );
        return JSON{ { "point", pointD( p ) } };
    }
    if( op == "rotate_d_about" )
    {
        checkKeys( aInput, { "op", "point", "center", "angle" } );
        VECTOR2D       p = readPointD( aInput.at( "point" ) );
        const VECTOR2D c = readPointD( aInput.at( "center" ) );
        RotatePoint( p, c, readAngle( aInput.at( "angle" ) ) );
        return JSON{ { "point", pointD( p ) } };
    }
    if( op == "deg2rad" || op == "rad2deg" || op == "rad2decideg" )
    {
        checkKeys( aInput, { "op", "value" } );
        const double value = decodeBits( aInput.at( "value" ) );

        if( op == "deg2rad" )
            return JSON{ { "value", bits( DEG2RAD( value ) ) } };
        if( op == "rad2deg" )
            return JSON{ { "value", bits( RAD2DEG( value ) ) } };
        return JSON{ { "value", bits( RAD2DECIDEG( value ) ) } };
    }
    if( op == "normalize_angle_pos" || op == "normalize_angle_pos_ref" || op == "normalize_angle_180" )
    {
        checkKeys( aInput, { "op", "type", "angle" } );

        if( readWide( aInput ) )
        {
            long long angle = boundedLongLong( aInput.at( "angle" ) );

            if( op == "normalize_angle_pos" )
                return JSON{ { "angle", NormalizeAnglePos( angle ) } };
            if( op == "normalize_angle_180" )
                return JSON{ { "angle", NormalizeAngle180( angle ) } };
            NORMALIZE_ANGLE_POS( angle );
            return JSON{ { "angle", angle } };
        }

        int angle = readInt( aInput.at( "angle" ) );

        if( op == "normalize_angle_pos" )
            return JSON{ { "angle", NormalizeAnglePos( angle ) } };
        if( op == "normalize_angle_180" )
            return JSON{ { "angle", NormalizeAngle180( angle ) } };
        NORMALIZE_ANGLE_POS( angle );
        return JSON{ { "angle", angle } };
    }
    if( op == "intercepts_positive_x" || op == "intercepts_negative_x" )
    {
        checkKeys( aInput, { "op", "start", "end" } );
        const double start = decodeBits( aInput.at( "start" ) );
        const double end = decodeBits( aInput.at( "end" ) );

        if( op == "intercepts_positive_x" )
            return JSON{ { "value", InterceptsPositiveX( start, end ) } };
        return JSON{ { "value", InterceptsNegativeX( start, end ) } };
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterTrigoHeader( REGISTRY& aRegistry )
{
    aRegistry.Add( TRIGO_HEADER_UNIT, "trigo.h", run );
}
} // namespace oracle
