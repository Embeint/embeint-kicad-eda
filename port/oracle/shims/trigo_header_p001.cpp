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
// that overflows). The double bodies and InterceptsPositiveX/NegativeX are defined for every
// input, and so are the int and long long angle templates: their loops never overflow and always
// terminate. The double templates' loops round at every pass; they never terminate where a pass
// makes no progress (an infinite angle or a magnitude above 2^65, where fl( a +- 3600 ) == a),
// which is undefined ([intro.progress]) and refused after testing the first pass.
//
// Harness bound (not an undefined domain): GCC 13.3 compiles the NormalizeAnglePos and
// NormalizeAngle180 loops literally, one pass per 3600, so a long long near its limits would run
// for about 10^15 passes. An angle whose loops need more than PASS_BUDGET passes (counted exactly
// below) is refused as "outside the verified domain" before the call; every other angle is
// evaluated by the pinned templates.
#include "registry.h"
#include <geometry/eda_angle.h>
#include <math/vector2d.h>
#include <trigo.h>
#include <bit>
#include <climits>
#include <cmath>
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

// The most passes of the template loops (both loops together) the adapter runs.
constexpr __int128 PASS_BUDGET = __int128( 1 ) << 24;

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


// The template's type T.
enum class TEMPLATE_TYPE
{
    INT,
    LONG_LONG,
    DOUBLE
};


TEMPLATE_TYPE readType( const JSON& aInput )
{
    const std::string type = readText( aInput, "type" );

    if( type == "int" )
        return TEMPLATE_TYPE::INT;
    if( type == "long long" )
        return TEMPLATE_TYPE::LONG_LONG;
    if( type == "double" )
        return TEMPLATE_TYPE::DOUBLE;
    throw std::invalid_argument( "unknown type" );
}


// The passes of the template loops (both loops together) for the angle aFloor + g, where g in
// [0, 1) is nonzero exactly when aFraction. Each loop runs to the first value past its bound and
// every pass before the last changes the angle by exactly 3600 within the counted range, so each
// count is a quotient. NormalizeAnglePos: ceil( -a / 3600 ) for a < 0, floor( a / 3600 ) for
// a >= 3600 (a double first loop whose last pass rounds up to 3600 adds one second-loop pass;
// that needs an angle above -2^11, a single pass, so it never decides admission and is not
// counted). NormalizeAngle180: floor( ( -1800 - a ) / 3600 ) + 1 for a <= -1800,
// ceil( ( a - 1800 ) / 3600 ) for a > 1800.
__int128 templatePasses( const std::string& aOp, __int128 aFloor, bool aFraction )
{
    auto q = []( __int128 a ) -> __int128
    {
        return a >= 0 ? a / 3600 : -( ( -a + 3599 ) / 3600 );
    };
    const __int128 f = aFraction ? 1 : 0;

    if( aOp == "normalize_angle_180" )
    {
        if( aFloor < -1800 || ( aFloor == -1800 && !aFraction ) )
            return q( -1800 - aFloor - f ) + 1;
        if( aFloor > 1800 || ( aFloor == 1800 && aFraction ) )
            return aFraction ? q( aFloor - 1800 ) + 1 : -q( 1800 - aFloor );
        return 0;
    }

    if( aFloor < 0 )
        return aFraction ? q( -aFloor - 1 ) + 1 : -q( aFloor );
    if( aFloor >= 3600 )
        return q( aFloor );
    return 0;
}


void checkBudget( const std::string& aOp, long long aAngle )
{
    if( templatePasses( aOp, aAngle, false ) > PASS_BUDGET )
        throw std::invalid_argument( "outside the verified domain" );
}


// A double angle: undefined where a loop never terminates, and outside the budget like an
// integer one. The first pass decides termination: fl( a +- 3600 ) == a for |a| > 2^65 or an
// infinite a, and from every other angle each pass progresses (by at least 3584).
void checkDouble( const std::string& aOp, double aAngle )
{
    const bool half = aOp == "normalize_angle_180";

    if( ( half ? aAngle <= -1800 : aAngle < 0 ) && aAngle + 3600 == aAngle )
        undefined();
    if( ( half ? aAngle > 1800 : aAngle >= 3600 ) && aAngle - 3600 == aAngle )
        undefined();
    if( std::isnan( aAngle ) )
        return;

    // Up to 2^57 every pass changes the angle by exactly 3600, so beyond 2^40 the loops need
    // more than 2^24 passes.
    if( !( std::fabs( aAngle ) <= 0x1p40 ) )
        throw std::invalid_argument( "outside the verified domain" );

    const double whole = std::floor( aAngle );

    if( templatePasses( aOp, static_cast<long long>( whole ), whole != aAngle ) > PASS_BUDGET )
        throw std::invalid_argument( "outside the verified domain" );
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
        const TEMPLATE_TYPE type = readType( aInput );

        if( type == TEMPLATE_TYPE::DOUBLE )
        {
            double angle = decodeBits( aInput.at( "angle" ) );
            checkDouble( op, angle );

            if( op == "normalize_angle_pos" )
                return JSON{ { "angle", bits( NormalizeAnglePos( angle ) ) } };
            if( op == "normalize_angle_180" )
                return JSON{ { "angle", bits( NormalizeAngle180( angle ) ) } };
            NORMALIZE_ANGLE_POS( angle );
            return JSON{ { "angle", bits( angle ) } };
        }

        if( type == TEMPLATE_TYPE::LONG_LONG )
        {
            long long angle = readLongLong( aInput.at( "angle" ) );
            checkBudget( op, angle );

            if( op == "normalize_angle_pos" )
                return JSON{ { "angle", NormalizeAnglePos( angle ) } };
            if( op == "normalize_angle_180" )
                return JSON{ { "angle", NormalizeAngle180( angle ) } };
            NORMALIZE_ANGLE_POS( angle );
            return JSON{ { "angle", angle } };
        }

        int angle = readInt( aInput.at( "angle" ) );
        checkBudget( op, angle );

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
